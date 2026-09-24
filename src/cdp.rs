//! Minimal synchronous Chrome DevTools Protocol client over one websocket.
//!
//! Single-threaded pump design: every wait (command response, event, sleep, or
//! an idle gap between ops) drains incoming messages, so frames keep flowing and
//! get acked no matter what the caller is doing. The socket's read timeout is
//! set from the remaining budget of whatever wait is in progress, so the wait
//! ends when the caller asked it to rather than when the socket felt like it.

use base64::Engine;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Longest a single socket read may block. Waits are built out of slices of this
/// length, so a wait never overshoots its own deadline by more than one slice.
const PUMP_SLICE: Duration = Duration::from_millis(5);

/// `set_read_timeout(0)` means "block forever" on a BSD socket and is rejected
/// outright by std, so a slice can never be shorter than this.
const PUMP_SLICE_MIN: Duration = Duration::from_millis(1);

/// Slowest the screenshot pump will back off to when the browser cannot keep up.
///
/// 120ms, not the 250ms this used to be. The interval is measured from when a shot was SENT
/// and only one is ever in flight, so a slow capture already paces itself: the ceiling only
/// bites as idle time added after a reply lands, and after an abandoned shot. At 250ms that
/// idle time was up to seven and a half output frames of held picture, which reads as a
/// stutter rather than as a slow take. 120ms is under four, and because a second capture can
/// never be queued it still cannot pile work onto the page being filmed.
const SHOT_INTERVAL_MAX: Duration = Duration::from_millis(120);

/// How long an unanswered `Page.captureScreenshot` is waited for before the pump
/// writes it off and takes a new one. Only reached when the target is wedged.
const SHOT_ABANDON: Duration = Duration::from_secs(5);

/// Default ceiling on the frame spool. At `--scale 2` a busy take writes on the
/// order of 20MB/s, so an unbounded spool fills the temp filesystem in minutes.
pub const SPOOL_DEFAULT_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// A take will not start with less free space than this where the spool lives.
const SPOOL_MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

/// Headroom left for everything else on the filesystem when the spool limit is
/// clamped down to what is actually free.
const SPOOL_FREE_MARGIN_BYTES: u64 = 256 * 1024 * 1024;

/// Set by SIGINT/SIGTERM once `install_signal_handlers` has run.
///
/// Every cleanup kaviri does lives in a `Drop`, and a default signal disposition
/// runs none of them: the headless browser, its profile directory and the frame
/// spool would all be orphaned. The handler flips this instead, and the op loops
/// return so the normal teardown path runs.
pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// True once a SIGINT or SIGTERM has been seen. Callers check it between ops.
pub fn shutting_down() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

/// Install the SIGINT/SIGTERM handlers. Idempotent, and safe to call before
/// anything else exists.
///
/// The second signal is not graceful: someone pressing ctrl-C twice wants out
/// now, and by then the browser may well be the thing that is stuck.
#[allow(dead_code)]
pub fn install_signal_handlers() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            // SAFETY: the handler only stores into an atomic and, on a repeat
            // signal, calls `_exit`, both of which are async-signal-safe.
            let r = unsafe {
                signal_hook::low_level::register(sig, move || {
                    if SHUTDOWN.swap(true, Ordering::SeqCst) {
                        signal_hook::low_level::exit(130);
                    }
                })
            };
            if let Err(e) = r {
                eprintln!("kaviri: could not install a handler for signal {sig}: {e}");
            }
        }
    });
}

/// A token no other user on the machine can guess.
///
/// `RandomState` is seeded by the OS, so hashing through it is the only source
/// of real randomness the standard library exposes without a dependency.
fn run_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    format!("{:016x}", h.finish())
}

/// Create a directory owned by this user alone, failing if the name is taken.
fn create_private_dir(dir: &Path) -> Result<(), String> {
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Prefixes a temp directory must carry before the sweep will consider removing it.
///
/// `lensa-` is the name this tool had before it was renamed. It is here because a machine
/// that ran the old binary still has those directories, and each one is a Chromium profile of
/// around 250MB. On the laptop this was written on there were eighteen of them holding 4.5GB,
/// which was enough to exhaust a per-user tmpfs quota and make every subsequent take die
/// mid-recording with a websocket reset. Removing the prefix means those are never cleaned up
/// by anything, so it stays until it is certain nobody has one.
const RUN_DIR_PREFIXES: [&str; 2] = ["kaviri-", "lensa-"];

/// How old a run directory has to be before it is removed without knowing whose it was.
///
/// Only reached for a directory whose name carries no parseable process id, which means the
/// old naming scheme. A week is far longer than any take and longer than any plausible
/// `kaviri serve` session, so this cannot delete work in progress.
const SWEEP_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Does this process exist?
///
/// `kill(pid, 0)` sends nothing and only asks. `EPERM` counts as alive: the process is there,
/// it just belongs to someone else, and a directory of ours it does not own is not evidence
/// that our run has ended.
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return true;
    }
    // SAFETY: kill with signal 0 performs no action and only checks for the process.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool {
    // No cheap equivalent, so nothing is ever swept on the strength of a dead process.
    true
}

/// Should this entry in the temp root be removed?
///
/// Pure, so the rules can be tested without creating and destroying real directories, which
/// is not a thing to get wrong in a function whose job is `remove_dir_all`.
fn is_stale_run_dir(
    name: &str,
    self_pid: u32,
    alive: &dyn Fn(u32) -> bool,
    age: Option<Duration>,
) -> bool {
    let Some(rest) = RUN_DIR_PREFIXES.iter().find_map(|p| name.strip_prefix(p)) else {
        return false;
    };

    /*
     * Every number anywhere in the name, not just the first segment. The names are not all
     * one shape: a session is `kaviri-<pid>-<token>`, but render scratch is
     * `kaviri-render-sc-<pid>-<token>` and the old scheme was `lensa-profile-<port>`. Taking
     * only the first segment found no pid in two of those three and left them to the age
     * rule, which is a week of holding disk for a run that ended a minute ago.
     *
     * The 16-hex-character run token cannot be mistaken for one: sixteen digits does not fit
     * in a u32, so it never parses.
     */
    let ids: Vec<u32> = rest
        .split('-')
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();

    // Ours. We are about to put a browser profile in it.
    if ids.contains(&self_pid) {
        return false;
    }
    /*
     * Something in the name is a process that exists, so leave it alone, and leave it alone
     * however old it is. Age was tried as a tiebreak here and it is the wrong call: the cost
     * of being wrong is deleting the profile out from under a `kaviri serve` session that an
     * agent has held open, and the cost of being conservative is one directory that survives
     * exactly as long as the process whose number is in its name. Pids are reused, so a
     * number that was never a pid gets collected the next time nothing is holding it.
     */
    if ids.iter().any(|&id| alive(id)) {
        return false;
    }
    // Every process named here is gone, so nothing is coming back for this.
    if !ids.is_empty() {
        return true;
    }
    // No number at all. Age is the only evidence there is.
    age.map(|a| a >= SWEEP_MAX_AGE).unwrap_or(false)
}

/// Remove run directories left behind by runs that are over.
///
/// kaviri cleans up in a `Drop`, which covers an ordinary exit and a signal, and covers
/// nothing at all when the process is killed or the browser takes it down with it. Every one
/// of those leaves a Chromium profile behind, and on a snap-packaged Chromium the profile has
/// to live under the temp root because the sandbox will not let it live anywhere else. They
/// accumulate silently until the filesystem or the user's quota runs out, and the symptom
/// then is not "out of space", it is takes that die part way through.
///
/// Nothing here is allowed to fail a run: a sweep that cannot read the temp root, or that
/// trips over one directory it may not remove, has not stopped anyone from recording.
fn sweep_stale_runs(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let self_pid = std::process::id();
    let now = SystemTime::now();
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // symlink_metadata, so a symlink named like a run directory is seen as a symlink and
        // skipped rather than followed somewhere it should not be followed.
        let Ok(meta) = entry.path().symlink_metadata() else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let age = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok());
        if !is_stale_run_dir(name, self_pid, &pid_alive, age) {
            continue;
        }
        if std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 && crate::env::is_set("DEBUG") {
        eprintln!(
            "kaviri[debug]: swept {removed} leftover run director{}",
            if removed == 1 { "y" } else { "ies" }
        );
    }
}

static SESSION_DIR: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// The one 0700 directory this run keeps its temporary state in.
///
/// The browser profile and the frame spool both live here. Putting them in one
/// owned directory means a single removal cleans the run up, two kaviri runs
/// sharing a temp directory cannot collide, and neither path is guessable by
/// another local user who might otherwise pre-create it as a symlink.
pub fn session_dir() -> Result<PathBuf, String> {
    SESSION_DIR
        .get_or_init(|| {
            let root = std::env::temp_dir();
            // Before taking a new one, give back the ones nobody is using. Once per process,
            // because this initialiser runs once, and that is the right cadence: it is a
            // directory listing, and it is the only moment kaviri is guaranteed to reach on
            // every entry point that records anything.
            sweep_stale_runs(&root);
            let mut last = String::new();
            for _ in 0..8 {
                let dir = root.join(format!("kaviri-{}-{}", std::process::id(), run_token()));
                match create_private_dir(&dir) {
                    Ok(()) => return Ok(dir),
                    Err(e) => last = e,
                }
            }
            Err(format!(
                "could not create a private temp directory under {}: {last}",
                root.display()
            ))
        })
        .clone()
}

/// Bytes available to this user on the filesystem holding `dir`.
// The statvfs field widths differ between platforms, so one of the two casts is
// redundant on any given target.
#[allow(clippy::unnecessary_cast)]
#[cfg(unix)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` is a zeroed statvfs of
    // the right size; statvfs only writes into it.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
}

#[cfg(not(unix))]
fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// One captured frame, as it sits in the spool: when it happened and where its JPEG
/// bytes live. The bytes themselves are never held in memory after the frame arrives.
pub struct Frame {
    /// Seconds on the recording clock (relative to `rec_t0`).
    pub t: f64,
    offset: u64,
    len: u32,
}

/// Captured frames, spooled to a file rather than accumulated in RAM.
///
/// A 1470x830 JPEG is 10-20KB, so a minute of busy capture is a few hundred megabytes
/// held live if the frames are kept in a `Vec`. That is fine for a fifteen second demo
/// and ruinous for anything longer, and it is the kind of limit that only shows up on
/// the take you cannot repeat. Frames go to one append-only file; the index carries the
/// timestamps, which is the only part the zoom planner needs to read.
pub struct FrameSpool {
    file: Option<std::fs::File>,
    path: PathBuf,
    frames: Vec<Frame>,
    bytes: u64,
    /// Where to open the next spool. `None` uses this run's session directory.
    dir: Option<PathBuf>,
    max_bytes: u64,
    /// Set once a push has been refused for want of room. The take stops here
    /// rather than dying on ENOSPC halfway through a write.
    full: bool,
    /// Keep the file on `discard`, printing where it is. Set by `--keep-temp` and
    /// automatically when a take ends abnormally, so the frames can be recovered.
    keep: bool,
}

impl FrameSpool {
    pub fn new() -> FrameSpool {
        FrameSpool {
            file: None,
            path: PathBuf::new(),
            frames: Vec::new(),
            bytes: 0,
            dir: crate::env::var_os("SPOOL_DIR").map(PathBuf::from),
            max_bytes: crate::env::var("MAX_SPOOL_BYTES")
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(SPOOL_DEFAULT_MAX_BYTES),
            full: false,
            keep: crate::env::is_set("KEEP_TEMP"),
        }
    }

    /// Where the spool file is written, and how large it may grow. Applies from
    /// the next `reset`, so it must be set before capture starts.
    pub fn configure(&mut self, dir: Option<PathBuf>, max_bytes: Option<u64>) {
        if let Some(d) = dir {
            self.dir = Some(d);
        }
        if let Some(m) = max_bytes.filter(|m| *m > 0) {
            self.max_bytes = m;
        }
    }

    /// Keep the spool file on teardown instead of deleting it, and say where it is.
    pub fn keep(&mut self, keep: bool) {
        self.keep = keep;
    }

    /// True once the spool hit its byte limit and stopped accepting frames.
    pub fn is_full(&self) -> bool {
        self.full
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Open a fresh spool. The old one, if any, is closed and removed unless it
    /// was marked to be kept.
    pub fn reset(&mut self) -> Result<(), String> {
        self.discard();
        let dir = match self.dir.clone() {
            Some(d) => {
                std::fs::create_dir_all(&d)
                    .map_err(|e| format!("create spool dir {}: {e}", d.display()))?;
                d
            }
            None => session_dir()?,
        };

        /*
         * Check the room before the take rather than after. A spool that runs out
         * of space mid-write costs the whole recording; a refusal costs nothing but
         * a re-run with --spool-dir somewhere roomier.
         */
        if let Some(free) = free_bytes(&dir) {
            if free < SPOOL_MIN_FREE_BYTES {
                return Err(format!(
                    "only {} MiB free on {}; kaviri needs at least {} MiB for the frame spool (use --spool-dir)",
                    free / (1024 * 1024),
                    dir.display(),
                    SPOOL_MIN_FREE_BYTES / (1024 * 1024)
                ));
            }
            let room = free.saturating_sub(SPOOL_FREE_MARGIN_BYTES);
            if room < self.max_bytes {
                eprintln!(
                    "kaviri: frame spool limited to {} MiB by free space on {}",
                    room / (1024 * 1024),
                    dir.display()
                );
                self.max_bytes = room;
            }
        }

        let (file, path) = loop {
            let path = dir.join(format!("spool-{}.jpgs", run_token()));
            let mut opts = std::fs::OpenOptions::new();
            opts.create_new(true).read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            match opts.open(&path) {
                Ok(f) => break (f, path),
                // A name collision is a fresh token away; anything else is real.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("open frame spool {}: {e}", path.display())),
            }
        };
        self.file = Some(file);
        self.path = path;
        self.full = false;
        Ok(())
    }

    fn push(&mut self, t: f64, jpeg: &[u8]) -> Result<(), String> {
        if self.bytes.saturating_add(jpeg.len() as u64) > self.max_bytes {
            self.full = true;
            return Ok(());
        }
        let bytes = self.bytes;
        let file = match self.file.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        if let Err(e) = file.write_all(jpeg) {
            /*
             * A short write leaves bytes on disk that no Frame indexes, which would
             * shift every later offset and hand the renderer garbage. Cut the file
             * back to the last complete frame so losing this one stays survivable.
             */
            let _ = file.set_len(bytes);
            let _ = file.seek(SeekFrom::Start(bytes));
            return Err(format!("spool write: {e}"));
        }
        self.frames.push(Frame {
            t,
            offset: bytes,
            len: jpeg.len() as u32,
        });
        self.bytes = bytes + jpeg.len() as u64;
        Ok(())
    }

    /// Read one frame's JPEG back off the spool.
    pub fn read(&self, i: usize) -> Result<Vec<u8>, String> {
        let frame = self.frames.get(i).ok_or("frame index out of range")?;
        let mut file = std::fs::File::open(&self.path).map_err(|e| format!("spool open: {e}"))?;
        file.seek(SeekFrom::Start(frame.offset))
            .map_err(|e| format!("spool seek: {e}"))?;
        let mut buf = vec![0u8; frame.len as usize];
        file.read_exact(&mut buf)
            .map_err(|e| format!("spool read: {e}"))?;
        Ok(buf)
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Bytes written so far, for the progress line on a long take.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Close the spool, keeping the file if it was marked to survive.
    fn discard(&mut self) {
        self.file = None;
        if !self.path.as_os_str().is_empty() {
            if self.keep && !self.frames.is_empty() {
                eprintln!(
                    "kaviri: {} captured frames kept at {}",
                    self.frames.len(),
                    self.path.display()
                );
            } else {
                let _ = std::fs::remove_file(&self.path);
            }
        }
        self.frames.clear();
        self.bytes = 0;
        self.full = false;
        self.path = PathBuf::new();
    }
}

impl Default for FrameSpool {
    fn default() -> FrameSpool {
        FrameSpool::new()
    }
}

impl Drop for FrameSpool {
    fn drop(&mut self) {
        self.discard();
    }
}

/// How frames are taken off the page.
///
/// `Page.startScreencast` is push-based and free when nothing repaints, but its frames
/// are the size of the CSS viewport: `maxWidth` only ever scales them down, so a 2x
/// device scale factor buys nothing and the zoom crops into pixels that were never
/// captured. `Page.captureScreenshot` honours the device scale factor, so it is the
/// only path that actually supersamples. It costs a round trip per frame, which is why
/// it is used only when it buys something.
#[derive(PartialEq)]
enum Capture {
    Screencast,
    /// Poll `Page.captureScreenshot`; the cadence lives in `shot_base`.
    Screenshot,
}

/// Owns a spawned browser and its profile directory until the `Cdp` around them
/// is fully built.
///
/// Everything between `spawn` and the constructed `Cdp` can fail: the debugger
/// endpoint may never appear, the websocket handshake may be refused. Without
/// this guard those paths drop a bare `Child`, which neither kills nor reaps,
/// leaving a headless browser running and its profile on disk forever.
struct LaunchGuard {
    child: Option<Child>,
    profile_dir: Option<PathBuf>,
}

impl LaunchGuard {
    fn new(child: Child, profile_dir: PathBuf) -> LaunchGuard {
        LaunchGuard {
            child: Some(child),
            profile_dir: Some(profile_dir),
        }
    }

    fn child(&mut self) -> &mut Child {
        self.child
            .as_mut()
            .expect("launch guard still holds its child")
    }

    /// Hand ownership to the caller; nothing is cleaned up after this.
    fn disarm(mut self) -> (Child, PathBuf) {
        (
            self.child
                .take()
                .expect("launch guard still holds its child"),
            self.profile_dir.take().unwrap_or_default(),
        )
    }
}

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        // Still armed means the launch failed part way; a disarmed guard has
        // already handed everything to the `Cdp` and must touch nothing.
        let armed = self.child.is_some();
        if let Some(mut child) = self.child.take() {
            kill_browser(&mut child);
        }
        if let Some(dir) = self.profile_dir.take() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        // A launch that never got as far as a spool leaves nothing else behind,
        // so the run's directory can go too. Non-recursive, so it is a no-op if
        // anything is still in there.
        if armed {
            if let Ok(dir) = session_dir() {
                let _ = std::fs::remove_dir(dir);
            }
        }
    }
}

/// Kill a spawned browser and everything it spawned, then reap it.
///
/// On snap systems the process kaviri spawned is a wrapper script, so killing it
/// alone leaves the real browser running. The child is put in its own process
/// group at spawn precisely so the whole group can be signalled here.
fn kill_browser(child: &mut Child) {
    #[cfg(unix)]
    {
        // SAFETY: the pid is one we spawned, into a process group of its own, so
        // this cannot reach anything else kaviri did not start.
        unsafe {
            libc::killpg(child.id() as i32, libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub struct Cdp {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    child: Child,
    next_id: u64,
    session_id: String,
    responses: HashMap<u64, Value>,
    ack_ids: HashSet<u64>,
    events: VecDeque<(String, Value)>,
    pub frames: FrameSpool,
    pub rec_t0: Option<Instant>,
    recording: bool,
    capture: Capture,
    last_shot: Option<Instant>,
    /// The one screenshot allowed to be in flight, and when it was asked for.
    pending_shot: Option<u64>,
    shot_sent: Option<Instant>,
    /// Requested cadence, and the cadence actually in use after backing off.
    shot_base: Duration,
    shot_interval: Duration,
    /// Mirrors the socket's current read timeout so the syscall is only made
    /// when the value really changes.
    ws_timeout: Duration,
    /// Set once capture has been abandoned mid-take, so it is only reported once.
    capture_abandoned: bool,
    /// True while teardown is running. Teardown pumps the socket, and the pump
    /// can decide to tear down, so the two have to be kept from chasing each other.
    stopping: bool,
    /// What the user asked for with `--keep-temp`, as distinct from the spool
    /// retention a broken capture turns on by itself.
    keep_spool: bool,
    profile_dir: PathBuf,
}

fn free_port() -> std::io::Result<u16> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

fn http_get(port: u16, path: &str) -> std::io::Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_millis(1500)))?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )?;
    // Chromium's DevTools HTTP server may keep the socket open, so read by
    // Content-Length rather than until EOF.
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_ascii_lowercase();
            if let Some(cl) = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
            {
                if buf.len() >= pos + 4 + cl {
                    break;
                }
            }
        }
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break
            }
            Err(e) => return Err(e),
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    match text.split_once("\r\n\r\n") {
        Some((_, body)) => Ok(body.to_string()),
        None => Err(std::io::Error::other("bad HTTP response")),
    }
}

fn find_chromium(explicit: Option<&str>) -> Result<String, String> {
    if let Some(p) = explicit {
        return Ok(p.to_string());
    }
    if let Some(p) = crate::env::var("CHROMIUM") {
        return Ok(p);
    }
    for cand in [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "/snap/bin/chromium",
    ] {
        if Command::new(cand)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(cand.to_string());
        }
    }
    Err("no Chromium/Chrome binary found (set KAVIRI_CHROMIUM or use --chromium)".into())
}

/// Run `step` in slices until `total` has elapsed, never letting one slice run
/// past the deadline.
///
/// This is the pacing behind every wait in the op protocol, factored out so it
/// can be tested without a browser. The bug it exists to prevent: if the socket's
/// read timeout is a fixed 30ms and the loop always runs at least one iteration,
/// then `sleep_pump(18)` takes 35ms and a typewriter asked for 18ms per character
/// types at half speed.
fn pump_until<F>(total: Duration, mut step: F) -> Result<(), String>
where
    F: FnMut(Duration) -> Result<(), String>,
{
    let deadline = Instant::now() + total;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        let slice = (deadline - now).min(PUMP_SLICE).max(PUMP_SLICE_MIN);
        step(slice)?;
    }
}

impl Cdp {
    pub fn launch(
        chromium: Option<&str>,
        css_w: u32,
        css_h: u32,
        scale: f64,
    ) -> Result<Cdp, String> {
        let bin = find_chromium(chromium)?;
        let dev_w = (css_w as f64 * scale).round() as u32;
        let dev_h = (css_h as f64 * scale).round() as u32;
        let port = free_port().map_err(|e| e.to_string())?;
        let profile_dir = session_dir()?.join(format!("profile-{port}"));
        std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&bin);
        cmd.args([
            "--headless=new",
            &format!("--remote-debugging-port={port}"),
            &format!("--user-data-dir={}", profile_dir.display()),
            /*
             * --window-size is in device pixels, so it has to carry the scale or
             * the compositor surface stays 1x and every screencast frame comes
             * back at the CSS size no matter what maxWidth asks for. This is what
             * makes --scale actually supersample rather than being decoration.
             */
            &format!("--window-size={dev_w},{dev_h}"),
            &format!("--force-device-scale-factor={scale}"),
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            /*
             * Software rasterisation, and it is the reason a take is choppy.
             *
             * Measured on this machine (Intel UHD 620, /dev/dri present) with the demo
             * script: scale 1 gives 17 captured frames a second, scale 1.5 gives 12, and
             * scale 2 gives 6. The output is 30fps, so at scale 2 every captured frame is
             * held for five output frames.
             *
             * Dropping this flag was tried and the browser dies mid-take with a websocket
             * reset, and so does --use-angle=swiftshader, so hardware compositing is not a
             * fix that can simply be switched on here. Until that is understood, --smooth on
             * is the answer for anything that has to look smooth, and the warning kaviri
             * already prints when capture falls below 24fps is how you find out you need it.
             */
            /*
             * Software rasterisation, and it was measured rather than assumed.
             *
             * On this machine (Intel UHD 620, /dev/dri present, snap Chromium) the demo
             * script captured 7 frames a second with this flag, 7 without it, 6 with
             * --use-angle=gl and 2 with --use-angle=swiftshader. Enabling the GPU buys
             * nothing, so the flag stays: it is one less thing to go wrong on a CI runner
             * with no display, and it is not what makes a take choppy. What makes a take
             * choppy is how fast the browser paints, and under software rendering that is
             * around ten frames a second whatever the flags say.
             */
            "--disable-gpu",
            "--hide-scrollbars",
            /*
             * No speculative prerendering. A site with speculation rules builds the
             * next page in a hidden target, and a click activates that target in
             * place of the one kaviri is attached to: the take then freezes on the
             * old page while the browser moves on, with a 30s CDP stall at the
             * swap. One page, one target, for the whole recording.
             */
            "--disable-features=Prerender2",
            "--mute-audio",
            "--force-color-profile=srgb",
        ])
        // Extra flags for the machine this is running on, not for kaviri to decide.
        // CI is the reason this exists: Chromium's sandbox cannot start inside most
        // containers, so a runner needs --no-sandbox. That is a real reduction in
        // isolation and belongs to whoever owns the machine, so it is opt in here rather
        // than a default that quietly weakens every local recording too.
        .args(
            crate::env::var("CHROMIUM_ARGS")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        )
        .arg("about:blank")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        /*
         * Its own process group, so teardown can signal the whole browser tree
         * rather than just the wrapper script snap puts in front of it, and so a
         * ctrl-C in the terminal reaches kaviri's handler instead of killing the
         * browser out from under a take that is still being rendered.
         */
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn {bin}: {e}"))?;
        let mut guard = LaunchGuard::new(child, profile_dir.clone());

        // Wait for the debugger endpoint.
        let deadline = Instant::now() + Duration::from_secs(40);
        let mut exited: Option<(Instant, std::process::ExitStatus)> = None;
        let ws_url = loop {
            match http_get(port, "/json/version") {
                Ok(body) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&body) {
                        if let Some(u) = v["webSocketDebuggerUrl"].as_str() {
                            break u.to_string();
                        }
                    }
                    if crate::env::is_set("DEBUG") {
                        eprintln!("kaviri[debug]: /json/version body without ws url: {body}");
                    }
                }
                Err(e) => {
                    if crate::env::is_set("DEBUG") {
                        eprintln!("kaviri[debug]: /json/version poll failed: {e}");
                    }
                }
            }
            if shutting_down() {
                return Err("interrupted while waiting for chromium".into());
            }
            /*
             * A browser that has already exited is never going to answer, so say so
             * in two seconds rather than in forty. The grace period is for launchers
             * that fork and let the parent exit, where the real browser is still
             * coming up behind a process kaviri can no longer see.
             */
            if exited.is_none() {
                if let Ok(Some(status)) = guard.child().try_wait() {
                    exited = Some((Instant::now(), status));
                }
            }
            if let Some((seen, status)) = exited {
                if seen.elapsed() > Duration::from_secs(2) {
                    return Err(format!(
                        "{bin} exited during startup ({status}) without exposing a debugger endpoint"
                    ));
                }
            }
            if Instant::now() > deadline {
                return Err("chromium did not expose a debugger endpoint in 40s".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        };

        let (mut ws, _) =
            tungstenite::connect(&ws_url).map_err(|e| format!("websocket connect failed: {e}"))?;
        let ws_timeout = PUMP_SLICE;
        if let MaybeTlsStream::Plain(s) = ws.get_mut() {
            s.set_read_timeout(Some(ws_timeout))
                .map_err(|e| e.to_string())?;
        }

        let (child, profile_dir) = guard.disarm();
        let mut cdp = Cdp {
            ws,
            child,
            next_id: 1,
            session_id: String::new(),
            responses: HashMap::new(),
            ack_ids: HashSet::new(),
            events: VecDeque::new(),
            frames: FrameSpool::new(),
            capture: Capture::Screencast,
            last_shot: None,
            pending_shot: None,
            shot_sent: None,
            shot_base: Duration::from_millis(25),
            shot_interval: Duration::from_millis(25),
            ws_timeout,
            capture_abandoned: false,
            stopping: false,
            keep_spool: crate::env::is_set("KEEP_TEMP"),
            rec_t0: None,
            recording: false,
            profile_dir,
        };

        let target = cdp.send_raw("Target.createTarget", json!({"url": "about:blank"}), None)?;
        let target_id = target["targetId"]
            .as_str()
            .ok_or("no targetId")?
            .to_string();
        let attach = cdp.send_raw(
            "Target.attachToTarget",
            json!({"targetId": target_id, "flatten": true}),
            None,
        )?;
        cdp.session_id = attach["sessionId"]
            .as_str()
            .ok_or("no sessionId")?
            .to_string();

        cdp.send("Page.enable", json!({}))?;
        cdp.send("Runtime.enable", json!({}))?;
        cdp.send(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": css_w, "height": css_h, "deviceScaleFactor": scale, "mobile": false}),
        )?;
        Ok(cdp)
    }

    /// Where the frame spool is written and how large it may grow. Must be set
    /// before `start_capture`; afterwards it applies to the next take.
    // Both of these are also configurable by environment variable, so the CLI is
    // free not to wire the flags through.
    #[allow(dead_code)]
    pub fn set_spool_config(&mut self, dir: Option<PathBuf>, max_bytes: Option<u64>) {
        self.frames.configure(dir, max_bytes);
    }

    /// Keep the spool file after teardown, printing where it landed. This is
    /// `--keep-temp` for the frames.
    #[allow(dead_code)]
    pub fn set_keep_spool(&mut self, keep: bool) {
        self.keep_spool = keep;
        self.frames.keep(keep);
    }

    /// Longest the next socket read may block. Only issues the syscall on a change.
    fn set_ws_timeout(&mut self, d: Duration) -> Result<(), String> {
        if self.ws_timeout == d {
            return Ok(());
        }
        if let MaybeTlsStream::Plain(s) = self.ws.get_mut() {
            s.set_read_timeout(Some(d))
                .map_err(|e| format!("ws timeout: {e}"))?;
        }
        self.ws_timeout = d;
        Ok(())
    }

    /// Send a command; empty session_id means browser-level.
    fn send_raw(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<Value, String> {
        self.send_raw_within(method, params, session, Duration::from_secs(30))
    }

    fn send_raw_within(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            if !s.is_empty() {
                msg["sessionId"] = json!(s);
            }
        }
        self.ws
            .send(Message::Text(msg.to_string()))
            .map_err(|e| format!("ws send: {e}"))?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(resp) = self.responses.remove(&id) {
                if let Some(err) = resp.get("error") {
                    return Err(format!("{method}: {err}"));
                }
                return Ok(resp["result"].clone());
            }
            let now = Instant::now();
            if now > deadline {
                /* A late answer is discarded on arrival rather than piling up unread. */
                self.ack_ids.insert(id);
                return Err(format!("{method}: timed out waiting for response"));
            }
            // Never block past this command's own deadline.
            let slice = (deadline - now).min(PUMP_SLICE).max(PUMP_SLICE_MIN);
            self.set_ws_timeout(slice)?;
            self.pump()?;
            /*
             * Keep capturing while waiting for a command to answer.
             *
             * This used to pump only the socket, which left a hole in the footage for the
             * whole of every round trip. That sounds small until you count them: resolving a
             * selector is two Runtime.evaluate calls, the caret probe is a third that appends
             * a node and forces layout, and a `wait` on a selector is an evaluate every
             * 100ms. All of that happens in the moment around a click, which is exactly the
             * moment the camera has zoomed in on.
             *
             * The CFR pass is sample-and-hold, so a gap in capture is emitted as N copies of
             * the last frame and then the next real one, by which time the page has already
             * finished changing. On screen that is a freeze followed by a snap: a jump cut,
             * in the one second of the take that matters.
             *
             * The error is swallowed deliberately. A capture that cannot proceed must not
             * take the op down with it, and if the socket is genuinely gone the pump above
             * reports it on the next turn of this same loop.
             */
            let _ = self.shoot_if_due();
            self.enforce_spool_limit();
        }
    }

    pub fn send(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let session = self.session_id.clone();
        self.send_raw(method, params, Some(&session))
    }

    /// As `send`, but gives up after `timeout`. For calls that are cheap to lose.
    pub fn send_within(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let session = self.session_id.clone();
        self.send_raw_within(method, params, Some(&session), timeout)
    }

    /// Drain any pending websocket messages. Blocks for at most the socket's
    /// current read timeout, which the caller sets from its own deadline.
    /// Frames are acked and stored here.
    pub fn pump(&mut self) -> Result<(), String> {
        loop {
            match self.ws.read() {
                Ok(Message::Text(txt)) => self.handle_message(&txt)?,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Ok(())
                }
                Err(e) => {
                    let msg = format!("ws read: {e}");
                    self.abandon_capture(&msg);
                    return Err(msg);
                }
            }
        }
    }

    /// One capture step and nothing else: drain the socket, take a screenshot if
    /// one is due, stop cleanly if the spool is full.
    ///
    /// This is what lets capture run on a clock rather than only inside an op.
    /// Without it, serve mode captures nothing during the seconds an agent spends
    /// thinking between ops, and the finished video is a sequence of freeze frames
    /// covering only the moments kaviri was already busy.
    #[allow(dead_code)]
    pub fn tick(&mut self) -> Result<(), String> {
        self.pump()?;
        self.shoot_if_due()?;
        self.enforce_spool_limit();
        Ok(())
    }

    /// Capture has failed part way through a take. Stop, but keep what was
    /// captured: on a long recording the frames already on disk are the whole
    /// value of the take, and the caller can still render them.
    fn abandon_capture(&mut self, err: &str) {
        if !self.recording {
            return;
        }
        self.recording = false;
        self.pending_shot = None;
        if self.frames.is_empty() || self.capture_abandoned {
            return;
        }
        self.capture_abandoned = true;
        self.frames.keep(true);
        eprintln!(
            "kaviri: capture stopped after {}: the {} frames already captured are kept for rendering",
            err,
            self.frames.len()
        );
    }

    fn enforce_spool_limit(&mut self) {
        if !self.recording || self.stopping || !self.frames.is_full() {
            return;
        }
        eprintln!(
            "kaviri: frame spool reached its {} MiB limit; stopping capture and rendering what was recorded (raise --max-spool-bytes)",
            self.frames.max_bytes() / (1024 * 1024)
        );
        let _ = self.stop_capture_best_effort();
    }

    fn handle_message(&mut self, txt: &str) -> Result<(), String> {
        let v: Value = match serde_json::from_str(txt) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        if let Some(id) = v["id"].as_u64() {
            if self.pending_shot == Some(id) {
                self.take_shot_reply(&v);
                return Ok(());
            }
            if !self.ack_ids.remove(&id) {
                self.responses.insert(id, v);
            }
            return Ok(());
        }
        let method = v["method"].as_str().unwrap_or("").to_string();
        if method == "Page.screencastFrame" {
            let params = &v["params"];
            if self.recording {
                if let (Some(data), Some(t0)) = (params["data"].as_str(), self.rec_t0) {
                    if let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(data) {
                        let t = t0.elapsed().as_secs_f64();
                        if let Err(e) = self.frames.push(t, &jpeg) {
                            /*
                             * A spool that cannot be written to is the end of capture,
                             * not the end of the session: the caller still gets to
                             * render everything that landed before this frame.
                             */
                            self.abandon_capture(&e);
                        }
                    }
                }
            }
            // Ack (fire and forget; response id is discarded in handle_message).
            if let Some(frame_session) = params["sessionId"].as_i64() {
                let id = self.next_id;
                self.next_id += 1;
                self.ack_ids.insert(id);
                let msg = json!({
                    "id": id,
                    "method": "Page.screencastFrameAck",
                    "params": {"sessionId": frame_session},
                    "sessionId": self.session_id,
                });
                self.ws
                    .send(Message::Text(msg.to_string()))
                    .map_err(|e| format!("ws send ack: {e}"))?;
            }
            return Ok(());
        }
        self.events.push_back((method, v["params"].clone()));
        // Keep the event queue bounded.
        while self.events.len() > 512 {
            self.events.pop_front();
        }
        Ok(())
    }

    /// The answer to the one outstanding `Page.captureScreenshot`.
    ///
    /// A reply that came back late is still a real frame of the take, so it is
    /// spooled at the moment it arrived rather than thrown away; the CFR pass
    /// holds the previous frame up to it either way.
    fn take_shot_reply(&mut self, v: &Value) {
        self.pending_shot = None;
        let sent = self.shot_sent.take();
        if v.get("error").is_some() {
            if crate::env::is_set("DEBUG") {
                eprintln!("kaviri[debug]: captureScreenshot: {}", v["error"]);
            }
        } else if self.recording {
            if let (Some(data), Some(t0)) = (v["result"]["data"].as_str(), self.rec_t0) {
                if let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(data) {
                    let t = t0.elapsed().as_secs_f64();
                    if let Err(e) = self.frames.push(t, &jpeg) {
                        self.abandon_capture(&e);
                        return;
                    }
                }
            }
        }
        /*
         * Adapt the cadence to what the browser can actually deliver. A capture that
         * took longer than the interval means the page is heavy or the machine is
         * loaded; asking again at the same rate would only queue round trips and
         * steal CPU from the app being filmed, which is what made a --scale 2 take
         * run visibly slower than reality.
         */
        if let Some(sent) = sent {
            let took = sent.elapsed();
            self.shot_interval = if took > self.shot_base {
                took.min(SHOT_INTERVAL_MAX)
            } else {
                self.shot_base
            };
        }
    }

    /// Wait until an event with this method arrives (consumes it).
    pub fn wait_event(&mut self, method: &str, timeout: Duration) -> Result<bool, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pos) = self.events.iter().position(|(m, _)| m == method) {
                self.events.remove(pos);
                return Ok(true);
            }
            let now = Instant::now();
            if now > deadline || shutting_down() {
                return Ok(false);
            }
            let slice = (deadline - now).min(PUMP_SLICE).max(PUMP_SLICE_MIN);
            self.set_ws_timeout(slice)?;
            self.pump()?;
            self.shoot_if_due()?;
            self.enforce_spool_limit();
        }
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Sleep while keeping the frame pump alive.
    ///
    /// The socket read is what does the waiting, with its timeout cut down to the
    /// budget that is left, so this returns when the caller asked rather than at
    /// whatever granularity the socket happens to be set to.
    pub fn sleep_pump(&mut self, ms: u64) -> Result<(), String> {
        pump_until(Duration::from_millis(ms), |slice| {
            if shutting_down() {
                return Err(String::new());
            }
            self.set_ws_timeout(slice)?;
            self.pump()?;
            /*
             * Every wait in the op protocol lands here, so this is where the screenshot
             * pump gets its cadence: no separate thread, no second CDP connection, and
             * the frame clock stays the recording clock.
             */
            self.shoot_if_due()?;
            self.enforce_spool_limit();
            Ok(())
        })
        // An interrupt cuts the wait short and says nothing; the caller's op loop
        // sees `shutting_down()` on its next turn and unwinds properly.
        .or_else(|e| if e.is_empty() { Ok(()) } else { Err(e) })
    }

    /// Evaluate JS in the page; returns the by-value result.
    pub fn evaluate(&mut self, expr: &str) -> Result<Value, String> {
        let r = self.send(
            "Runtime.evaluate",
            json!({"expression": expr, "returnByValue": true, "awaitPromise": true}),
        )?;
        if let Some(exc) = r.get("exceptionDetails") {
            return Err(format!("js exception: {exc}"));
        }
        Ok(r["result"]["value"].clone())
    }

    /// Begin capturing.
    ///
    /// Screencast for everything. This used to switch to the screenshot pump whenever
    /// `scale > 1`, on the belief that screencast could not deliver more pixels than the CSS
    /// viewport has. That belief was wrong, and it cost most of the frame rate of every
    /// supersampled take.
    ///
    /// Measured on the demo script at 1100x620@2x, software rendering:
    ///
    ///   screenshot poll   7 frames a second
    ///   screencast       11 frames a second, spool frames 2200x1240
    ///
    /// The JPEGs on disk were checked, not just the number kaviri prints: screencast honours
    /// maxWidth and maxHeight in device pixels, and --window-size already carries the scale,
    /// so the compositor surface really is 2200x1240 and that is what arrives. The screenshot
    /// path is slower because every frame is a full command round trip plus a base64 of a
    /// 2.7 megapixel JPEG, and none of that is on the compositor's own path.
    ///
    /// The old path is kept behind KAVIRI_CAPTURE=screenshot, because screencast is the one
    /// thing here that depends on the browser and a future Chromium could clamp it again.
    /// If a take comes back at the CSS size when scale is above 1, that is the switch to try,
    /// and this comment is wrong.
    pub fn start_capture(&mut self, max_w: u32, max_h: u32) -> Result<(), String> {
        self.frames.reset()?;
        self.rec_t0 = Some(Instant::now());
        self.recording = true;
        self.capture_abandoned = false;
        self.last_shot = None;
        self.pending_shot = None;
        self.shot_sent = None;
        if crate::env::var("CAPTURE").as_deref() == Some("screenshot") {
            /*
             * 25ms is a target, not a guarantee: a screenshot of a heavy page takes
             * longer than that and the pump simply falls behind, which the CFR pass
             * absorbs. Only one capture is ever in flight, so falling behind costs
             * frames rather than piling work onto the browser.
             */
            self.shot_base = Duration::from_millis(25);
            self.shot_interval = self.shot_base;
            self.capture = Capture::Screenshot;
            self.shoot()?;
        } else {
            self.capture = Capture::Screencast;
            self.send(
                "Page.startScreencast",
                json!({"format": "jpeg", "quality": 82, "maxWidth": max_w, "maxHeight": max_h, "everyNthFrame": 1}),
            )?;
        }
        Ok(())
    }

    fn shot_params() -> Value {
        /*
         * No clip. The window and the emulation already give the viewport device pixels,
         * so a plain viewport screenshot is css * scale. A clip is in document
         * coordinates, not viewport ones: pinned at y 0 it pointed above the fold as soon
         * as the page scrolled and came back blank, and carrying the scale as well
         * squared it.
         */
        json!({
            "format": "jpeg", "quality": 82,
            "optimizeForSpeed": true,
            "captureBeyondViewport": false,
        })
    }

    /// Ask for one screenshot. The reply is spooled by the pump when it arrives.
    ///
    /// Fire and forget on purpose. Waiting here would block the op that is running,
    /// and the earlier version's 700ms give-up did not cancel anything: Chromium
    /// still rendered and still sent a few hundred KB of JPEG, while a fresh request
    /// went out 25ms later. Three captures of a slow page could be outstanding at
    /// once, tripling the compositing work for frames that were then discarded.
    fn shoot(&mut self) -> Result<(), String> {
        if self.rec_t0.is_none() || self.pending_shot.is_some() {
            return Ok(());
        }
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "id": id,
            "method": "Page.captureScreenshot",
            "params": Cdp::shot_params(),
            "sessionId": self.session_id,
        });
        // A frame lost to a navigation in flight should not end a take, but a send
        // that fails means the socket is gone and there is nothing left to capture.
        self.ws
            .send(Message::Text(msg.to_string()))
            .map_err(|e| format!("ws send: {e}"))?;
        let now = Instant::now();
        self.pending_shot = Some(id);
        self.shot_sent = Some(now);
        self.last_shot = Some(now);
        Ok(())
    }

    /// Take a screenshot if one is due and none is in flight. No-op in screencast mode.
    fn shoot_if_due(&mut self) -> Result<(), String> {
        if self.capture != Capture::Screenshot || !self.recording {
            return Ok(());
        }
        if let (Some(id), Some(sent)) = (self.pending_shot, self.shot_sent) {
            if sent.elapsed() < SHOT_ABANDON {
                return Ok(());
            }
            // The target has stopped answering. Write this one off so the pump can
            // recover if the page ever comes back, and drop its reply on arrival.
            self.ack_ids.insert(id);
            self.pending_shot = None;
            self.shot_sent = None;
            self.shot_interval = SHOT_INTERVAL_MAX;
        }
        let due = self
            .last_shot
            .map(|t| t.elapsed() >= self.shot_interval)
            .unwrap_or(true);
        if due {
            self.shoot()?;
        }
        Ok(())
    }

    /// Stop capturing, whatever state the browser is in.
    ///
    /// Nothing here is allowed to fail the take. Every frame is already on disk by
    /// the time this runs, so a dead websocket or a browser that has gone away is
    /// worth a warning and nothing more; the caller still renders. Returns the
    /// warning text, if there was one.
    pub fn stop_capture_best_effort(&mut self) -> Option<String> {
        if self.stopping {
            return None;
        }
        self.stopping = true;
        let mut warning: Option<String> = None;
        let mut note = |w: String| {
            if warning.is_none() {
                warning = Some(w);
            }
        };

        if self.capture == Capture::Screencast {
            if let Err(e) =
                self.send_within("Page.stopScreencast", json!({}), Duration::from_secs(2))
            {
                note(e);
            }
        } else {
            // Abandon anything in flight, then take one last frame synchronously so
            // the tail is the page as it finally looked.
            if let Some(id) = self.pending_shot.take() {
                self.ack_ids.insert(id);
            }
            self.shot_sent = None;
            match self.send_within(
                "Page.captureScreenshot",
                Cdp::shot_params(),
                Duration::from_millis(1500),
            ) {
                Ok(r) => {
                    if let (Some(data), Some(t0)) = (r["data"].as_str(), self.rec_t0) {
                        if let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(data) {
                            let t = t0.elapsed().as_secs_f64();
                            if let Err(e) = self.frames.push(t, &jpeg) {
                                note(e);
                            }
                        }
                    }
                }
                Err(e) => note(e),
            }
        }

        // Drain stragglers, including any screencast frames still on the socket.
        if let Err(e) = self.sleep_pump(150) {
            note(e);
        }

        // The screencast only sends frames on paint, so a static tail would
        // otherwise be cut off: hold the last frame until stop time.
        let tail = self.frames.frames().last().map(|f| f.t);
        if let Some(last_t) = tail {
            let t_stop = self.now_rec();
            if t_stop > last_t + 0.05 {
                match self.frames.read(self.frames.len() - 1) {
                    Ok(jpeg) => {
                        if let Err(e) = self.frames.push(t_stop, &jpeg) {
                            note(e);
                        }
                    }
                    Err(e) => note(e),
                }
            }
        }

        self.recording = false;
        self.pending_shot = None;
        self.shot_sent = None;
        self.stopping = false;
        warning
    }

    /// `stop_capture_best_effort` in the shape older callers expect.
    // Kept so a caller that has not moved to the best-effort form still compiles;
    // it may legitimately have no callers left.
    #[allow(dead_code)]
    pub fn stop_capture(&mut self) -> Result<(), String> {
        match self.stop_capture_best_effort() {
            None => Ok(()),
            Some(w) => Err(w),
        }
    }

    /// Give the spool's disk space back once the take has been rendered.
    ///
    /// In serve mode the process outlives the take, so without this a delivered
    /// three-minute recording keeps gigabytes of temp space allocated for as long
    /// as the server is up.
    #[allow(dead_code)]
    pub fn frames_release(&mut self) {
        /*
         * The take has been rendered, so the retention a broken capture switched on
         * has done its job and no longer applies. An explicit --keep-temp still does.
         */
        self.frames.keep(self.keep_spool);
        self.frames.discard();
        self.capture_abandoned = false;
    }

    /// Seconds on the recording clock right now (0.0 if not recording).
    pub fn now_rec(&self) -> f64 {
        self.rec_t0
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }
}

impl Drop for Cdp {
    fn drop(&mut self) {
        // Ask the browser to exit via CDP first: a clean exit flushes the profile
        // and does not leave the port bound.
        let id = self.next_id;
        let msg = json!({"id": id, "method": "Browser.close", "params": {}});
        let _ = self.ws.send(Message::Text(msg.to_string()));
        std::thread::sleep(Duration::from_millis(300));
        kill_browser(&mut self.child);
        // Release the spool before removing the session directory, so a spool that
        // is being kept is not swept away by the directory removal below.
        self.frames.discard();
        let _ = std::fs::remove_dir_all(&self.profile_dir);
        // Non-recursive on purpose: it succeeds only once everything this run put
        // inside it is gone, and leaves a deliberately kept spool alone.
        if let Ok(dir) = session_dir() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// Parse JPEG dimensions from SOF marker (width, height).
pub fn jpeg_dims(data: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize; // skip SOI
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        // SOF0..SOF15 except DHT(C4)/JPG(C8)/DAC(CC)
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            let h = u32::from(data[i + 5]) << 8 | u32::from(data[i + 6]);
            let w = u32::from(data[i + 7]) << 8 | u32::from(data[i + 8]);
            return Some((w, h));
        }
        let len = usize::from(data[i + 2]) << 8 | usize::from(data[i + 3]);
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep decides what to `remove_dir_all`, so the interesting cases are the ones
    /// where it must say no.
    #[test]
    fn the_sweep_only_takes_directories_nobody_is_using() {
        let dead = |_: u32| false;
        let alive = |_: u32| true;
        let old = Some(SWEEP_MAX_AGE + Duration::from_secs(1));
        let fresh = Some(Duration::from_secs(60));

        // Somebody else's finished run, which is the whole point of the thing.
        assert!(is_stale_run_dir("kaviri-4242-abc123", 99, &dead, fresh));

        // The pid is not always the first segment. Render scratch puts two words in front of
        // it, and reading only the first segment left these behind for a week.
        assert!(is_stale_run_dir(
            "kaviri-render-sc-4242-eb1bfd2ccdf3b953",
            99,
            &dead,
            fresh
        ));
        assert!(!is_stale_run_dir(
            "kaviri-render-sc-4242-eb1bfd2ccdf3b953",
            4242,
            &dead,
            fresh
        ));

        // The 16 hex characters of a run token are never read as a process id: sixteen
        // digits does not fit in a u32, so an all-numeric token cannot collide with one.
        assert!(is_stale_run_dir(
            "kaviri-4242-1234567890123456",
            99,
            &dead,
            fresh
        ));

        // Not ours to touch.
        assert!(!is_stale_run_dir("some-other-tool-4242", 99, &dead, old));
        assert!(!is_stale_run_dir("systemd-private-77bb", 99, &dead, old));

        // Our own directory, which we are about to put a browser profile in.
        assert!(!is_stale_run_dir("kaviri-4242-abc123", 4242, &dead, fresh));

        // A run that is still going. This is the one that matters: a `kaviri serve` session
        // an agent has held open for hours must not have its profile deleted underneath it.
        assert!(!is_stale_run_dir("kaviri-4242-abc123", 99, &alive, old));

        // The old naming scheme carries a port, not a process id. If nothing is holding that
        // number it goes; if something is, it waits, however old it is. Waiting costs one
        // directory, and the alternative risks deleting a live session's profile.
        assert!(is_stale_run_dir("lensa-profile-46393", 99, &dead, fresh));
        assert!(!is_stale_run_dir("lensa-profile-46393", 99, &alive, old));

        // A name with no number in it at all. Age is the only evidence there is, and no
        // mtime is no evidence.
        assert!(is_stale_run_dir("lensa-plate-test", 99, &alive, old));
        assert!(!is_stale_run_dir("lensa-plate-test", 99, &alive, fresh));
        assert!(!is_stale_run_dir("lensa-plate-test", 99, &alive, None));
    }

    /// The regression this guards: with a fixed 30ms socket read timeout and a
    /// loop that always ran at least once, an 18ms wait took 35ms, so a typewriter
    /// asked for 18ms per character typed at half the requested speed.
    #[test]
    fn a_wait_ends_on_its_own_deadline_not_the_socket_timeout() {
        let mut slices: Vec<Duration> = Vec::new();
        let started = Instant::now();
        // Stand in for `pump()` on an idle socket: it blocks for exactly the read
        // timeout it was handed and then reports WouldBlock.
        pump_until(Duration::from_millis(18), |slice| {
            slices.push(slice);
            std::thread::sleep(slice);
            Ok(())
        })
        .unwrap();
        let took = started.elapsed();
        assert!(
            took >= Duration::from_millis(18),
            "an 18ms wait must not return early, took {took:?}"
        );
        assert!(
            took < Duration::from_millis(25),
            "an 18ms wait must not cost 35ms, took {took:?}"
        );
        assert!(
            slices
                .iter()
                .all(|s| *s <= PUMP_SLICE && *s >= PUMP_SLICE_MIN),
            "every read timeout must be a bounded, non-zero slice: {slices:?}"
        );
    }

    #[test]
    fn a_zero_wait_does_not_block_at_all() {
        let mut calls = 0;
        pump_until(Duration::from_millis(0), |_| {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 0);
    }

    #[test]
    fn the_spool_stops_at_its_limit_instead_of_filling_the_disk() {
        let mut spool = FrameSpool::new();
        spool.configure(None, Some(4096));
        spool.reset().expect("spool opens");
        let frame = vec![0u8; 1000];
        for i in 0..8 {
            spool
                .push(i as f64 * 0.04, &frame)
                .expect("push never errors on a full spool");
        }
        assert!(spool.is_full(), "the spool must report that it stopped");
        assert_eq!(spool.len(), 4, "only the frames that fit are indexed");
        assert!(spool.bytes() <= 4096);
        // The frames that did fit must still be readable back byte for byte.
        assert_eq!(spool.read(3).expect("frame 3 reads back"), frame);
    }

    #[test]
    fn the_session_directory_is_private_to_this_run() {
        let dir = session_dir().expect("session dir");
        assert!(dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o700,
                "the session directory must not be readable by others"
            );
        }
    }
}
