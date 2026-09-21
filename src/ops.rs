//! Agent-facing control protocol: newline-delimited JSON ops.
//!
//! Every executed op auto-emits a timestamped mark (on the recording clock)
//! carrying the target element's bounding box in CSS pixels; the zoom
//! generator consumes these.

use crate::backdrop::Choice;
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

/// Where an interaction lands, in CSS pixels: the point the mouse event goes
/// to, the target's box if it has one (the zoom generator's input), and the
/// pointer shape the OS would show over it.
struct Aim {
    point: (f64, f64),
    bbox: Option<(f64, f64, f64, f64)>,
    cursor: String,
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
    pub cursor: CursorCfg,
    /// Backdrop for the finished video; resolved at render time.
    pub background: Choice,
    pub rendered: Option<(f64, usize)>,
}

/// The pointer shapes lensa can draw, in the macOS idiom.
pub const CURSOR_SHAPES: &[(&str, &str)] = &[
    ("arrow", "the classic pointer; the fallback for anything else"),
    ("hand", "pointing hand, for links and buttons"),
    ("text", "I-beam, for text fields and editable content"),
];

/// A 1x pointer is a 24 CSS px arrow, which is a speck once a 1470px take is
/// scaled into a phone-sized player. Just under 2x reads clearly without
/// covering the thing it is pointing at.
pub const DEFAULT_CURSOR_SCALE: f64 = 1.75;

/// Which pointer to draw, and how big.
#[derive(Clone, Copy)]
pub struct CursorCfg {
    /// `None` follows whatever is under the pointer; `Some` pins one shape.
    pub shape: Option<&'static str>,
    /// Multiplier over a 1x system pointer.
    pub scale: f64,
    pub enabled: bool,
}

/// Where the text is being entered, in CSS pixels, as [x, y, height].
///
/// lensa records agents, not people. There is no hand on a mouse to follow, and the pointer it
/// draws is a prop: it is parked wherever the field was clicked and stays there while a whole
/// sentence is typed. The thing that actually moves, and the thing a viewer is reading, is the
/// caret. So that is what the camera follows.
///
/// Works for contenteditable through the selection, and for input and textarea by mirroring the
/// text up to the caret into a hidden element with the same typography and measuring where it
/// ends. Returns null when there is nothing focused to measure.
const CARET_JS: &str = r#"(() => {
  const el = document.activeElement;
  if (!el) return null;
  if (el.isContentEditable) {
    const s = getSelection();
    if (s && s.rangeCount) {
      const r = s.getRangeAt(0).getBoundingClientRect();
      if (r.height) return [r.left, r.top, r.height];
    }
    const r = el.getBoundingClientRect();
    return [r.left, r.top, r.height];
  }
  if (el.value === undefined) return null;
  const r = el.getBoundingClientRect(), cs = getComputedStyle(el);
  const d = document.createElement('div');
  for (const k of ['fontFamily','fontSize','fontWeight','fontStyle','letterSpacing',
                   'textTransform','paddingLeft','paddingTop','borderLeftWidth','borderTopWidth',
                   'width','lineHeight','textIndent'])
    d.style[k] = cs[k];
  d.style.position = 'absolute';
  d.style.left = '-9999px';
  d.style.top = '0';
  d.style.visibility = 'hidden';
  d.style.whiteSpace = el.tagName === 'TEXTAREA' ? 'pre-wrap' : 'pre';
  d.style.wordWrap = 'break-word';
  const upto = el.selectionEnd == null ? el.value.length : el.selectionEnd;
  d.textContent = el.value.slice(0, upto);
  const tip = document.createElement('span');
  tip.textContent = '\u200b';
  d.appendChild(tip);
  document.body.appendChild(d);
  const ox = tip.offsetLeft, oy = tip.offsetTop;
  d.remove();
  const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2 || 16;
  return [r.left + ox - (el.scrollLeft || 0), r.top + oy - (el.scrollTop || 0), lh];
})()"#;

/// Spacing of the synthetic pan samples laid down across a typing op, in seconds. Close enough
/// that the movement reads as continuous, far enough apart not to bloat the filter expression.
const CARET_SAMPLE_S: f64 = 0.12;

impl Default for CursorCfg {
    fn default() -> CursorCfg {
        CursorCfg { shape: None, scale: DEFAULT_CURSOR_SCALE, enabled: true }
    }
}

/// The `--cursor` values, for help and error text.
pub fn cursor_help() -> String {
    let mut s = CURSOR_SHAPES
        .iter()
        .map(|(n, d)| format!("  {n:<10} {d}"))
        .collect::<Vec<_>>()
        .join("\n");
    s.push_str("\n  auto       follow the element under the pointer (default)");
    s.push_str("\n  none       draw no pointer at all");
    s
}

impl CursorCfg {
    pub fn parse(name: &str, scale: f64) -> Result<CursorCfg, String> {
        if !(scale.is_finite() && (0.2..=8.0).contains(&scale)) {
            return Err("--cursor-scale must be between 0.2 and 8".into());
        }
        match name {
            "none" | "off" => Ok(CursorCfg { shape: None, scale, enabled: false }),
            "auto" => Ok(CursorCfg { shape: None, scale, enabled: true }),
            n => match CURSOR_SHAPES.iter().find(|(s, _)| *s == n) {
                Some((s, _)) => Ok(CursorCfg { shape: Some(s), scale, enabled: true }),
                None => Err(format!("unknown cursor: {n}\n\nCursors:\n{}", cursor_help())),
            },
        }
    }
}

/// Classify an element the way the OS would: what shape should be over it.
/// Shared by the selector and point paths, and deliberately independent of the
/// injected overlay so it still answers when the cursor is turned off.
const KIND_FN: &str = "((el) => { const cs = getComputedStyle(el).cursor, \
    tag = el.tagName.toLowerCase(); \
  if (cs === 'pointer' || cs === 'grab' || cs === 'grabbing') return 'hand'; \
  if (cs === 'text' || cs === 'vertical-text') return 'text'; \
  if (tag === 'textarea' || el.isContentEditable) return 'text'; \
  if (tag === 'input') return /^(button|submit|reset|checkbox|radio|range|color|file|image)$/ \
    .test(el.type || 'text') ? 'hand' : 'text'; \
  if (tag === 'a' || tag === 'button' || tag === 'select' || \
      el.getAttribute('role') === 'button') return 'hand'; \
  return 'arrow'; })";

/// Injected into every document: a large macOS-style cursor + click ripple, so
/// the video shows pointer intent (headless capture has no OS cursor).
///
/// Drawn as vector SVG at the requested size rather than an upscaled bitmap, so
/// it stays crisp when a zoom crops into it. It lives in the page, which means
/// it is captured as part of the frame and the zoom transform carries it along
/// for free: no second compositing path, no chance of the pointer and the
/// content disagreeing about where the click happened.
///
/// Each shape is painted twice over the same geometry, a thick white pass and
/// then the black body, so a shape built from several overlapping pieces (the
/// hand) gets one clean outline around the union rather than seams between the
/// pieces.
const CURSOR_JS: &str = r##"
(() => {
  const S = __LENSA_SCALE__;
  const PIN = __LENSA_SHAPE__;
  const SHAPES = {
    arrow: {
      w: 16, h: 24, vb: '-2 -2 16 24', ox: 2, oy: 2, fill: 1, hw: 3, bw: 0,
      parts: '<path d="M0 0L0 16.8L4.2 12.9L6.3 19.4L9.1 19.7L7 13.25L9.8 13.6Z"/>'
    },
    hand: {
      /* An index finger, three shorter knuckles, a thumb and a fist, drawn as
         overlapping rounded rects: the two-pass paint turns their union into one
         outlined silhouette, so the pieces never show as seams. */
      w: 22, h: 27, vb: '-2 -1 22 27', ox: 8, oy: 2, fill: 1, hw: 3, bw: 0,
      parts:
        '<rect x="4" y="1" width="4" height="13" rx="2"/>' +
        '<rect x="7.6" y="8.5" width="3.8" height="6" rx="1.9"/>' +
        '<rect x="11" y="9.6" width="3.7" height="5.4" rx="1.85"/>' +
        '<rect x="14.3" y="10.9" width="3.4" height="4.9" rx="1.7"/>' +
        '<rect x="0.3" y="13.5" width="4" height="7" rx="2"/>' +
        '<rect x="2.6" y="12.8" width="15.1" height="10.2" rx="4.5"/>'
    },
    text: {
      w: 12, h: 24, vb: '-6 -12 12 24', ox: 6, oy: 12, fill: 0, hw: 4.2, bw: 1.7,
      parts: '<path d="M-3.2 -8.8H3.2M0 -8.8V8.8M-3.2 8.8H3.2"/>'
    }
  };
  const svg = (s) =>
    '<svg xmlns="http://www.w3.org/2000/svg" width="' + (s.w * S) + '" height="' +
    (s.h * S) + '" viewBox="' + s.vb +
    '" style="position:absolute;left:0;top:0;display:block;overflow:visible">' +
    '<g fill="' + (s.fill ? '#fff' : 'none') + '" stroke="#fff" stroke-width="' + s.hw +
    '" stroke-linejoin="round" stroke-linecap="round">' + s.parts + '</g>' +
    '<g fill="' + (s.fill ? '#0b0b0c' : 'none') + '" stroke="' +
    (s.bw ? '#0b0b0c' : 'none') + '" stroke-width="' + s.bw +
    '" stroke-linejoin="round" stroke-linecap="round">' + s.parts + '</g></svg>';
  const state = { k: null, x: -400, y: -400 };
  const ensure = () => {
    let c = document.getElementById('__lensa_cursor');
    if (c) return c;
    c = document.createElement('div');
    c.id = '__lensa_cursor';
    /* Zero-sized box: the SVG hangs off its top-left, so one translate puts the
       hot spot of any shape exactly on the point being clicked. */
    c.style.cssText = 'position:fixed;left:0;top:0;width:0;height:0;' +
      'z-index:2147483647;pointer-events:none;' +
      'transition:transform .45s cubic-bezier(.22,.61,.36,1);' +
      'transform:translate(-400px,-400px);will-change:transform;' +
      'filter:drop-shadow(0 ' + (1.2 * S) + 'px ' + (1.8 * S) + 'px rgba(0,0,0,.38))';
    (document.body || document.documentElement).appendChild(c);
    return c;
  };
  const apply = (k) => {
    const c = ensure();
    k = PIN || k || state.k || 'arrow';
    if (!SHAPES[k]) k = 'arrow';
    if (k !== state.k) { state.k = k; c.innerHTML = svg(SHAPES[k]); }
    const s = SHAPES[k];
    c.style.transform =
      'translate(' + (state.x - s.ox * S) + 'px,' + (state.y - s.oy * S) + 'px)';
  };
  window.__lensa = {
    move(x, y, k) { state.x = x; state.y = y; apply(k); },
    shape(k) { apply(k); },
    ripple(x, y) {
      if (!document.getElementById('__lensa_style')) {
        const s = document.createElement('style'); s.id = '__lensa_style';
        s.textContent = '@keyframes __lensa_r{from{transform:scale(.4);opacity:1}' +
          'to{transform:scale(1.7);opacity:0}}';
        (document.head || document.documentElement).appendChild(s);
      }
      const R = 18 * Math.max(1, S * 0.85);
      const r = document.createElement('div');
      r.style.cssText = 'position:fixed;left:' + (x - R) + 'px;top:' + (y - R) + 'px;' +
        'width:' + (2 * R) + 'px;height:' + (2 * R) + 'px;border-radius:50%;' +
        'border:' + Math.max(3, 1.7 * S) + 'px solid rgba(59,130,246,.85);' +
        'z-index:2147483646;pointer-events:none;' +
        'animation:__lensa_r .5s ease-out forwards';
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
        cursor: CursorCfg,
    ) -> Result<Session, String> {
        let mut cdp = Cdp::launch(chromium, css_w, css_h, scale)?;
        if cursor.enabled {
            let source = CURSOR_JS
                .replace("__LENSA_SCALE__", &format!("{:.4}", cursor.scale))
                .replace("__LENSA_SHAPE__", &js_string(cursor.shape.unwrap_or("")));
            cdp.send(
                "Page.addScriptToEvaluateOnNewDocument",
                json!({ "source": source }),
            )?;
        }
        Ok(Session {
            cdp,
            marks: Vec::new(),
            css_w,
            css_h,
            scale,
            out_size,
            out_path: None,
            keep_temp,
            cursor,
            background: Choice::Auto,
            rendered: None,
        })
    }

    /// Where the caret sits horizontally, in CSS pixels, or None if nothing is focused.
    fn caret_x(&mut self) -> Option<f64> {
        self.cdp
            .evaluate(CARET_JS)
            .ok()
            .and_then(|v| v.as_array().and_then(|a| a.first().and_then(|x| x.as_f64())))
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

    /// Scroll element into view, settle, and describe where the interaction
    /// lands: the point, the target's box (CSS px), and the pointer shape that
    /// belongs over it.
    fn resolve_box(&mut self, selector: &str) -> Result<Aim, String> {
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
             return [r.x, r.y, r.width, r.height, {KIND_FN}(el)]; }})()"
        );
        let v = self.cdp.evaluate(&js)?;
        let a = v.as_array().ok_or_else(|| format!("selector vanished: {selector}"))?;
        let b = (
            a[0].as_f64().unwrap_or(0.0),
            a[1].as_f64().unwrap_or(0.0),
            a[2].as_f64().unwrap_or(0.0),
            a[3].as_f64().unwrap_or(0.0),
        );
        Ok(Aim {
            point: (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0),
            bbox: Some(b),
            cursor: a
                .get(4)
                .and_then(|k| k.as_str())
                .unwrap_or("arrow")
                .to_string(),
        })
    }

    /// The pointer shape for a bare x/y click: whatever is under that point.
    fn kind_at(&mut self, x: f64, y: f64) -> String {
        let js = format!(
            "(() => {{ const el = document.elementFromPoint({x:.1},{y:.1}); \
             return el ? {KIND_FN}(el) : 'arrow'; }})()"
        );
        self.cdp
            .evaluate(&js)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "arrow".into())
    }

    /// Glide the pointer to a point, in the shape that belongs there.
    ///
    /// The settle always happens, cursor or no cursor: the pause before a click
    /// is part of the pacing the zoom is cut against, not just cursor travel.
    fn cursor_to(&mut self, x: f64, y: f64, kind: &str) -> Result<(), String> {
        if self.cursor.enabled {
            let k = js_string(kind);
            let js = format!("window.__lensa && __lensa.move({x:.1},{y:.1},{k})");
            let _ = self.cdp.evaluate(&js);
        }
        self.cdp.sleep_pump(500) // matches the CSS transition
    }

    fn mouse_click(&mut self, x: f64, y: f64) -> Result<(), String> {
        if self.cursor.enabled {
            let js = format!("window.__lensa && __lensa.ripple({x:.1},{y:.1})");
            let _ = self.cdp.evaluate(&js);
        }
        for (t, clicks) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 1)] {
            self.cdp.send(
                "Input.dispatchMouseEvent",
                json!({"type": t, "x": x, "y": y, "button": "left", "clickCount": clicks}),
            )?;
        }
        Ok(())
    }

    fn click_target(&mut self, op: &Value) -> Result<Aim, String> {
        if let Some(sel) = op["selector"].as_str() {
            self.resolve_box(sel)
        } else {
            let x = op["x"].as_f64().ok_or("click needs selector or x/y")?;
            let y = op["y"].as_f64().ok_or("click needs selector or x/y")?;
            Ok(Aim {
                point: (x, y),
                bbox: Some((x - 10.0, y - 10.0, 20.0, 20.0)),
                cursor: self.kind_at(x, y),
            })
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
                let aim = self.click_target(op)?;
                let (x, y) = aim.point;
                self.cursor_to(x, y, &aim.cursor)?;
                let m = self.mark("click", op["selector"].as_str().unwrap_or("point"), aim.bbox);
                self.mouse_click(x, y)?;
                self.cdp.sleep_pump(250)?;
                Ok(m)
            }
            "type" => {
                let text = op["text"].as_str().ok_or("type needs text")?.to_string();
                // Fast by default. 45ms per character is a person hunting for keys; an agent
                // does not hunt, and a viewer does not want to watch one. A take can still ask
                // for slower with typewriter_ms when the point is to read along.
                let per_char = op["typewriter_ms"].as_u64().unwrap_or(18);
                let bbox = if let Some(sel) = op["selector"].as_str() {
                    let aim = self.resolve_box(sel)?;
                    let (x, y) = aim.point;
                    self.cursor_to(x, y, &aim.cursor)?;
                    self.mouse_click(x, y)?;
                    self.cdp.sleep_pump(150)?;
                    aim.bbox
                } else {
                    None
                };
                let sel = op["selector"].as_str().unwrap_or("").to_string();
                let m = self.mark("type", &sel, bbox);
                let field_h = bbox.map(|(_, _, _, h)| h).unwrap_or(24.0);
                let field_y = bbox.map(|(_, y, _, _)| y).unwrap_or(0.0);
                // Measure the caret either side of the typing, never during it. Asking the page
                // where the caret is costs a round trip, and doing that between keystrokes put
                // the round trip INSIDE the typing rhythm: the words came out slower than the
                // requested speed, and the camera moved in steps because the samples were as
                // uneven as the latency. Two measurements and an interpolation give a smooth
                // pan and typing that runs at the speed that was asked for.
                let t_start = self.cdp.now_rec();
                let caret_start = self.caret_x();
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
                // Lay the pan down after the fact, evenly spaced across the time the typing
                // actually took. A caret is a line, not a box: it gets a sliver of width so the
                // framing has something to sit against, and the field's height so following it
                // does not tighten the zoom to one line of text.
                if self.cdp.is_recording() {
                    let t_end = self.cdp.now_rec();
                    if let (Some(x0), Some(x1)) = (caret_start, self.caret_x()) {
                        let span = t_end - t_start;
                        if span > 0.2 && (x1 - x0).abs() > 1.0 {
                            let steps = ((span / CARET_SAMPLE_S).round() as usize).clamp(2, 40);
                            for k in 1..=steps {
                                let f = k as f64 / steps as f64;
                                self.marks.push(Mark {
                                    t: t_start + span * f,
                                    kind: "type".into(),
                                    label: sel.clone(),
                                    bbox: Some((x0 + (x1 - x0) * f, field_y, 2.0, field_h)),
                                });
                            }
                        }
                    }
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
                    // 20 seconds is fine for a page to render something and far too short for
                    // anything that has to think first. A take that waits on a model finishing
                    // its answer should say how long it is prepared to wait, and get told how
                    // long it actually waited when it gives up.
                    let budget = op["timeout_ms"].as_u64().unwrap_or(20_000);
                    let started = std::time::Instant::now();
                    let deadline = started + Duration::from_millis(budget);
                    loop {
                        let v = self
                            .cdp
                            .evaluate(&format!("!!document.querySelector({sel_js})"))?;
                        if v.as_bool() == Some(true) {
                            break;
                        }
                        if std::time::Instant::now() > deadline {
                            return Err(format!(
                                "wait: selector never appeared after {:.1}s: {sel}\n  \
                                 raise it with {{\"op\":\"wait\",\"selector\":\"...\",\"timeout_ms\":60000}}",
                                started.elapsed().as_secs_f64()
                            ));
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
                    self.background,
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
