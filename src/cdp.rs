//! Minimal synchronous Chrome DevTools Protocol client over one websocket.
//!
//! Single-threaded pump design: the socket has a short read timeout and every
//! wait (command response, event, sleep) drains incoming messages, so
//! screencast frames keep flowing and get acked no matter what the caller is
//! doing.

use base64::Engine;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

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
}

impl FrameSpool {
    pub fn new() -> FrameSpool {
        FrameSpool { file: None, path: PathBuf::new(), frames: Vec::new(), bytes: 0 }
    }

    /// Open a fresh spool. The old one, if any, is dropped and its file removed.
    pub fn reset(&mut self) -> Result<(), String> {
        self.discard();
        let path = std::env::temp_dir().join(format!(
            "lensa-spool-{}-{}.jpgs",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        ));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| format!("open frame spool {}: {e}", path.display()))?;
        self.file = Some(file);
        self.path = path;
        Ok(())
    }

    fn push(&mut self, t: f64, jpeg: &[u8]) -> Result<(), String> {
        let file = match self.file.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        file.write_all(jpeg).map_err(|e| format!("spool write: {e}"))?;
        self.frames.push(Frame { t, offset: self.bytes, len: jpeg.len() as u32 });
        self.bytes += jpeg.len() as u64;
        Ok(())
    }

    /// Read one frame's JPEG back off the spool.
    pub fn read(&self, i: usize) -> Result<Vec<u8>, String> {
        let frame = self.frames.get(i).ok_or("frame index out of range")?;
        let mut file = std::fs::File::open(&self.path).map_err(|e| format!("spool open: {e}"))?;
        file.seek(SeekFrom::Start(frame.offset)).map_err(|e| format!("spool seek: {e}"))?;
        let mut buf = vec![0u8; frame.len as usize];
        file.read_exact(&mut buf).map_err(|e| format!("spool read: {e}"))?;
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

    fn discard(&mut self) {
        self.file = None;
        if !self.path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.path);
        }
        self.frames.clear();
        self.bytes = 0;
        self.path = PathBuf::new();
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
    /// Poll `Page.captureScreenshot` at roughly this interval.
    Screenshot(Duration),
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
    _profile_dir: PathBuf,
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
    if let Ok(p) = std::env::var("LENSA_CHROMIUM") {
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
    Err("no Chromium/Chrome binary found (set LENSA_CHROMIUM or use --chromium)".into())
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
        let profile_dir = std::env::temp_dir().join(format!("lensa-profile-{port}"));
        std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
        let mut child = Command::new(&bin)
            .args([
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
                "--disable-gpu",
                "--hide-scrollbars",
                /*
                 * No speculative prerendering. A site with speculation rules builds the
                 * next page in a hidden target, and a click activates that target in
                 * place of the one lensa is attached to: the take then freezes on the
                 * old page while the browser moves on, with a 30s CDP stall at the
                 * swap. One page, one target, for the whole recording.
                 */
                "--disable-features=Prerender2",
                "--mute-audio",
                "--force-color-profile=srgb",
                "about:blank",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn {bin}: {e}"))?;

        // Wait for the debugger endpoint.
        let deadline = Instant::now() + Duration::from_secs(40);
        let ws_url = loop {
            match http_get(port, "/json/version") {
                Ok(body) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&body) {
                        if let Some(u) = v["webSocketDebuggerUrl"].as_str() {
                            break u.to_string();
                        }
                    }
                    if std::env::var_os("LENSA_DEBUG").is_some() {
                        eprintln!("lensa[debug]: /json/version body without ws url: {body}");
                    }
                }
                Err(e) => {
                    if std::env::var_os("LENSA_DEBUG").is_some() {
                        eprintln!("lensa[debug]: /json/version poll failed: {e}");
                    }
                }
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_dir_all(&profile_dir);
                return Err("chromium did not expose a debugger endpoint in 40s".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        };

        let (mut ws, _) = tungstenite::connect(&ws_url)
            .map_err(|e| format!("websocket connect failed: {e}"))?;
        if let MaybeTlsStream::Plain(s) = ws.get_mut() {
            s.set_read_timeout(Some(Duration::from_millis(30)))
                .map_err(|e| e.to_string())?;
        }

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
            rec_t0: None,
            recording: false,
            _profile_dir: profile_dir,
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
            if Instant::now() > deadline {
                /* A late answer is discarded on arrival rather than piling up unread. */
                self.ack_ids.insert(id);
                return Err(format!("{method}: timed out waiting for response"));
            }
            self.pump()?;
        }
    }

    pub fn send(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let session = self.session_id.clone();
        self.send_raw(method, params, Some(&session))
    }

    /// As `send`, but gives up after `timeout`. For calls that are cheap to lose.
    pub fn send_within(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let session = self.session_id.clone();
        self.send_raw_within(method, params, Some(&session), timeout)
    }

    /// Drain any pending websocket messages (non-blocking beyond the socket's
    /// 30ms read timeout). Frames are acked and stored here.
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
                Err(e) => return Err(format!("ws read: {e}")),
            }
        }
    }

    fn handle_message(&mut self, txt: &str) -> Result<(), String> {
        let v: Value = match serde_json::from_str(txt) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        if let Some(id) = v["id"].as_u64() {
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
                        self.frames.push(t0.elapsed().as_secs_f64(), &jpeg)?;
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

    /// Wait until an event with this method arrives (consumes it).
    pub fn wait_event(&mut self, method: &str, timeout: Duration) -> Result<bool, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(pos) = self.events.iter().position(|(m, _)| m == method) {
                self.events.remove(pos);
                return Ok(true);
            }
            if Instant::now() > deadline {
                return Ok(false);
            }
            self.pump()?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Sleep while keeping the frame pump alive.
    pub fn sleep_pump(&mut self, ms: u64) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            self.pump()?;
            /*
             * Every wait in the op protocol lands here, so this is where the screenshot
             * pump gets its cadence: no separate thread, no second CDP connection, and
             * the frame clock stays the recording clock.
             */
            self.shoot_if_due()?;
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
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

    /// Begin capturing. `scale` above 1 switches to the screenshot pump, which is the
    /// only path that yields more pixels than the CSS viewport has.
    pub fn start_capture(&mut self, max_w: u32, max_h: u32, scale: f64) -> Result<(), String> {
        self.rec_t0 = Some(Instant::now());
        self.recording = true;
        self.frames.reset()?;
        self.last_shot = None;
        if scale > 1.0 {
            /*
             * 25ms is a target, not a guarantee: a screenshot of a heavy page takes
             * longer than that and the pump simply falls behind, which the CFR pass
             * absorbs. Asking for 30fps here would only queue round trips.
             */
            self.capture = Capture::Screenshot(Duration::from_millis(25));
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

    /// Take one screenshot and spool it. Errors are swallowed on purpose: a frame lost
    /// to a navigation in flight should not end a take.
    fn shoot(&mut self) -> Result<(), String> {
        let t0 = match self.rec_t0 {
            Some(t) => t,
            None => return Ok(()),
        };
        let t = t0.elapsed().as_secs_f64();
        /*
         * No clip. The window and the emulation already give the viewport device pixels,
         * so a plain viewport screenshot is css * scale. A clip is in document
         * coordinates, not viewport ones: pinned at y 0 it pointed above the fold as soon
         * as the page scrolled and came back blank, and carrying the scale as well
         * squared it.
         *
         * Short timeout. While a navigation is in flight the screenshot cannot answer until
         * the new page paints, and under the general 30s timeout that one lost frame froze
         * the take for half a minute. A frame is cheap to skip; the pump tries again 25ms
         * later and the CFR pass holds the previous one in between.
         */
        let r = match self.send_within(
            "Page.captureScreenshot",
            json!({
                "format": "jpeg", "quality": 82,
                "optimizeForSpeed": true,
                "captureBeyondViewport": false,
            }),
            Duration::from_millis(700),
        ) {
            Ok(r) => r,
            Err(_) => {
                self.last_shot = Some(Instant::now());
                return Ok(());
            }
        };
        if let Some(data) = r["data"].as_str() {
            if let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(data) {
                self.frames.push(t, &jpeg)?;
            }
        }
        self.last_shot = Some(Instant::now());
        Ok(())
    }

    /// Take a screenshot if one is due. No-op in screencast mode.
    fn shoot_if_due(&mut self) -> Result<(), String> {
        let interval = match self.capture {
            Capture::Screenshot(i) if self.recording => i,
            _ => return Ok(()),
        };
        let due = self.last_shot.map(|t| t.elapsed() >= interval).unwrap_or(true);
        if due {
            self.shoot()?;
        }
        Ok(())
    }

    pub fn stop_capture(&mut self) -> Result<(), String> {
        if self.capture == Capture::Screencast {
            self.send("Page.stopScreencast", json!({}))?;
        } else {
            // One last frame so the tail is the page as it finally looked.
            self.shoot()?;
        }
        // Drain stragglers.
        self.sleep_pump(150)?;
        // The screencast only sends frames on paint, so a static tail would
        // otherwise be cut off: hold the last frame until stop time.
        let tail = self.frames.frames().last().map(|f| f.t);
        if let Some(last_t) = tail {
            let t_stop = self.now_rec();
            if t_stop > last_t + 0.05 {
                let jpeg = self.frames.read(self.frames.len() - 1)?;
                self.frames.push(t_stop, &jpeg)?;
            }
        }
        self.recording = false;
        Ok(())
    }

    /// Seconds on the recording clock right now (0.0 if not recording).
    pub fn now_rec(&self) -> f64 {
        self.rec_t0.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }
}

impl Drop for Cdp {
    fn drop(&mut self) {
        // Ask the browser to exit via CDP first: on snap systems the spawned
        // child is a wrapper script, so kill() alone would leak the browser.
        let id = self.next_id;
        let msg = json!({"id": id, "method": "Browser.close", "params": {}});
        let _ = self.ws.send(Message::Text(msg.to_string()));
        std::thread::sleep(Duration::from_millis(300));
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self._profile_dir);
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
