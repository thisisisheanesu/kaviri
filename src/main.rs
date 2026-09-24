//! kaviri: a programmable browser that records itself and produces
//! Screen Studio-style auto-zoomed videos. "Screen Studio for AI agents."
//!
//!   kaviri record --script demo.jsonl --out demo.mp4
//!   kaviri serve
//!   kaviri doctor

mod backdrop;
mod cdp;
mod env;
mod ops;
mod zoom;

use ops::{CursorCfg, Session, DEFAULT_CURSOR_SCALE};
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const USAGE: &str = "\
kaviri: programmable recording browser (Screen Studio for AI agents)

USAGE:
  kaviri record --script <file.jsonl> --out <file.mp4> [options]
  kaviri serve [--port <n>] [--out <file.mp4>] [options]
  kaviri doctor | kaviri presets | kaviri backgrounds | kaviri --version

OPTIONS:
  --script <path>     newline-delimited JSON ops to run (record mode)
  --out <path>        output MP4 (default kaviri-out.mp4)
  --port <n>          serve ops over TCP instead of stdin/stdout. Opt-in: see
                      the token note below before using it
  --width <px>        logical viewport width  (default 1470, 64..16384)
  --height <px>       logical viewport height (default 830, 64..16384)
  --scale <f>         device scale factor / capture supersampling
                      (default 2, 0.5..4)
  --preset <name>     a named viewport/capture/output shape; --presets lists them
  --out-width <px>    video width  (default: the viewport width, 64..8192)
  --out-height <px>   video height (default: the viewport height, 64..8192)
  --background <name> backdrop the take is composited onto: a name, auto or
                      none (default auto); --backgrounds lists the names
  --smooth <mode>     on, off or auto (default auto, which is off with a note).
                      A browser under software rendering paints six to seventeen
                      frames a second, so a take can be choppy however smooth
                      the camera is. --smooth on invents the frames in between,
                      which looks right and costs about twelve times the length
                      of the take.
  --slowmo <k>        run the page's clock k times slower while recording (default
                      1, up to 16), then play the take back at normal speed. The
                      browser gets k times as long to paint each moment, so a
                      canvas-heavy page that paints 7 frames a second comes out
                      at 7k. Real frames, unlike --smooth, and the take costs k
                      times its length to record. Script timings stay as written.
  --cursor <name>     pointer shape: auto, arrow, hand, text or none
                      (default auto: whatever the OS would show)
  --cursor-scale <f>  pointer size against a 1x system cursor (default 1.75)
  --chromium <path>   browser binary (default: autodetect / $KAVIRI_CHROMIUM)
  --spool-dir <path>  where captured frames are spooled (default: a private
                      directory under $TMPDIR)
  --max-spool-bytes <n>
                      ceiling on the frame spool; the take stops and renders
                      what it has when it is reached (default 8 GiB)
  --keep-temp         keep the intermediate CFR video and the frame spool
  --audio             reserved; audio capture is not yet implemented
  --version, -V       print the version and exit

OPS (one JSON object per line):
  {\"op\":\"start_recording\"[,\"path\":\"out.mp4\"]}
  {\"op\":\"navigate\",\"url\":\"https://…\"}       (bare paths become file://)
  {\"op\":\"click\",\"selector\":\"css\"}  or  {\"op\":\"click\",\"x\":.., \"y\":..}
  {\"op\":\"hover\",\"selector\":\"css\"[,\"at\":[0.2,0.5]][,\"ms\":500]}  or  {\"op\":\"hover\",\"x\":.., \"y\":..}
  {\"op\":\"type\",\"selector\":\"css\",\"text\":\"…\",\"typewriter_ms\":18}
  {\"op\":\"scroll\",\"y\":600,\"smooth\":true}
  {\"op\":\"wait\",\"ms\":800}  or  {\"op\":\"wait\",\"selector\":\"css\"[,\"timeout_ms\":20000]}
  {\"op\":\"mark\",\"label\":\"checkout\"}
  {\"op\":\"stop_recording\"}

Every op answers with one JSON line: {\"ok\":true,\"result\":{…}} or
{\"ok\":false,\"error\":\"…\"}. With --port, the first line of a connection must be
{\"op\":\"hello\",\"token\":\"…\"}; the token is printed on stderr at startup, or set
it yourself with $KAVIRI_TOKEN.
";

/// A named shape for a take: how the page lays out, how much is captured, and how big
/// the file is.
///
/// The three are separate on purpose. A vertical take wants a 432px viewport so the site
/// lays out the way it would on a phone, 2.5x capture so a zoom still has real pixels to
/// crop into, and a 1080x1920 file so it is not a postage stamp on TikTok. Collapsing
/// them into one number, which is what `--width` alone did, makes any two of those three
/// impossible at once.
struct Preset {
    name: &'static str,
    css: (u32, u32),
    scale: f64,
    out: (u32, u32),
    about: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        name: "desktop",
        css: (1470, 830),
        scale: 2.0,
        out: (1470, 830),
        about: "the default: a laptop window, captured at 2x",
    },
    Preset {
        name: "tiktok",
        css: (432, 768),
        scale: 2.5,
        out: (1080, 1920),
        about: "9:16 vertical, phone layout, 1080x1920. Also for Reels and Shorts",
    },
    Preset {
        name: "reels",
        css: (432, 768),
        scale: 2.5,
        out: (1080, 1920),
        about: "same as tiktok",
    },
    Preset {
        name: "shorts",
        css: (432, 768),
        scale: 2.5,
        out: (1080, 1920),
        about: "same as tiktok",
    },
    Preset {
        name: "square",
        css: (540, 540),
        scale: 2.0,
        out: (1080, 1080),
        about: "1:1 for a feed post",
    },
    Preset {
        name: "landscape",
        css: (960, 540),
        scale: 2.0,
        out: (1920, 1080),
        about: "16:9 1080p. Large type for a screen at the back of a room",
    },
    Preset {
        name: "readme",
        css: (1100, 620),
        scale: 2.0,
        out: (1100, 620),
        about: "wide and light, sized to sit in a README without scaling",
    },
    Preset {
        name: "phone",
        css: (390, 844),
        scale: 3.0,
        out: (1170, 2532),
        about: "a real phone's viewport and pixel count, for a device mock",
    },
];

fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

fn preset_help() -> String {
    PRESETS
        .iter()
        .map(|p| {
            format!(
                "  {:<10} {}x{} viewport, {}x{} video  {}",
                p.name, p.css.0, p.css.1, p.out.0, p.out.1, p.about
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct Args {
    mode: String,
    script: Option<String>,
    out: String,
    port: Option<u16>,
    width: u32,
    height: u32,
    scale: f64,
    out_w: Option<u32>,
    out_h: Option<u32>,
    background: backdrop::Choice,
    smooth: zoom::Smooth,
    slowmo: f64,
    cursor: CursorCfg,
    chromium: Option<String>,
    keep_temp: bool,
    spool_dir: Option<PathBuf>,
    max_spool_bytes: Option<u64>,
}

/// What the command line asked for.
///
/// Help and the listing subcommands are not errors: they are the whole point of
/// the invocation, so they go to stdout and exit 0. Only `Error` is a usage
/// failure. Keeping the two apart is what makes `kaviri presets > presets.txt`
/// write a file rather than an empty one.
enum Parsed {
    Run(Box<Args>),
    Help(String),
    Error(String),
}

fn version_line() -> String {
    format!("kaviri {}", env!("CARGO_PKG_VERSION"))
}

/// Parse an integer flag, rejecting the values that only fail much later.
///
/// `--width 0` used to be accepted here and surfaced as ffmpeg rejecting a
/// filter expression containing the literal `NaN`, after the whole take had been
/// captured. The range is the flag's contract, so it belongs at the flag.
fn dimension(flag: &str, raw: &str, lo: u32, hi: u32) -> Result<u32, String> {
    let n: u32 = raw.parse().map_err(|_| {
        format!("{flag} must be a whole number of pixels between {lo} and {hi}, got {raw}")
    })?;
    if !(lo..=hi).contains(&n) {
        return Err(format!("{flag} must be between {lo} and {hi}, got {n}"));
    }
    Ok(n)
}

fn ratio(flag: &str, raw: &str, lo: f64, hi: f64) -> Result<f64, String> {
    let v: f64 = raw
        .parse()
        .map_err(|_| format!("{flag} must be a number between {lo} and {hi}, got {raw}"))?;
    if !(v.is_finite() && (lo..=hi).contains(&v)) {
        return Err(format!("{flag} must be between {lo} and {hi}, got {raw}"));
    }
    Ok(v)
}

fn positive_u64(flag: &str, raw: &str) -> Result<u64, String> {
    match raw.parse::<u64>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(format!("{flag} must be a positive whole number, got {raw}")),
    }
}

fn parse_args() -> Parsed {
    // Everything below reports a usage failure the same way, so the fallible part
    // is written with `?` and wrapped once here.
    match parse_argv() {
        Ok(p) => p,
        Err(e) => Parsed::Error(e),
    }
}

fn parse_argv() -> Result<Parsed, String> {
    let mut argv = std::env::args().skip(1);
    let mode = match argv.next() {
        Some(m) => m,
        None => return Err(USAGE.to_string()),
    };
    if mode == "--help" || mode == "-h" || mode == "help" {
        return Ok(Parsed::Help(USAGE.to_string()));
    }
    if mode == "--version" || mode == "-V" || mode == "version" {
        return Ok(Parsed::Help(version_line()));
    }
    if mode == "--presets" || mode == "presets" {
        return Ok(Parsed::Help(format!(
            "Presets:\n{}\n\nUse one with: kaviri record --preset <name> ...",
            preset_help()
        )));
    }
    if mode == "--backgrounds" || mode == "backgrounds" {
        return Ok(Parsed::Help(format!(
            "Backgrounds:\n{}\n\nUse one with: kaviri record --background <name> ...\n\nCursors (--cursor):\n{}",
            backdrop::help(),
            ops::cursor_help()
        )));
    }
    /*
     * Held as text until the whole line has been read, so --cursor-scale works
     * whichever side of --cursor it lands on and a bad name is reported once,
     * with the list, rather than at the point it was parsed.
     */
    let mut background = "auto".to_string();
    let mut smooth = "auto".to_string();
    let mut cursor = "auto".to_string();
    let mut cursor_scale = DEFAULT_CURSOR_SCALE;
    let mut a = Args {
        mode,
        script: None,
        out: "kaviri-out.mp4".into(),
        port: None,
        width: 1470,
        height: 830,
        scale: 2.0,
        out_w: None,
        out_h: None,
        background: backdrop::Choice::Auto,
        smooth: zoom::Smooth::Auto,
        slowmo: 1.0,
        cursor: CursorCfg::default(),
        chromium: None,
        keep_temp: false,
        spool_dir: None,
        max_spool_bytes: None,
    };
    while let Some(flag) = argv.next() {
        let mut val = |name: &str| -> Result<String, String> {
            argv.next().ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--script" => a.script = Some(val("--script")?),
            "--out" => a.out = val("--out")?,
            "--port" => {
                let raw = val("--port")?;
                let p: u16 = raw.parse().map_err(|_| {
                    format!("--port must be a number between 1 and 65535, got {raw}")
                })?;
                if p == 0 {
                    return Err("--port must be between 1 and 65535, got 0".into());
                }
                a.port = Some(p);
            }
            "--width" => a.width = dimension("--width", &val("--width")?, 64, 16384)?,
            "--height" => a.height = dimension("--height", &val("--height")?, 64, 16384)?,
            "--scale" => a.scale = ratio("--scale", &val("--scale")?, 0.5, 4.0)?,
            "--out-width" => {
                a.out_w = Some(dimension("--out-width", &val("--out-width")?, 64, 8192)?)
            }
            "--out-height" => {
                a.out_h = Some(dimension("--out-height", &val("--out-height")?, 64, 8192)?)
            }
            /*
             * Applied where it is read, so an explicit --width after --preset still wins
             * and the preset stays a starting point rather than a cage.
             */
            "--preset" => {
                let name = val("--preset")?;
                let p = preset(&name).ok_or_else(|| {
                    format!("unknown preset: {name}\n\nPresets:\n{}", preset_help())
                })?;
                a.width = p.css.0;
                a.height = p.css.1;
                a.scale = p.scale;
                a.out_w = Some(p.out.0);
                a.out_h = Some(p.out.1);
            }
            "--presets" => {
                return Ok(Parsed::Help(format!("Presets:\n{}", preset_help())));
            }
            "--background" => background = val("--background")?,
            "--smooth" => smooth = val("--smooth")?,
            "--slowmo" => a.slowmo = ratio("--slowmo", &val("--slowmo")?, 1.0, 16.0)?,
            "--backgrounds" => {
                return Ok(Parsed::Help(format!("Backgrounds:\n{}", backdrop::help())));
            }
            "--cursor" => cursor = val("--cursor")?,
            "--cursor-scale" => {
                cursor_scale = ratio("--cursor-scale", &val("--cursor-scale")?, 0.2, 8.0)?
            }
            "--chromium" => a.chromium = Some(val("--chromium")?),
            "--spool-dir" => a.spool_dir = Some(PathBuf::from(val("--spool-dir")?)),
            "--max-spool-bytes" => {
                a.max_spool_bytes = Some(positive_u64(
                    "--max-spool-bytes",
                    &val("--max-spool-bytes")?,
                )?)
            }
            "--keep-temp" => a.keep_temp = true,
            "--audio" => {
                eprintln!("kaviri: --audio is not implemented yet (headless backend); ignoring")
            }
            "--help" | "-h" => return Ok(Parsed::Help(USAGE.to_string())),
            "--version" | "-V" => return Ok(Parsed::Help(version_line())),
            other => return Err(format!("unknown flag: {other}\n\n{USAGE}")),
        }
    }
    a.background = backdrop::parse_choice(&background)?;
    a.smooth = zoom::Smooth::parse(&smooth)?;
    a.cursor = CursorCfg::parse(&cursor, cursor_scale)?;
    Ok(Parsed::Run(Box::new(a)))
}

/// Everything a take needs from the machine, checked before anything is spent.
///
/// ffmpeg used to be looked up inside `zoom::render`, which only runs from
/// `stop_recording`: a machine without it launched the browser, drove the whole
/// script, spooled every frame and only then said it could not encode. The
/// frames were deleted with the session.
fn preflight() -> Result<String, String> {
    zoom::find_ffmpeg()
}

/// Report what kaviri found on this machine, and say so in one place rather than
/// in the middle of a take.
fn run_doctor(a: &Args) -> Result<(), String> {
    println!("{}", version_line());
    let mut ok = true;

    match probe_chromium(a.chromium.as_deref()) {
        Some((bin, ver)) => println!("chromium: {bin}\n  {ver}"),
        None => {
            ok = false;
            println!("chromium: NOT FOUND (set KAVIRI_CHROMIUM or pass --chromium)");
        }
    }
    match zoom::find_ffmpeg() {
        Ok(bin) => {
            let ver = first_line_of(&bin, "-version").unwrap_or_else(|| "(no version)".into());
            println!("ffmpeg: {bin}\n  {ver}");
        }
        Err(e) => {
            ok = false;
            println!("ffmpeg: NOT FOUND\n  {e}");
        }
    }
    // Reported rather than created: doctor must not leave a session directory
    // behind, since nothing here ever runs the Drop that would remove it.
    let spool = a
        .spool_dir
        .clone()
        .or_else(|| env::var_os("SPOOL_DIR").map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    println!("frame spool: under {}", spool.display());
    println!(
        "spool limit: {} MiB (--max-spool-bytes)",
        a.max_spool_bytes.unwrap_or(cdp::SPOOL_DEFAULT_MAX_BYTES) / (1024 * 1024)
    );

    if ok {
        Ok(())
    } else {
        Err("one or more dependencies are missing".into())
    }
}

fn first_line_of(bin: &str, arg: &str) -> Option<String> {
    let out = std::process::Command::new(bin)
        .arg(arg)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.trim().to_string())
}

/// The browser kaviri would use, and what it calls itself.
///
/// Deliberately separate from `cdp`'s own lookup: this one reports a version
/// string for a human, and must keep going past a candidate that is present but
/// does not run.
fn probe_chromium(explicit: Option<&str>) -> Option<(String, String)> {
    let mut cands: Vec<String> = Vec::new();
    if let Some(p) = explicit {
        cands.push(p.to_string());
    } else if let Some(p) = env::var("CHROMIUM") {
        cands.push(p);
    } else {
        for c in [
            "chromium",
            "chromium-browser",
            "google-chrome",
            "google-chrome-stable",
            "/snap/bin/chromium",
        ] {
            cands.push(c.to_string());
        }
    }
    for c in cands {
        if let Some(v) = first_line_of(&c, "--version") {
            return Some((c, v));
        }
    }
    None
}

/// One JSON line out, in the shape every mode uses.
fn emit<W: Write + ?Sized>(w: &mut W, v: &Value) -> std::io::Result<()> {
    writeln!(w, "{v}")?;
    w.flush()
}

fn ok_envelope(result: Value) -> Value {
    json!({"ok": true, "result": result})
}

fn err_envelope(error: &str) -> Value {
    json!({"ok": false, "error": error})
}

fn run_record(a: &Args) -> Result<(), String> {
    let script_path = a.script.as_ref().ok_or("record mode needs --script")?;
    let body = std::fs::read_to_string(script_path)
        .map_err(|e| format!("cannot read {script_path}: {e}"))?;
    let mut ops_list: Vec<Value> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("{script_path}:{}: {e}", i + 1))?;
        ops_list.push(v);
    }
    let has_start = ops_list.iter().any(|o| o["op"] == "start_recording");

    let ffmpeg = preflight()?;
    if env::is_set("DEBUG") {
        eprintln!("kaviri[debug]: ffmpeg at {ffmpeg}");
    }

    eprintln!(
        "kaviri {}: launching browser ({}x{}@{}x) ...",
        env!("CARGO_PKG_VERSION"),
        a.width,
        a.height,
        a.scale
    );
    let mut s = Session::launch(
        a.chromium.as_deref(),
        a.width,
        a.height,
        a.scale,
        (a.out_w.unwrap_or(a.width), a.out_h.unwrap_or(a.height)),
        a.keep_temp,
        a.cursor,
    )?;
    s.out_path = Some(a.out.clone());
    s.background = a.background;
    s.smooth = a.smooth;
    s.set_slowmo(a.slowmo)?;
    s.cdp
        .set_spool_config(a.spool_dir.clone(), a.max_spool_bytes);
    s.cdp.set_keep_spool(a.keep_temp);

    let mut out = std::io::stdout();
    /*
     * The first error ends the script but not the take: every frame captured so
     * far is already on disk, and a partial video plus a clear error is far more
     * use than the nothing this used to produce. The exit code still says the
     * take was incomplete.
     */
    let mut failure: Option<String> = None;

    if !has_start {
        match s.exec(&json!({"op": "start_recording", "path": a.out.clone()})) {
            Ok(v) => {
                let _ = emit(&mut out, &ok_envelope(v));
            }
            Err(e) => {
                let _ = emit(&mut out, &err_envelope(&e));
                return Err(e);
            }
        }
    }
    for (i, op) in ops_list.iter().enumerate() {
        if cdp::shutting_down() {
            failure = Some("interrupted".into());
            break;
        }
        // --out wins over any path inside the script.
        let mut op = op.clone();
        if op["op"] == "start_recording" {
            op["path"] = json!(a.out.clone());
        }
        match s.exec(&op) {
            Ok(v) => {
                let _ = emit(&mut out, &ok_envelope(v));
            }
            Err(e) => {
                let kind = op["op"].as_str().unwrap_or("?").to_string();
                let msg = format!("{script_path} op {} ({kind}): {e}", i + 1);
                let _ = emit(
                    &mut out,
                    &json!({"ok": false, "error": e, "op": kind, "index": i + 1}),
                );
                failure = Some(msg);
                break;
            }
        }
    }
    if s.cdp.is_recording() {
        match s.exec(&json!({"op": "stop_recording"})) {
            Ok(v) => {
                let _ = emit(&mut out, &ok_envelope(v));
            }
            Err(e) => {
                let _ = emit(&mut out, &err_envelope(&e));
                if failure.is_none() {
                    failure = Some(e);
                }
            }
        }
    }
    if let Some((dur, n)) = s.rendered {
        eprintln!("kaviri: done: {} ({dur:.1}s, {n} zoom events)", a.out);
    }
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// What a reader had for us this turn.
enum Incoming {
    Line(String),
    /// Nothing arrived inside the tick budget. This is the common case in serve
    /// mode, and the only reason capture keeps running through it.
    Idle,
    Eof,
}

/// A line-at-a-time source that hands control back on a deadline.
///
/// `BufRead::lines()` cannot do this: it blocks until the client says something,
/// and while it blocks nothing pumps CDP. That is what made serve mode record a
/// sequence of freeze frames covering only the moments kaviri was already busy.
trait LineSource {
    fn next_line(&mut self, budget: Duration) -> Incoming;
}

/// Longest capture is allowed to stall while waiting for the next op. The
/// screenshot pump targets 25ms, so this is one frame.
const TICK: Duration = Duration::from_millis(25);

/// A line longer than this is not an op. Bounded so a client that never sends a
/// newline cannot grow the buffer without limit.
const MAX_LINE_BYTES: usize = 1 << 20;

fn take_line(buf: &mut Vec<u8>) -> Option<String> {
    let nl = buf.iter().position(|b| *b == b'\n')?;
    let line: Vec<u8> = buf.drain(..=nl).collect();
    Some(String::from_utf8_lossy(&line).trim().to_string())
}

struct SocketLines {
    sock: std::net::TcpStream,
    buf: Vec<u8>,
}

impl LineSource for SocketLines {
    fn next_line(&mut self, budget: Duration) -> Incoming {
        if let Some(l) = take_line(&mut self.buf) {
            return Incoming::Line(l);
        }
        if self.sock.set_read_timeout(Some(budget)).is_err() {
            return Incoming::Eof;
        }
        let mut chunk = [0u8; 8192];
        match self.sock.read(&mut chunk) {
            Ok(0) => Incoming::Eof,
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                if self.buf.len() > MAX_LINE_BYTES {
                    eprintln!(
                        "kaviri: dropping a client that sent {} bytes with no newline",
                        MAX_LINE_BYTES
                    );
                    return Incoming::Eof;
                }
                match take_line(&mut self.buf) {
                    Some(l) => Incoming::Line(l),
                    None => Incoming::Idle,
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Incoming::Idle
            }
            Err(_) => Incoming::Eof,
        }
    }
}

/// stdin, read on its own thread.
///
/// A pipe has no read timeout, so the only way to stop blocking on it is not to
/// block on it: the thread owns the blocking read and the op loop waits on the
/// channel with a deadline it can keep.
struct StdinLines {
    rx: std::sync::mpsc::Receiver<String>,
}

impl StdinLines {
    fn spawn() -> StdinLines {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        StdinLines { rx }
    }
}

impl LineSource for StdinLines {
    fn next_line(&mut self, budget: Duration) -> Incoming {
        use std::sync::mpsc::RecvTimeoutError;
        match self.rx.recv_timeout(budget) {
            Ok(l) => Incoming::Line(l),
            Err(RecvTimeoutError::Timeout) => Incoming::Idle,
            Err(RecvTimeoutError::Disconnected) => Incoming::Eof,
        }
    }
}

/// Take the session lock, saying so loudly if a previous op panicked while
/// holding it. A poisoned session may be half-mutated, and silently carrying on
/// is how the next client inherits it.
fn session(s: &Mutex<Session>) -> MutexGuard<'_, Session> {
    s.lock().unwrap_or_else(|p| {
        eprintln!("kaviri: warning: an op panicked while holding the session; the browser may be in an unknown state");
        p.into_inner()
    })
}

fn try_session(s: &Mutex<Session>) -> Option<MutexGuard<'_, Session>> {
    match s.try_lock() {
        Ok(g) => Some(g),
        Err(std::sync::TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

/// One capture step, if nobody else is holding the browser.
///
/// `try_lock` rather than `lock`: an op that is already running is pumping CDP
/// itself, so waiting for it would buy nothing and would serialize every reader
/// behind a long navigate.
fn tick(s: &Mutex<Session>) {
    if let Some(mut g) = try_session(s) {
        // A tick that fails has already stopped capture and said why; the next op
        // reports it in band.
        let _ = g.cdp.tick();
    }
}

/// How a client connection ended.
struct StreamOutcome {
    failed_ops: usize,
}

/// Drive one client until it hangs up, keeping capture running in the gaps.
fn serve_stream<L: LineSource, W: Write>(
    s: &Mutex<Session>,
    src: &mut L,
    writer: &mut W,
    out_path: &str,
    token: Option<&str>,
) -> StreamOutcome {
    let mut failed_ops = 0usize;
    // `None` once the client has said hello, or immediately when no token is
    // required (stdin is already a private channel).
    let mut awaiting_hello = token.is_some();
    // A connection that speaks a token also speaks strict JSON: one bad line is a
    // disconnect, so an HTTP request from a web page cannot half-execute a script.
    let strict = token.is_some();

    loop {
        if cdp::shutting_down() {
            break;
        }
        let line = match src.next_line(TICK) {
            Incoming::Line(l) => l,
            Incoming::Idle => {
                tick(s);
                continue;
            }
            Incoming::Eof => break,
        };
        // Same comment and blank-line handling the record path applies, so the
        // bundled example scripts can be piped straight in.
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let op: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = emit(&mut *writer, &err_envelope(&format!("bad json: {e}")));
                if strict {
                    break;
                }
                continue;
            }
        };
        if awaiting_hello {
            if !hello_accepted(&op, token.unwrap_or("")) {
                let _ = emit(
                    &mut *writer,
                    &err_envelope(
                        "this connection must start with {\"op\":\"hello\",\"token\":\"…\"}; \
                         the token was printed on stderr at startup",
                    ),
                );
                break;
            }
            awaiting_hello = false;
            let _ = emit(
                &mut *writer,
                &ok_envelope(json!({"event": "hello", "version": env!("CARGO_PKG_VERSION")})),
            );
            continue;
        }
        /*
         * One browser, so ops are serialized here rather than raced. The lock is
         * held for a single op: a client that goes quiet mid-script blocks nobody,
         * and a slow op (a navigate, a render) makes the others wait their turn
         * instead of interleaving into the same page.
         */
        let mut op = op;
        if op["op"] == "start_recording" {
            /*
             * A client-supplied path is an arbitrary file write: the MP4 and its
             * telemetry sidecar both land wherever it points. Record mode has always
             * overridden it with --out, and serve mode has no reason to be laxer.
             */
            if op.get("path").is_some() && op["path"] != json!(out_path) {
                eprintln!("kaviri: ignoring start_recording path; the output is {out_path}");
            }
            op["path"] = json!(out_path);
        }
        let resp = {
            let mut guard = session(s);
            match guard.exec(&op) {
                Ok(v) => ok_envelope(v),
                Err(e) => {
                    failed_ops += 1;
                    err_envelope(&e)
                }
            }
        };
        if emit(&mut *writer, &resp).is_err() {
            break;
        }
    }
    StreamOutcome { failed_ops }
}

/// Whether a connection's first line is a valid handshake.
///
/// Loopback is not a trust boundary against a browser: a page the user happens to
/// be visiting can POST to 127.0.0.1, and the op set includes navigating to
/// `file://` and writing an MP4. The token is what makes that fetch useless.
fn hello_accepted(op: &Value, token: &str) -> bool {
    !token.is_empty() && op["op"] == "hello" && op["token"].as_str() == Some(token)
}

/// A secret no other process on the machine can guess.
///
/// `RandomState` is seeded by the OS, which is the only entropy the standard
/// library exposes without pulling in a dependency.
fn random_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..2u32 {
        let mut h = RandomState::new().build_hasher();
        h.write_u32(std::process::id());
        h.write_u32(i);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Client threads alive right now, so the accept loop can refuse to spawn an
/// unbounded number of them and so shutdown can wait for them.
static CLIENTS: AtomicUsize = AtomicUsize::new(0);
const MAX_CLIENTS: usize = 16;

fn run_serve(a: &Args) -> Result<(), String> {
    let ffmpeg = preflight()?;
    if env::is_set("DEBUG") {
        eprintln!("kaviri[debug]: ffmpeg at {ffmpeg}");
    }

    eprintln!(
        "kaviri {}: launching browser ({}x{}@{}x) ...",
        env!("CARGO_PKG_VERSION"),
        a.width,
        a.height,
        a.scale
    );
    let mut sess = Session::launch(
        a.chromium.as_deref(),
        a.width,
        a.height,
        a.scale,
        (a.out_w.unwrap_or(a.width), a.out_h.unwrap_or(a.height)),
        a.keep_temp,
        a.cursor,
    )?;
    sess.out_path = Some(a.out.clone());
    sess.background = a.background;
    sess.smooth = a.smooth;
    sess.set_slowmo(a.slowmo)?;
    sess.cdp
        .set_spool_config(a.spool_dir.clone(), a.max_spool_bytes);
    sess.cdp.set_keep_spool(a.keep_temp);
    let s = Arc::new(Mutex::new(sess));

    let failed = Arc::new(AtomicUsize::new(0));
    match a.port {
        Some(port) => {
            /*
             * A loopback socket is not a trust boundary against a browser: any page
             * the user visits can POST to it. Without a token, a drive-by fetch could
             * navigate this browser to file:// and record the result.
             */
            let token = env::var("TOKEN").unwrap_or_else(random_token);
            let listener = std::net::TcpListener::bind(("127.0.0.1", port))
                .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
            listener
                .set_nonblocking(true)
                .map_err(|e| format!("listener: {e}"))?;
            eprintln!("kaviri: listening on 127.0.0.1:{port} (NDJSON ops, concurrent connections)");
            eprintln!("kaviri: token {token}");
            eprintln!(
                "kaviri: every connection must begin with {{\"op\":\"hello\",\"token\":\"{token}\"}}"
            );
            eprintln!("kaviri: --port opens a local control socket; prefer stdin mode when one client is enough");

            let mut accept_errors = 0u32;
            loop {
                if cdp::shutting_down() {
                    eprintln!("kaviri: interrupted; shutting down");
                    break;
                }
                match listener.accept() {
                    Ok((conn, _)) => {
                        accept_errors = 0;
                        if CLIENTS.load(Ordering::Relaxed) >= MAX_CLIENTS {
                            eprintln!(
                                "kaviri: refusing a connection; {} clients already attached",
                                MAX_CLIENTS
                            );
                            continue;
                        }
                        let peer = conn
                            .peer_addr()
                            .map(|p| p.to_string())
                            .unwrap_or_else(|_| "?".into());
                        // The listener is non-blocking so the accept loop can see a
                        // signal; the connection itself is driven by its own timeout.
                        if let Err(e) = conn.set_nonblocking(false) {
                            eprintln!("kaviri: cannot configure socket for {peer}: {e}");
                            continue;
                        }
                        let writer = match conn.try_clone() {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("kaviri: cannot clone socket for {peer}: {e}");
                                continue;
                            }
                        };
                        let shared = Arc::clone(&s);
                        let failed = Arc::clone(&failed);
                        let token = token.clone();
                        let out_path = a.out.clone();
                        CLIENTS.fetch_add(1, Ordering::Relaxed);
                        std::thread::spawn(move || {
                            eprintln!("kaviri: client {peer} connected");
                            let mut src = SocketLines {
                                sock: conn,
                                buf: Vec::new(),
                            };
                            let mut w = writer;
                            let outcome =
                                serve_stream(&shared, &mut src, &mut w, &out_path, Some(&token));
                            failed.fetch_add(outcome.failed_ops, Ordering::Relaxed);
                            eprintln!("kaviri: client {peer} disconnected");
                            CLIENTS.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        /*
                         * Nobody is attached, but a take may still be running: keep the
                         * capture clock going here too, then wait out the rest of the
                         * tick instead of spinning on accept.
                         */
                        let before = Instant::now();
                        tick(&s);
                        if let Some(rest) = TICK.checked_sub(before.elapsed()) {
                            std::thread::sleep(rest);
                        }
                    }
                    Err(e) => {
                        /*
                         * EMFILE and ENFILE persist until a descriptor is freed, so
                         * retrying flat out would spin at 100% CPU on the machine that
                         * is trying to capture frames.
                         */
                        accept_errors += 1;
                        eprintln!("kaviri: accept failed: {e}");
                        if accept_errors >= 50 {
                            return Err(format!("accept kept failing: {e}"));
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            // Give the clients a moment to notice the shutdown flag and let go of
            // the session, so its Drop can close the browser.
            let deadline = Instant::now() + Duration::from_secs(2);
            while CLIENTS.load(Ordering::Relaxed) > 0 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        None => {
            eprintln!("kaviri: reading NDJSON ops from stdin");
            let mut src = StdinLines::spawn();
            let mut out = std::io::stdout();
            let outcome = serve_stream(&s, &mut src, &mut out, &a.out, None);
            failed.fetch_add(outcome.failed_ops, Ordering::Relaxed);
        }
    }

    // EOF, or a signal: finish any open recording rather than dropping the frames.
    let mut unrendered = false;
    {
        let mut guard = session(&s);
        if guard.cdp.is_recording() {
            let mut out = std::io::stdout();
            match guard.exec(&json!({"op": "stop_recording"})) {
                Ok(v) => {
                    let _ = emit(&mut out, &ok_envelope(v));
                }
                Err(e) => {
                    let _ = emit(&mut out, &err_envelope(&e));
                    unrendered = true;
                }
            }
        }
        if guard.rendered.is_none() && !guard.marks.is_empty() {
            unrendered = true;
        }
    }

    /*
     * An exit status of 0 from a mode where every op failed and no video exists is
     * a green CI run over a broken take.
     */
    let failed = failed.load(Ordering::Relaxed);
    if failed > 0 {
        return Err(format!("{failed} op(s) failed"));
    }
    if unrendered {
        return Err("a recording was started but never rendered".into());
    }
    Ok(())
}

fn main() {
    // First: every cleanup kaviri does lives in a Drop, and the default signal
    // disposition runs none of them.
    cdp::install_signal_handlers();

    let a = match parse_args() {
        Parsed::Run(a) => a,
        Parsed::Help(msg) => {
            println!("{msg}");
            std::process::exit(0);
        }
        Parsed::Error(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let r = match a.mode.as_str() {
        "record" => run_record(&a),
        "serve" => run_serve(&a),
        "doctor" => run_doctor(&a),
        m => Err(format!("unknown mode: {m}\n\n{USAGE}")),
    };
    if let Err(e) = r {
        eprintln!("kaviri: error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dimension_flag_rejects_the_values_that_only_fail_at_render_time() {
        assert!(dimension("--width", "0", 64, 16384).is_err());
        assert!(dimension("--width", "1", 64, 16384).is_err());
        assert!(dimension("--width", "-8", 64, 16384).is_err());
        assert!(dimension("--width", "99999", 64, 16384).is_err());
        assert_eq!(dimension("--width", "1470", 64, 16384), Ok(1470));
    }

    #[test]
    fn scale_rejects_nan_and_infinity() {
        // Rust's f64 parser accepts both spellings, and NaN survives every clamp
        // in the zoom planner to arrive in the ffmpeg expression as the literal
        // "NaN".
        assert!(ratio("--scale", "nan", 0.5, 4.0).is_err());
        assert!(ratio("--scale", "inf", 0.5, 4.0).is_err());
        assert!(ratio("--scale", "0", 0.5, 4.0).is_err());
        assert!(ratio("--scale", "8", 0.5, 4.0).is_err());
        assert_eq!(ratio("--scale", "2", 0.5, 4.0), Ok(2.0));
    }

    #[test]
    fn lines_are_split_on_newlines_and_the_remainder_is_kept() {
        let mut buf = b"{\"op\":\"mark\"}\n{\"op\":\"wa".to_vec();
        assert_eq!(take_line(&mut buf).as_deref(), Some("{\"op\":\"mark\"}"));
        assert_eq!(take_line(&mut buf), None);
        buf.extend_from_slice(b"it\"}\r\n");
        assert_eq!(take_line(&mut buf).as_deref(), Some("{\"op\":\"wait\"}"));
        assert!(buf.is_empty());
    }

    #[test]
    fn only_a_matching_hello_opens_a_port_connection() {
        let t = "abc123";
        assert!(hello_accepted(&json!({"op": "hello", "token": t}), t));
        assert!(!hello_accepted(
            &json!({"op": "hello", "token": "wrong"}),
            t
        ));
        assert!(!hello_accepted(&json!({"op": "hello"}), t));
        // The op a drive-by fetch would send first.
        assert!(!hello_accepted(
            &json!({"op": "navigate", "url": "file:///etc/passwd"}),
            t
        ));
        // An empty token must never be accepted, whatever the client claims.
        assert!(!hello_accepted(&json!({"op": "hello", "token": ""}), ""));
    }

    #[test]
    fn the_tick_budget_is_no_longer_than_one_frame() {
        // Capture targets 25ms; a reader that waited longer than that would leave a
        // visible freeze in the gaps between ops, which is the defect this exists
        // to close.
        assert!(TICK <= Duration::from_millis(25));
    }
}
