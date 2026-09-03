//! Minimal synchronous Chrome DevTools Protocol client over one websocket.
//!
//! Single-threaded pump design: the socket has a short read timeout and every
//! wait (command response, event, sleep) drains incoming messages, so
//! screencast frames keep flowing and get acked no matter what the caller is
//! doing.

use base64::Engine;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub struct Frame {
    /// Seconds on the recording clock (relative to `rec_t0`).
    pub t: f64,
    pub jpeg: Vec<u8>,
}

pub struct Cdp {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    child: Child,
    next_id: u64,
    session_id: String,
    responses: HashMap<u64, Value>,
    ack_ids: HashSet<u64>,
    events: VecDeque<(String, Value)>,
    pub frames: Vec<Frame>,
    pub rec_t0: Option<Instant>,
    recording: bool,
    _profile_dir: PathBuf,
}

fn free_port() -> std::io::Result<u16> {
    let l = TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

fn http_get(port: u16, path: &str) -> std::io::Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )?;
    let mut buf = String::new();
    s.read_to_string(&mut buf)?;
    match buf.split_once("\r\n\r\n") {
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
        let port = free_port().map_err(|e| e.to_string())?;
        let profile_dir = std::env::temp_dir().join(format!("lensa-profile-{port}"));
        std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
        let child = Command::new(&bin)
            .args([
                "--headless=new",
                &format!("--remote-debugging-port={port}"),
                &format!("--user-data-dir={}", profile_dir.display()),
                &format!("--window-size={css_w},{css_h}"),
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--disable-gpu",
                "--hide-scrollbars",
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
                }
                Err(_) => {}
            }
            if Instant::now() > deadline {
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
            frames: Vec::new(),
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
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(resp) = self.responses.remove(&id) {
                if let Some(err) = resp.get("error") {
                    return Err(format!("{method}: {err}"));
                }
                return Ok(resp["result"].clone());
            }
            if Instant::now() > deadline {
                return Err(format!("{method}: timed out waiting for response"));
            }
            self.pump()?;
        }
    }

    pub fn send(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let session = self.session_id.clone();
        self.send_raw(method, params, Some(&session))
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
                        self.frames.push(Frame {
                            t: t0.elapsed().as_secs_f64(),
                            jpeg,
                        });
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

    pub fn start_screencast(&mut self, max_w: u32, max_h: u32) -> Result<(), String> {
        self.rec_t0 = Some(Instant::now());
        self.recording = true;
        self.frames.clear();
        self.send(
            "Page.startScreencast",
            json!({"format": "jpeg", "quality": 82, "maxWidth": max_w, "maxHeight": max_h, "everyNthFrame": 1}),
        )?;
        Ok(())
    }

    pub fn stop_screencast(&mut self) -> Result<(), String> {
        self.send("Page.stopScreencast", json!({}))?;
        // Drain stragglers.
        self.sleep_pump(150)?;
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
        let _ = self.child.kill();
        let _ = self.child.wait();
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
