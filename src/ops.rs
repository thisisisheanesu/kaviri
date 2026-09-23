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

/// The pointer shapes kaviri can draw, in the macOS idiom.
pub const CURSOR_SHAPES: &[(&str, &str)] = &[
    (
        "arrow",
        "the classic pointer; the fallback for anything else",
    ),
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
/// kaviri records agents, not people. There is no hand on a mouse to follow, and the pointer it
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
        CursorCfg {
            shape: None,
            scale: DEFAULT_CURSOR_SCALE,
            enabled: true,
        }
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
            "none" | "off" => Ok(CursorCfg {
                shape: None,
                scale,
                enabled: false,
            }),
            "auto" => Ok(CursorCfg {
                shape: None,
                scale,
                enabled: true,
            }),
            n => match CURSOR_SHAPES.iter().find(|(s, _)| *s == n) {
                Some((s, _)) => Ok(CursorCfg {
                    shape: Some(s),
                    scale,
                    enabled: true,
                }),
                None => Err(format!(
                    "unknown cursor: {n}\n\nCursors:\n{}",
                    cursor_help()
                )),
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
  const S = __KAVIRI_SCALE__;
  const PIN = __KAVIRI_SHAPE__;
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
    let c = document.getElementById('__kaviri_cursor');
    if (c) return c;
    c = document.createElement('div');
    c.id = '__kaviri_cursor';
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
  window.__kaviri = {
    move(x, y, k) { state.x = x; state.y = y; apply(k); },
    shape(k) { apply(k); },
    ripple(x, y) {
      if (!document.getElementById('__kaviri_style')) {
        const s = document.createElement('style'); s.id = '__kaviri_style';
        s.textContent = '@keyframes __kaviri_r{from{transform:scale(.4);opacity:1}' +
          'to{transform:scale(1.7);opacity:0}}';
        (document.head || document.documentElement).appendChild(s);
      }
      const R = 18 * Math.max(1, S * 0.85);
      const r = document.createElement('div');
      r.style.cssText = 'position:fixed;left:' + (x - R) + 'px;top:' + (y - R) + 'px;' +
        'width:' + (2 * R) + 'px;height:' + (2 * R) + 'px;border-radius:50%;' +
        'border:' + Math.max(3, 1.7 * S) + 'px solid rgba(59,130,246,.85);' +
        'z-index:2147483646;pointer-events:none;' +
        'animation:__kaviri_r .5s ease-out forwards';
      (document.body || document.documentElement).appendChild(r);
      setTimeout(() => r.remove(), 600);
    }
  };
})();
"##;

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

/// Read a millisecond duration field off an op.
///
/// `as_u64` on its own answers `None` for every JSON number that is not a
/// non-negative integer, and the op then silently ran with its default: a
/// generator that emits `"timeout_ms": 180000.0` got twenty seconds, and
/// `"typewriter_ms": 62.5` typed at eighteen. Any finite, non-negative number is
/// honoured; anything else is a named error rather than a quiet fallback.
fn duration_ms(op: &Value, field: &str, default: u64) -> Result<u64, String> {
    match op.get(field) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => match v.as_f64() {
            Some(n) if n.is_finite() && n >= 0.0 => Ok(n.round() as u64),
            _ => Err(format!(
                "{field} must be a non-negative number of milliseconds, got {v}"
            )),
        },
    }
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
        // `--keep-temp` is about every intermediate the run produces, and the frame
        // spool is by far the largest of them.
        cdp.set_keep_spool(keep_temp);
        if cursor.enabled {
            let source = CURSOR_JS
                .replace("__KAVIRI_SCALE__", &format!("{:.4}", cursor.scale))
                .replace("__KAVIRI_SHAPE__", &js_string(cursor.shape.unwrap_or("")));
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
        self.cdp.evaluate(CARET_JS).ok().and_then(|v| {
            v.as_array()
                .and_then(|a| a.first().and_then(|x| x.as_f64()))
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
                                   // Ask for the visibility facts in the same round trip as the geometry. A
                                   // node that matched the selector is not necessarily a thing that can be
                                   // clicked: a zero-size or hidden element has a box of all zeros, and
                                   // dispatching to its centre is a real click on whatever sits in the
                                   // viewport's top-left corner, reported as a success.
        let js = format!(
            "(() => {{ const el = document.querySelector({sel}); if (!el) return null; \
             const r = el.getBoundingClientRect(), cs = getComputedStyle(el); \
             const cx = r.x + r.width / 2, cy = r.y + r.height / 2; \
             let top = null; \
             if (r.width > 0 && r.height > 0 && cx >= 0 && cy >= 0 && \
                 cx < innerWidth && cy < innerHeight) {{ \
               const t = document.elementFromPoint(cx, cy); \
               if (t && t !== el && !el.contains(t) && !t.contains(el)) \
                 top = t.tagName.toLowerCase() + (t.id ? '#' + t.id : ''); \
             }} \
             return [r.x, r.y, r.width, r.height, {KIND_FN}(el), \
                     r.width * r.height > 0, \
                     cs.visibility !== 'hidden' && cs.display !== 'none', top]; }})()"
        );
        let v = self.cdp.evaluate(&js)?;
        let a = v
            .as_array()
            .ok_or_else(|| format!("selector vanished: {selector}"))?;
        let num = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0);
        let b = (num(0), num(1), num(2), num(3));
        if a.get(6).and_then(|x| x.as_bool()) != Some(true) {
            return Err(format!(
                "selector matched a non-visible element (display:none or visibility:hidden): {selector}"
            ));
        }
        if a.get(5).and_then(|x| x.as_bool()) != Some(true) {
            return Err(format!(
                "selector matched a zero-size element, so there is no point to aim at: {selector}"
            ));
        }
        if let Some(top) = a.get(7).and_then(|x| x.as_str()) {
            return Err(format!(
                "selector {selector} is covered by <{top}> at its centre point; \
                 close the overlay, or target the element that is actually on top"
            ));
        }
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
            let js = format!("window.__kaviri && __kaviri.move({x:.1},{y:.1},{k})");
            let _ = self.cdp.evaluate(&js);
        }
        self.cdp.sleep_pump(500) // matches the CSS transition
    }

    fn mouse_click(&mut self, x: f64, y: f64) -> Result<(), String> {
        if self.cursor.enabled {
            let js = format!("window.__kaviri && __kaviri.ripple({x:.1},{y:.1})");
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
                let budget = duration_ms(op, "timeout_ms", 25_000)?;
                self.cdp.clear_events();
                let r = self.cdp.send("Page.navigate", json!({"url": url}))?;
                if let Some(err) = r["errorText"].as_str() {
                    // ERR_ABORTED is what Chromium reports when the navigation turned
                    // into a download or was superseded by another one. The page is
                    // fine and the take carries on; everything else is a Chrome error
                    // page, which is not what the script asked to film.
                    if err != "net::ERR_ABORTED" {
                        return Err(format!("navigate failed: {err}: {url}"));
                    }
                }
                let loaded = self
                    .cdp
                    .wait_event("Page.loadEventFired", Duration::from_millis(budget))?;
                if !loaded {
                    // A same-document navigation (a hash change, a pushState route)
                    // never fires load because the document never changed. That is a
                    // success; a document still parsing after the whole budget is not.
                    let state = self.cdp.evaluate("document.readyState")?;
                    if state.as_str() != Some("complete") {
                        return Err(format!(
                            "navigate: no load event after {:.1}s and the document is still \
                             \"{}\": {url}\n  \
                             raise it with {{\"op\":\"navigate\",\"url\":\"...\",\"timeout_ms\":60000}}",
                            budget as f64 / 1000.0,
                            state.as_str().unwrap_or("unknown")
                        ));
                    }
                }
                self.cdp.sleep_pump(350)?;
                Ok(self.mark("navigate", &url, None))
            }
            "click" => {
                let aim = self.click_target(op)?;
                let (x, y) = aim.point;
                self.cursor_to(x, y, &aim.cursor)?;
                let m = self.mark(
                    "click",
                    op["selector"].as_str().unwrap_or("point"),
                    aim.bbox,
                );
                self.mouse_click(x, y)?;
                self.cdp.sleep_pump(250)?;
                Ok(m)
            }
            "type" => {
                let text = op["text"].as_str().ok_or("type needs text")?.to_string();
                // Fast by default. 45ms per character is a person hunting for keys; an agent
                // does not hunt, and a viewer does not want to watch one. A take can still ask
                // for slower with typewriter_ms when the point is to read along.
                let per_char = duration_ms(op, "typewriter_ms", 18)?;
                let bbox = if let Some(sel) = op["selector"].as_str() {
                    let aim = self.resolve_box(sel)?;
                    let (x, y) = aim.point;
                    self.cursor_to(x, y, &aim.cursor)?;
                    self.mouse_click(x, y)?;
                    self.cdp.sleep_pump(150)?;
                    aim.bbox
                } else {
                    // Input.insertText delivers to whatever holds focus, and on a
                    // freshly navigated page that is <body>: the characters go
                    // nowhere, no box is recorded so the op earns no zoom, and the
                    // result JSON is indistinguishable from a take that worked.
                    let editable = self.cdp.evaluate(
                        "(() => { const el = document.activeElement; \
                          if (!el || el === document.body || el === document.documentElement) \
                            return false; \
                          if (el.isContentEditable) return true; \
                          const tag = el.tagName.toLowerCase(); \
                          if (tag === 'textarea') return true; \
                          if (tag === 'input') \
                            return !/^(button|submit|reset|checkbox|radio|range|color|file|image)$/ \
                              .test(el.type || 'text'); \
                          return false; })()",
                    )?;
                    if editable.as_bool() != Some(true) {
                        return Err("type without a selector needs a focused editable element; \
                             click one first, or pass a selector"
                            .into());
                    }
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
                // No silent default here. `{"op":"scroll","top":600}` and a y that
                // arrived as the string "600" both used to scroll the page to the top
                // and report success, which looks exactly like a scroll that worked.
                let y = op["y"].as_f64().ok_or(
                    "scroll needs y: a number of CSS pixels, absolute from the top of the document",
                )?;
                if !y.is_finite() || y < 0.0 {
                    return Err(format!(
                        "scroll y must be a finite, non-negative number of CSS pixels, got {y}"
                    ));
                }
                let smooth = op["smooth"].as_bool().unwrap_or(true);
                let behavior = if smooth { "smooth" } else { "instant" };
                let js = format!("window.scrollTo({{top:{y},behavior:'{behavior}'}})");
                self.cdp.evaluate(&js)?;
                self.cdp.sleep_pump(if smooth { 800 } else { 120 })?;
                Ok(self.mark("scroll", &format!("y={y}"), None))
            }
            "wait" => {
                let has_ms = !op["ms"].is_null();
                let has_sel = !op["selector"].is_null();
                // Both fields together reads as "wait for this, but at most that
                // long", which is what timeout_ms is for. The old chain took the ms
                // branch and never looked at the selector, so the script raced ahead
                // of the page it meant to wait for.
                if has_ms && has_sel {
                    return Err("wait takes ms or selector, not both; \
                                use timeout_ms to bound a selector wait"
                        .into());
                }
                if has_ms {
                    let ms = duration_ms(op, "ms", 0)?;
                    self.cdp.sleep_pump(ms)?;
                    Ok(self.mark("wait", &format!("{ms}ms"), None))
                } else if has_sel {
                    let sel = op["selector"]
                        .as_str()
                        .ok_or("wait selector must be a CSS selector string")?
                        .to_string();
                    let sel_js = js_string(&sel);
                    // Existence is not what a script means by "wait for the success
                    // message": a `display:none` node is in the DOM from first paint,
                    // so the wait returned on its first poll. `visible: false` asks
                    // for the old presence-only test.
                    let want_visible = op["visible"].as_bool().unwrap_or(true);
                    let probe = if want_visible {
                        format!(
                            "(() => {{ const e = document.querySelector({sel_js}); \
                              return !!(e && e.getClientRects().length && \
                                        getComputedStyle(e).visibility !== 'hidden'); }})()"
                        )
                    } else {
                        format!("!!document.querySelector({sel_js})")
                    };
                    // 20 seconds is fine for a page to render something and far too short for
                    // anything that has to think first. A take that waits on a model finishing
                    // its answer should say how long it is prepared to wait, and get told how
                    // long it actually waited when it gives up.
                    let budget = duration_ms(op, "timeout_ms", 20_000)?;
                    let started = std::time::Instant::now();
                    let deadline = started + Duration::from_millis(budget);
                    loop {
                        let v = self.cdp.evaluate(&probe)?;
                        if v.as_bool() == Some(true) {
                            break;
                        }
                        if std::time::Instant::now() > deadline {
                            let what = if want_visible {
                                "never became visible"
                            } else {
                                "never appeared"
                            };
                            return Err(format!(
                                "wait: selector {what} after {:.1}s: {sel}\n  \
                                 raise it with {{\"op\":\"wait\",\"selector\":\"...\",\"timeout_ms\":60000}}, \
                                 or add \"visible\": false to wait for presence only",
                                started.elapsed().as_secs_f64()
                            ));
                        }
                        self.cdp.sleep_pump(100)?;
                    }
                    Ok(self.mark("wait", &sel, None))
                } else {
                    Err("wait needs ms or selector".into())
                }
            }
            "mark" => {
                let label = op["label"].as_str().unwrap_or("").to_string();
                Ok(self.mark("mark", &label, None))
            }
            "start_recording" => {
                // Starting over the top of a live take threw its frames away with no
                // word to the caller. The frames are the whole value of a recording,
                // so say so and let the script decide.
                if self.cdp.is_recording() {
                    return Err("already recording; send stop_recording to finish \
                                the take in flight before starting another"
                        .into());
                }
                if let Some(p) = op["path"].as_str() {
                    self.out_path = Some(p.to_string());
                }
                // A new take is a new clock. Marks left from the previous one are
                // timestamped against the old rec_t0, so carrying them over would put
                // zooms at times that belong to footage that no longer exists.
                self.marks.clear();
                self.rendered = None;
                let max_w = (self.css_w as f64 * self.scale) as u32;
                let max_h = (self.css_h as f64 * self.scale) as u32;
                self.cdp.start_capture(max_w, max_h, self.scale)?;
                // Nudge a paint so the first frame arrives promptly.
                let _ = self.cdp.evaluate("void 0");
                self.cdp.sleep_pump(200)?;
                Ok(json!({"event": "recording_started"}))
            }
            "stop_recording" => {
                // Teardown is not allowed to lose the take. Every frame is already on
                // disk by the time this runs, so a browser that died at second 55 of a
                // sixty second recording still gets rendered, and the CDP failure is
                // reported alongside the video rather than instead of it.
                let warning = self.cdp.stop_capture_best_effort();
                let out = self
                    .out_path
                    .clone()
                    .unwrap_or_else(|| "kaviri-out.mp4".to_string());
                let n_frames = self.cdp.frames.len();
                let spooled = self.cdp.frames.bytes();
                if n_frames == 0 {
                    self.cdp.frames_release();
                    return Err(match warning {
                        Some(w) => format!("stop_recording: no frames were captured ({w})"),
                        None => "stop_recording: no frames were captured; \
                                 was start_recording sent?"
                            .into(),
                    });
                }
                if let Some(w) = &warning {
                    eprintln!("kaviri: warning: capture ended early: {w}");
                }
                eprintln!(
                    "kaviri: captured {n_frames} frames ({:.1} MB spooled), rendering {out} ...",
                    spooled as f64 / 1_048_576.0
                );
                let rendered = crate::zoom::render(
                    &self.cdp.frames,
                    &self.marks,
                    &out,
                    self.css_w,
                    self.css_h,
                    self.out_size,
                    self.background,
                    self.keep_temp,
                );
                // The spool is the take's whole footprint on disk, and on a long
                // recording that is gigabytes. Release it whichever way the render
                // went: in serve mode the process outlives the take by hours.
                self.cdp.frames_release();
                let (dur, n_ev) = rendered?;
                self.rendered = Some((dur, n_ev));
                let mut result = json!({
                    "event": "recording_rendered", "path": out,
                    "duration": dur, "zoom_events": n_ev,
                    "frames": n_frames,
                    "spooled_bytes": spooled,
                });
                if let Some(w) = warning {
                    result["warning"] = json!(w);
                }
                Ok(result)
            }
            other => Err(format!("unknown op: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression this guards: `as_u64` answered `None` for any JSON number
    /// that was not a non-negative integer, so a duration written as a float was
    /// replaced by the default instead of being honoured or rejected.
    #[test]
    fn a_duration_written_as_a_float_is_honoured_not_defaulted() {
        let op = json!({"timeout_ms": 180000.0, "typewriter_ms": 62.5});
        assert_eq!(duration_ms(&op, "timeout_ms", 20_000), Ok(180_000));
        assert_eq!(duration_ms(&op, "typewriter_ms", 18), Ok(63));
    }

    #[test]
    fn a_missing_duration_falls_back_and_a_bad_one_is_an_error() {
        let op = json!({"ms": "800", "bad": -1, "worse": null});
        assert_eq!(duration_ms(&op, "absent", 42), Ok(42));
        assert_eq!(duration_ms(&op, "worse", 42), Ok(42));
        // A stringified number is what a shell or a template that stringifies
        // everything produces, and it used to be silently ignored.
        assert!(duration_ms(&op, "ms", 0).is_err());
        assert!(duration_ms(&op, "bad", 0).is_err());
    }
}
