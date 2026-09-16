//! Agent-facing control protocol: newline-delimited JSON ops.
//!
//! Every executed op auto-emits a timestamped mark (on the recording clock)
//! carrying the target element's bounding box in CSS pixels; the zoom
//! generator consumes these.

use crate::cdp::Cdp;
use serde_json::{json, Value};
use std::time::Duration;

pub struct Mark {
    pub t: f64,
    pub kind: String,
    pub label: String,
    /// CSS-pixel viewport box (x, y, w, h) of the interaction target.
    pub bbox: Option<(f64, f64, f64, f64)>,
}

pub struct Session {
    pub cdp: Cdp,
    pub marks: Vec<Mark>,
    pub css_w: u32,
    pub css_h: u32,
    pub scale: f64,
    /// The video's size. Independent of the viewport, so a phone-shaped take can still
    /// be a 1080p file.
    pub out_size: (u32, u32),
    pub out_path: Option<String>,
    pub keep_temp: bool,
    pub rendered: Option<(f64, usize)>,
}

/// Injected into every document: a fake cursor + click ripple so the video
/// shows pointer motion (headless screencast has no OS cursor).
const CURSOR_JS: &str = r##"
(() => {
  const ensure = () => {
    let c = document.getElementById('__lensa_cursor');
    if (c) return c;
    c = document.createElement('div');
    c.id = '__lensa_cursor';
    c.style.cssText = 'position:fixed;left:0;top:0;width:24px;height:24px;' +
      'z-index:2147483647;pointer-events:none;' +
      'transition:transform .45s cubic-bezier(.22,.61,.36,1);' +
      'transform:translate(-100px,-100px);will-change:transform';
    c.innerHTML = '<svg width="24" height="24" viewBox="0 0 24 24">' +
      '<path d="M4 2 L4 19 L8.5 15.5 L11.5 22 L14.5 20.5 L11.5 14 L17 13.5 Z"' +
      ' fill="#111" stroke="#fff" stroke-width="1.5"/></svg>';
    (document.body || document.documentElement).appendChild(c);
    return c;
  };
  window.__lensa = {
    move(x, y) { ensure().style.transform = `translate(${x-4}px,${y-2}px)`; },
    ripple(x, y) {
      if (!document.getElementById('__lensa_style')) {
        const s = document.createElement('style'); s.id = '__lensa_style';
        s.textContent = '@keyframes __lensa_r{from{transform:scale(.4);opacity:1}' +
          'to{transform:scale(1.7);opacity:0}}';
        (document.head || document.documentElement).appendChild(s);
      }
      const r = document.createElement('div');
      r.style.cssText = `position:fixed;left:${x-18}px;top:${y-18}px;` +
        'width:36px;height:36px;border-radius:50%;' +
        'border:3px solid rgba(59,130,246,.85);z-index:2147483646;' +
        'pointer-events:none;animation:__lensa_r .5s ease-out forwards';
      (document.body || document.documentElement).appendChild(r);
      setTimeout(() => r.remove(), 600);
    }
  };
})();
"##;

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

impl Session {
    pub fn launch(
        chromium: Option<&str>,
        css_w: u32,
        css_h: u32,
        scale: f64,
        out_size: (u32, u32),
        keep_temp: bool,
    ) -> Result<Session, String> {
        let mut cdp = Cdp::launch(chromium, css_w, css_h, scale)?;
        cdp.send(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({"source": CURSOR_JS}),
        )?;
        Ok(Session {
            cdp,
            marks: Vec::new(),
            css_w,
            css_h,
            scale,
            out_size,
            out_path: None,
            keep_temp,
            rendered: None,
        })
    }

    fn mark(&mut self, kind: &str, label: &str, bbox: Option<(f64, f64, f64, f64)>) -> Value {
        let t = self.cdp.now_rec();
        if self.cdp.is_recording() {
            self.marks.push(Mark {
                t,
                kind: kind.to_string(),
                label: label.to_string(),
                bbox,
            });
        }
        json!({
            "event": "mark", "t": t, "kind": kind, "label": label,
            "box": bbox.map(|(x, y, w, h)| vec![x, y, w, h]),
            "recording": self.cdp.is_recording(),
        })
    }

    /// Scroll element into view, settle, return its viewport box (CSS px).
    fn resolve_box(&mut self, selector: &str) -> Result<(f64, f64, f64, f64), String> {
        let sel = js_string(selector);
        let js = format!(
            "(() => {{ const el = document.querySelector({sel}); if (!el) return null; \
             el.scrollIntoView({{block:'center', inline:'center', behavior:'instant'}}); \
             return true; }})()"
        );
        let found = self.cdp.evaluate(&js)?;
        if found.is_null() {
            return Err(format!("selector not found: {selector}"));
        }
        self.cdp.sleep_pump(120)?; // let layout/scroll settle
        let js = format!(
            "(() => {{ const el = document.querySelector({sel}); if (!el) return null; \
             const r = el.getBoundingClientRect(); \
             return [r.x, r.y, r.width, r.height]; }})()"
        );
        let v = self.cdp.evaluate(&js)?;
        let a = v.as_array().ok_or_else(|| format!("selector vanished: {selector}"))?;
        Ok((
            a[0].as_f64().unwrap_or(0.0),
            a[1].as_f64().unwrap_or(0.0),
            a[2].as_f64().unwrap_or(0.0),
            a[3].as_f64().unwrap_or(0.0),
        ))
    }

    fn cursor_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        let js = format!("window.__lensa && __lensa.move({x:.1},{y:.1})");
        let _ = self.cdp.evaluate(&js);
        self.cdp.sleep_pump(500) // matches the CSS transition
    }

    fn mouse_click(&mut self, x: f64, y: f64) -> Result<(), String> {
        let js = format!("window.__lensa && __lensa.ripple({x:.1},{y:.1})");
        let _ = self.cdp.evaluate(&js);
        for (t, clicks) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 1)] {
            self.cdp.send(
                "Input.dispatchMouseEvent",
                json!({"type": t, "x": x, "y": y, "button": "left", "clickCount": clicks}),
            )?;
        }
        Ok(())
    }

    fn click_target(&mut self, op: &Value) -> Result<((f64, f64), Option<(f64, f64, f64, f64)>), String> {
        if let Some(sel) = op["selector"].as_str() {
            let b = self.resolve_box(sel)?;
            Ok(((b.0 + b.2 / 2.0, b.1 + b.3 / 2.0), Some(b)))
        } else {
            let x = op["x"].as_f64().ok_or("click needs selector or x/y")?;
            let y = op["y"].as_f64().ok_or("click needs selector or x/y")?;
            Ok(((x, y), Some((x - 10.0, y - 10.0, 20.0, 20.0))))
        }
    }

    /// Execute one op; returns the mark/result JSON to report to the agent.
    pub fn exec(&mut self, op: &Value) -> Result<Value, String> {
        let kind = op["op"].as_str().ok_or("missing \"op\" field")?.to_string();
        match kind.as_str() {
            "navigate" => {
                let url_in = op["url"].as_str().ok_or("navigate needs url")?;
                // Bare paths become file:// URLs.
                let url = if url_in.contains("://") || url_in.starts_with("about:") {
                    url_in.to_string()
                } else {
                    let p = std::fs::canonicalize(url_in)
                        .map_err(|e| format!("cannot resolve path {url_in}: {e}"))?;
                    format!("file://{}", p.display())
                };
                self.cdp.clear_events();
                self.cdp.send("Page.navigate", json!({"url": url}))?;
                self.cdp
                    .wait_event("Page.loadEventFired", Duration::from_secs(25))?;
                self.cdp.sleep_pump(350)?;
                Ok(self.mark("navigate", &url, None))
            }
            "click" => {
                let ((x, y), bbox) = self.click_target(op)?;
                self.cursor_to(x, y)?;
                let m = self.mark("click", op["selector"].as_str().unwrap_or("point"), bbox);
                self.mouse_click(x, y)?;
                self.cdp.sleep_pump(250)?;
                Ok(m)
            }
            "type" => {
                let text = op["text"].as_str().ok_or("type needs text")?.to_string();
                let per_char = op["typewriter_ms"].as_u64().unwrap_or(45);
                let bbox = if let Some(sel) = op["selector"].as_str() {
                    let b = self.resolve_box(sel)?;
                    let (x, y) = (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0);
                    self.cursor_to(x, y)?;
                    self.mouse_click(x, y)?;
                    self.cdp.sleep_pump(150)?;
                    Some(b)
                } else {
                    None
                };
                let m = self.mark("type", op["selector"].as_str().unwrap_or(""), bbox);
                for ch in text.chars() {
                    if ch == '\n' {
                        for t in ["rawKeyDown", "char", "keyUp"] {
                            self.cdp.send(
                                "Input.dispatchKeyEvent",
                                json!({"type": t, "key": "Enter", "code": "Enter",
                                       "text": "\r", "windowsVirtualKeyCode": 13}),
                            )?;
                        }
                    } else {
                        self.cdp
                            .send("Input.insertText", json!({"text": ch.to_string()}))?;
                    }
                    self.cdp.sleep_pump(per_char)?;
                }
                Ok(m)
            }
            "scroll" => {
                let y = op["y"].as_f64().unwrap_or(0.0);
                let smooth = op["smooth"].as_bool().unwrap_or(true);
                let behavior = if smooth { "smooth" } else { "instant" };
                let js = format!("window.scrollTo({{top:{y},behavior:'{behavior}'}})");
                self.cdp.evaluate(&js)?;
                self.cdp.sleep_pump(if smooth { 800 } else { 120 })?;
                Ok(self.mark("scroll", &format!("y={y}"), None))
            }
            "wait" => {
                if let Some(ms) = op["ms"].as_u64() {
                    self.cdp.sleep_pump(ms)?;
                    Ok(self.mark("wait", &format!("{ms}ms"), None))
                } else if let Some(sel) = op["selector"].as_str() {
                    let sel_js = js_string(sel);
                    let deadline = std::time::Instant::now() + Duration::from_secs(20);
                    loop {
                        let v = self
                            .cdp
                            .evaluate(&format!("!!document.querySelector({sel_js})"))?;
                        if v.as_bool() == Some(true) {
                            break;
                        }
                        if std::time::Instant::now() > deadline {
                            return Err(format!("wait: selector never appeared: {sel}"));
                        }
                        self.cdp.sleep_pump(100)?;
                    }
                    Ok(self.mark("wait", sel, None))
                } else {
                    Err("wait needs ms or selector".into())
                }
            }
            "mark" => {
                let label = op["label"].as_str().unwrap_or("").to_string();
                Ok(self.mark("mark", &label, None))
            }
            "start_recording" => {
                if let Some(p) = op["path"].as_str() {
                    self.out_path = Some(p.to_string());
                }
                let max_w = (self.css_w as f64 * self.scale) as u32;
                let max_h = (self.css_h as f64 * self.scale) as u32;
                self.cdp.start_capture(max_w, max_h, self.scale)?;
                // Nudge a paint so the first frame arrives promptly.
                let _ = self.cdp.evaluate("void 0");
                self.cdp.sleep_pump(200)?;
                Ok(json!({"event": "recording_started"}))
            }
            "stop_recording" => {
                self.cdp.stop_capture()?;
                let out = self
                    .out_path
                    .clone()
                    .unwrap_or_else(|| "lensa-out.mp4".to_string());
                eprintln!(
                    "lensa: captured {} frames ({:.1} MB spooled), rendering {out} ...",
                    self.cdp.frames.len(),
                    self.cdp.frames.bytes() as f64 / 1_048_576.0
                );
                let (dur, n_ev) = crate::zoom::render(
                    &self.cdp.frames,
                    &self.marks,
                    &out,
                    self.css_w,
                    self.css_h,
                    self.out_size,
                    self.keep_temp,
                )?;
                self.rendered = Some((dur, n_ev));
                Ok(json!({
                    "event": "recording_rendered", "path": out,
                    "duration": dur, "zoom_events": n_ev,
                    "frames": self.cdp.frames.len(),
                    "spooled_bytes": self.cdp.frames.bytes(),
                }))
            }
            other => Err(format!("unknown op: {other}")),
        }
    }
}
