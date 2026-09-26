//! Motion graphics: a JSONL timeline compiled to a video, frame by frame.
//!
//!   kaviri motion --script reel.jsonl --out reel.mp4
//!
//! `record` films a web app that already exists. `motion` films one kaviri
//! builds for the purpose: every line of the script is a scene, a layer, an
//! animation, a micro-interaction or a sound, and the page that plays them is a
//! pure function of time. kaviri seeks it to each frame, photographs it, and
//! muxes the pictures with a soundtrack synthesized on the same beat grid, so
//! a cut written at `"8bar"` lands on the downbeat of bar nine every time.
//!
//! This file owns the parts that are not pixels: reading the script, turning
//! beats and bars into seconds, laying scenes end to end, checking every name
//! against what the runtime (`motion.js`) implements, and inlining assets so the
//! page is one self-contained document. `synth.rs` makes the sound and
//! `render_frames` below drives the browser.

use crate::cdp::{self, Cdp};
use crate::synth;
use base64::Engine;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The in-page engine. Everything visual lives here.
const RUNTIME_JS: &str = include_str!("motion.js");
const RUNTIME_CSS: &str = include_str!("motion.css");

/// Every op a motion script may contain.
pub const OPS: &[&str] = &[
    "video",
    "theme",
    "font",
    "music",
    "sfx",
    "background",
    "scene",
    "camera",
    "shake",
    "flash",
    "text",
    "image",
    "svg",
    "icon",
    "shape",
    "html",
    "ui",
    "group",
    "particles",
    "anim",
    "act",
];

/// The ops that put something on screen, and so carry an id, a scene and motion.
const NODE_OPS: &[&str] = &[
    "text",
    "image",
    "svg",
    "icon",
    "shape",
    "html",
    "ui",
    "group",
    "particles",
];

/// Entrances. Each name is implemented in `IN_FX` in motion.js, and the test at
/// the bottom of this file fails if one is listed here and missing there.
pub const IN_FX: &[&str] = &[
    "fade",
    "rise",
    "drop",
    "left",
    "right",
    "pop",
    "zoom",
    "blur",
    "wave",
    "flip",
    "spin",
    "swing",
    "converge",
    "typewriter",
    "scramble",
    "mask",
    "wipe",
    "wipe-up",
    "wipe-down",
    "iris",
    "draw",
    "stretch",
    "fly",
    "glitch",
    "elastic",
    "bounce",
    "none",
];

/// Exits, from `OUT_FX` in motion.js.
pub const OUT_FX: &[&str] = &[
    "fade", "fall", "rise", "left", "right", "pop", "zoom", "blur", "wave", "flip", "scatter",
    "shrink", "wipe", "iris", "fly", "glitch", "spin", "none",
];

/// Ambient motion that runs for as long as a layer is on screen, from `LOOP_FX`.
pub const LOOP_FX: &[&str] = &[
    "float", "sway", "spin", "pulse", "breathe", "wiggle", "flicker", "glow", "shine", "drift",
];

/// Scene transitions, from `TRANSITIONS`.
pub const TRANSITIONS: &[&str] = &[
    "cut", "dissolve", "zoom", "whip", "slide", "push", "flash", "blur", "iris", "glitch", "spin",
];

/// Micro-interactions, from `ACTS`.
pub const ACTS: &[&str] = &[
    "type",
    "stream",
    "click",
    "toggle",
    "check",
    "select",
    "count",
    "highlight",
    "strike",
    "progress",
    "class",
    "text",
];

/// Built-in interface components, from `UI` in motion.js.
pub const UI_KINDS: &[&str] = &[
    "window", "phone", "card", "input", "button", "toggle", "check", "chip", "list", "code",
    "message", "field", "cursor", "stat", "rating", "progress", "skeleton", "kbd", "tile",
];

/// Shapes, from `SHAPES`.
pub const SHAPE_KINDS: &[&str] = &["rect", "circle", "ring", "line", "glow", "path", "star"];

/// Particle systems, from `PARTICLES`.
pub const PARTICLE_KINDS: &[&str] = &[
    "stars",
    "dust",
    "bokeh",
    "burst",
    "rays",
    "shockwave",
    "confetti",
];

/// Backgrounds, from `BACKGROUNDS`.
pub const BACKGROUND_KINDS: &[&str] = &["nebula", "gradient", "grid", "solid", "aurora", "mesh"];

/// Easing names, from `EASE`.
pub const EASES: &[&str] = &[
    "linear",
    "step",
    "inQuad",
    "outQuad",
    "inOutQuad",
    "inCubic",
    "outCubic",
    "inOutCubic",
    "inQuart",
    "outQuart",
    "inOutQuart",
    "inQuint",
    "outQuint",
    "inOutQuint",
    "inExpo",
    "outExpo",
    "inOutExpo",
    "inCirc",
    "outCirc",
    "inOutCirc",
    "inBack",
    "outBack",
    "inOutBack",
    "outElastic",
    "outBounce",
    "spring",
    "snap",
];

/// Every field whose value is a time, anywhere in an op. Only these are read as
/// beats and bars; a number anywhere else is whatever that field says it is.
const TIME_KEYS: &[&str] = &[
    "at", "dur", "t", "stagger", "every", "period", "delay", "len", "hold", "offset", "duration",
    "fade_out", "blink", "from", "until",
];

/// The beat grid every musical time is measured on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub bpm: f64,
    pub beats_per_bar: f64,
    /// Seconds before the first downbeat, for a track that does not start on one.
    pub offset: f64,
}

impl Grid {
    pub fn beat(&self) -> f64 {
        60.0 / self.bpm
    }
    pub fn bar(&self) -> f64 {
        self.beat() * self.beats_per_bar
    }
}

/// A time as a script writes it, in seconds.
///
/// `2.5` and `"2.5s"` are seconds, `"800ms"` milliseconds, `"3b"` beats, `"2bar"`
/// bars, and any of those may be summed: `"4bar+2b"`, `"8b-0.5b"`. Times are
/// lengths on the grid, not positions on it, so the grid's offset is added
/// once, to scene positions, and never here.
pub fn parse_time(v: &Value, grid: &Grid) -> Result<f64, String> {
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| f.is_finite())
            .ok_or_else(|| format!("{n} is not a finite time")),
        Value::String(s) => parse_time_str(s, grid),
        other => Err(format!(
            "a time is a number of seconds or a string like \"2b\", \"1bar\", \"800ms\"; got {other}"
        )),
    }
}

fn parse_time_str(s: &str, grid: &Grid) -> Result<f64, String> {
    let src = s.trim();
    if src.is_empty() {
        return Err("an empty string is not a time".into());
    }
    let mut total = 0.0;
    let mut sign = 1.0;
    let mut rest = src;
    let mut first = true;
    loop {
        rest = rest.trim_start();
        if !first || rest.starts_with('-') || rest.starts_with('+') {
            if let Some(r) = rest.strip_prefix('+') {
                sign = 1.0;
                rest = r;
            } else if let Some(r) = rest.strip_prefix('-') {
                sign = -1.0;
                rest = r;
            } else if !first {
                return Err(format!("cannot read time \"{src}\""));
            }
        }
        first = false;
        rest = rest.trim_start();
        let num_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        if num_end == 0 {
            return Err(format!("cannot read time \"{src}\": expected a number"));
        }
        let n: f64 = rest[..num_end]
            .parse()
            .map_err(|_| format!("cannot read time \"{src}\""))?;
        rest = &rest[num_end..];
        let unit_end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = &rest[..unit_end];
        rest = &rest[unit_end..];
        let secs = match unit {
            "" | "s" | "sec" => n,
            "ms" => n / 1000.0,
            "b" | "beat" | "beats" => n * grid.beat(),
            "bar" | "bars" => n * grid.bar(),
            "f" | "fr" => {
                return Err(format!(
                    "\"{src}\": frames are not a time unit; the frame rate is a render choice"
                ))
            }
            u => {
                return Err(format!(
                    "\"{src}\": unknown time unit \"{u}\" (use s, ms, b or bar)"
                ))
            }
        };
        total += sign * secs;
        if rest.trim().is_empty() {
            break;
        }
    }
    if !total.is_finite() {
        return Err(format!("\"{src}\" is not a finite time"));
    }
    Ok(total)
}

/// Replace every time-valued field in `v` with seconds, recursively.
fn convert_times(v: &mut Value, grid: &Grid, path: &str) -> Result<(), String> {
    match v {
        Value::Object(m) => {
            for (k, val) in m.iter_mut() {
                let here = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if TIME_KEYS.contains(&k.as_str()) && (val.is_number() || val.is_string()) {
                    let secs = parse_time(val, grid).map_err(|e| format!("{here}: {e}"))?;
                    *val = json!(secs);
                } else {
                    convert_times(val, grid, &here)?;
                }
            }
        }
        Value::Array(a) => {
            for (i, val) in a.iter_mut().enumerate() {
                convert_times(val, grid, &format!("{path}[{i}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// One line of the script, remembered with where it came from so every error
/// can name it.
#[derive(Clone, Debug)]
struct Line {
    no: usize,
    op: Value,
}

fn read_lines(body: &str, name: &str) -> Result<Vec<Line>, String> {
    let mut out = Vec::new();
    for (i, raw) in body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let op: Value = serde_json::from_str(line).map_err(|e| format!("{name}:{}: {e}", i + 1))?;
        if !op.is_object() {
            return Err(format!("{name}:{}: each line must be a JSON object", i + 1));
        }
        let kind = op["op"]
            .as_str()
            .ok_or_else(|| format!("{name}:{}: missing \"op\"", i + 1))?;
        if !OPS.contains(&kind) {
            return Err(format!(
                "{name}:{}: unknown op \"{kind}\" (motion ops: {})",
                i + 1,
                OPS.join(", ")
            ));
        }
        out.push(Line { no: i + 1, op });
    }
    Ok(out)
}

/// A scene after layout: where it sits on the timeline, in seconds.
#[derive(Clone, Debug)]
pub struct Scene {
    pub id: String,
    pub start: f64,
    pub end: f64,
    /// The transition into this scene, centred on `start`.
    pub tin: Option<(String, f64)>,
}

/// A script compiled to everything the page and the synthesizer need.
pub struct Compiled {
    /// The document the runtime reads, as JSON.
    pub spec: Value,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub grid: Grid,
    pub scenes: Vec<Scene>,
    pub music: Option<synth::Music>,
    pub music_src: Option<(PathBuf, f64, f64)>,
    pub cues: Vec<synth::Cue>,
    pub warnings: Vec<String>,
}

fn f64_field(op: &Value, k: &str) -> Option<f64> {
    op.get(k).and_then(Value::as_f64)
}

fn str_field<'a>(op: &'a Value, k: &str) -> Option<&'a str> {
    op.get(k).and_then(Value::as_str)
}

fn mime_for(path: &Path) -> Result<&'static str, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        other => {
            return Err(format!(
                "{}: cannot embed a .{other} file (images: png jpg webp gif svg avif; fonts: woff2 woff ttf otf)",
                path.display()
            ))
        }
    })
}

/// Largest asset kaviri will inline. The whole page travels as one document, and
/// a 200MB PNG is a mistake rather than a design.
const MAX_ASSET_BYTES: u64 = 40 * 1024 * 1024;

fn data_uri(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_ASSET_BYTES {
        return Err(format!(
            "{} is {} MiB; assets are inlined into the page and may be at most {} MiB",
            path.display(),
            meta.len() / (1024 * 1024),
            MAX_ASSET_BYTES / (1024 * 1024)
        ));
    }
    let mime = mime_for(path)?;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn resolve(base: &Path, p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    if pb.is_absolute() {
        pb
    } else {
        base.join(pb)
    }
}

/// One of kaviri's own illustrated app icons as a standalone SVG, clipped to a
/// rounded tile. `prefix` keeps gradient ids unique on a page with many.
pub fn icon_svg(name: &str, prefix: &str) -> Option<String> {
    let art = crate::icons::art(name)?;
    let inner = art.replace("ID", prefix);
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" width=\"100%\" height=\"100%\">\
<defs><filter id=\"{prefix}sh\" x=\"-20%\" y=\"-20%\" width=\"140%\" height=\"140%\">\
<feDropShadow dx=\"0\" dy=\"2\" stdDeviation=\"2.2\" flood-opacity=\".28\"/></filter>\
<clipPath id=\"{prefix}clip\"><rect width=\"100\" height=\"100\" rx=\"23\"/></clipPath></defs>\
<g clip-path=\"url(#{prefix}clip)\">{inner}</g></svg>"
    ))
}

/// Give every id in an SVG a prefix, and every reference to one.
///
/// Inline SVGs share one document, so two files that both call a gradient "g"
/// would paint with whichever came first. Rewriting `id="x"`, `url(#x)` and
/// `href="#x"` keeps each drawing's defs its own.
pub fn scope_svg_ids(svg: &str, prefix: &str) -> String {
    let mut ids: Vec<String> = Vec::new();
    for q in ['"', '\''] {
        let pat = format!("id={q}");
        let mut rest = svg;
        while let Some(i) = rest.find(&pat) {
            // Only a whole attribute name: `grid=` and `data-id=` are not ids.
            let before = rest[..i].chars().last();
            rest = &rest[i + pat.len()..];
            if !matches!(before, Some(' ') | Some('\n') | Some('\t')) {
                continue;
            }
            if let Some(end) = rest.find(q) {
                ids.push(rest[..end].to_string());
            }
        }
    }
    if ids.is_empty() {
        return svg.to_string();
    }
    let mut out = svg.to_string();
    for id in &ids {
        let new = format!("{prefix}{id}");
        for q in ['"', '\''] {
            out = out.replace(&format!(" id={q}{id}{q}"), &format!(" id={q}{new}{q}"));
            out = out.replace(&format!("href={q}#{id}{q}"), &format!("href={q}#{new}{q}"));
        }
        out = out.replace(&format!("url(#{id})"), &format!("url(#{new})"));
        out = out.replace(&format!("url('#{id}')"), &format!("url('#{new}')"));
        out = out.replace(&format!("url(\"#{id}\")"), &format!("url(\"#{new}\")"));
    }
    out
}

fn check_name(what: &str, name: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&name) {
        Ok(())
    } else {
        Err(format!(
            "unknown {what} \"{name}\" (one of: {})",
            allowed.join(", ")
        ))
    }
}

/// An `in`, `out` or `loop` value may be a bare name or an object with `fx`.
fn fx_name(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s.as_str()),
        Value::Object(m) => m.get("fx").and_then(Value::as_str),
        _ => None,
    }
}

fn check_ease(v: &Value, here: &str) -> Result<(), String> {
    match v {
        Value::String(s) => {
            check_name("ease", s, EASES).map_err(|e| format!("{here}: {e}"))?;
        }
        Value::Array(a) => {
            if a.len() != 4 || !a.iter().all(Value::is_number) {
                return Err(format!(
                    "{here}: a cubic-bezier ease is four numbers, like [0.2, 0.8, 0.2, 1]"
                ));
            }
        }
        _ => return Err(format!("{here}: ease is a name or four numbers")),
    }
    Ok(())
}

/// Walk an op and check every ease it contains, wherever it sits.
fn check_eases(v: &Value, here: &str) -> Result<(), String> {
    match v {
        Value::Object(m) => {
            for (k, val) in m {
                let at = format!("{here}.{k}");
                if k == "ease" {
                    check_ease(val, &at)?;
                } else {
                    check_eases(val, &at)?;
                }
            }
        }
        Value::Array(a) => {
            for (i, val) in a.iter().enumerate() {
                check_eases(val, &format!("{here}[{i}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Normalize `"transition": "zoom"` and `{"kind": "zoom", "dur": ...}` to a pair.
fn transition_of(v: &Value, grid: &Grid) -> Result<Option<(String, f64)>, String> {
    let default_dur = (grid.beat() * 1.0).clamp(0.25, 0.8);
    match v {
        Value::Null => Ok(None),
        Value::String(s) => {
            check_name("transition", s, TRANSITIONS)?;
            Ok(Some((
                s.clone(),
                if s == "cut" { 0.0 } else { default_dur },
            )))
        }
        Value::Object(m) => {
            let kind = m
                .get("kind")
                .and_then(Value::as_str)
                .ok_or("a transition object needs \"kind\"")?;
            check_name("transition", kind, TRANSITIONS)?;
            let dur = match m.get("dur") {
                Some(d) => d.as_f64().ok_or("transition dur must be a time")?,
                None if kind == "cut" => 0.0,
                None => default_dur,
            };
            if dur < 0.0 {
                return Err("a transition cannot have a negative duration".into());
            }
            Ok(Some((kind.to_string(), dur)))
        }
        _ => Err("transition is a name or {\"kind\":…, \"dur\":…}".into()),
    }
}

/// The grid a script declares, for reading musical times on the command line.
pub fn script_grid(path: &str) -> Result<Grid, String> {
    let body = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let lines = read_lines(&body, path)?;
    let find = |k: &str| lines.iter().find(|l| l.op["op"] == k).map(|l| l.op.clone());
    let v = find("video").unwrap_or_else(|| json!({}));
    let m = find("music").unwrap_or_else(|| json!({}));
    let bpm = f64_field(&v, "bpm")
        .or_else(|| f64_field(&m, "bpm"))
        .unwrap_or(120.0);
    Ok(Grid {
        bpm: bpm.clamp(30.0, 300.0),
        beats_per_bar: f64_field(&v, "beats_per_bar")
            .unwrap_or(4.0)
            .clamp(1.0, 16.0),
        offset: 0.0,
    })
}

/// Compile a script. `base` is the directory relative asset paths resolve against.
pub fn compile(body: &str, name: &str, base: &Path) -> Result<Compiled, String> {
    let mut lines = read_lines(body, name)?;
    let at = |l: &Line| format!("{name}:{}", l.no);
    let mut warnings = Vec::new();

    // The grid first: every other time in the file is measured on it.
    let singletons = ["video", "music", "theme", "background"];
    for s in singletons {
        let n = lines.iter().filter(|l| l.op["op"] == s).count();
        if n > 1 {
            let second = lines.iter().filter(|l| l.op["op"] == s).nth(1).unwrap();
            return Err(format!(
                "{}: a script has at most one \"{s}\" op",
                at(second)
            ));
        }
    }
    let video_op = lines
        .iter()
        .find(|l| l.op["op"] == "video")
        .map(|l| l.op.clone())
        .unwrap_or_else(|| json!({}));
    let music_op = lines
        .iter()
        .find(|l| l.op["op"] == "music")
        .map(|l| l.op.clone());
    let bpm = f64_field(&video_op, "bpm")
        .or_else(|| music_op.as_ref().and_then(|m| f64_field(m, "bpm")))
        .unwrap_or(120.0);
    if !(30.0..=300.0).contains(&bpm) {
        return Err(format!("bpm must be between 30 and 300, got {bpm}"));
    }
    let bpb = f64_field(&video_op, "beats_per_bar").unwrap_or(4.0);
    if !(1.0..=16.0).contains(&bpb) {
        return Err(format!("beats_per_bar must be between 1 and 16, got {bpb}"));
    }
    let grid0 = Grid {
        bpm,
        beats_per_bar: bpb,
        offset: 0.0,
    };
    // The grid offset is itself a time, and may be written in beats.
    let offset = match music_op.as_ref().and_then(|m| m.get("offset")) {
        Some(v) => parse_time(v, &grid0).map_err(|e| format!("music.offset: {e}"))?,
        None => 0.0,
    };
    let grid = Grid { offset, ..grid0 };

    for l in lines.iter_mut() {
        let here = at(l);
        convert_times(&mut l.op, &grid, "").map_err(|e| format!("{here}: {e}"))?;
        check_eases(&l.op, "").map_err(|e| format!("{here}: {}", e.trim_start_matches('.')))?;
    }

    // Video shape.
    let v = lines
        .iter()
        .find(|l| l.op["op"] == "video")
        .map(|l| l.op.clone())
        .unwrap_or_else(|| json!({}));
    let (width, height) = match v.get("size") {
        Some(Value::Array(a)) if a.len() == 2 => {
            let w = a[0].as_u64().ok_or("video.size is [width, height]")? as u32;
            let h = a[1].as_u64().ok_or("video.size is [width, height]")? as u32;
            (w, h)
        }
        Some(Value::String(s)) => match s.as_str() {
            "1080p" | "landscape" => (1920, 1080),
            "720p" => (1280, 720),
            "4k" => (3840, 2160),
            "vertical" | "tiktok" | "reels" | "shorts" => (1080, 1920),
            "square" => (1080, 1080),
            other => {
                return Err(format!(
                    "video.size \"{other}\" is not a known shape (1080p, 720p, 4k, vertical, square, or [w, h])"
                ))
            }
        },
        None => (1920, 1080),
        _ => return Err("video.size is [width, height] or a named shape".into()),
    };
    if !(64..=7680).contains(&width) || !(64..=7680).contains(&height) {
        return Err(format!(
            "video.size must be between 64 and 7680 on each side, got {width}x{height}"
        ));
    }
    if width % 2 == 1 || height % 2 == 1 {
        return Err(format!(
            "video.size must be even on both sides for H.264, got {width}x{height}"
        ));
    }
    let fps = f64_field(&v, "fps").unwrap_or(30.0);
    if !(1.0..=120.0).contains(&fps) {
        return Err(format!("video.fps must be between 1 and 120, got {fps}"));
    }

    // Scenes, end to end unless placed.
    let mut scenes: Vec<Scene> = Vec::new();
    let mut scene_ids: HashSet<String> = HashSet::new();
    let mut cursor = grid.offset;
    for l in lines.iter().filter(|l| l.op["op"] == "scene") {
        let here = at(l);
        let id = str_field(&l.op, "id")
            .ok_or_else(|| format!("{here}: a scene needs an \"id\""))?
            .to_string();
        if !scene_ids.insert(id.clone()) {
            return Err(format!("{here}: scene id \"{id}\" is used twice"));
        }
        let start = match f64_field(&l.op, "at") {
            Some(a) => a + grid.offset,
            None => cursor,
        };
        let dur = f64_field(&l.op, "dur")
            .ok_or_else(|| format!("{here}: scene \"{id}\" needs a \"dur\""))?;
        if dur <= 0.0 {
            return Err(format!("{here}: scene \"{id}\" has no length"));
        }
        let tin = transition_of(l.op.get("transition").unwrap_or(&Value::Null), &grid)
            .map_err(|e| format!("{here}: {e}"))?;
        cursor = start + dur;
        scenes.push(Scene {
            id,
            start,
            end: start + dur,
            tin,
        });
    }

    // Nodes: ids, parents, scenes and every name checked.
    let mut node_ids: HashMap<String, (String, usize)> = HashMap::new();
    let mut node_scene: HashMap<String, Option<String>> = HashMap::new();
    let mut n_auto = 0usize;
    let mut icon_n = 0usize;
    for l in lines.iter_mut() {
        let kind = l.op["op"].as_str().unwrap_or("").to_string();
        if !NODE_OPS.contains(&kind.as_str()) {
            continue;
        }
        let here = format!("{name}:{}", l.no);
        let id = match str_field(&l.op, "id") {
            Some(s) => s.to_string(),
            None => {
                n_auto += 1;
                let id = format!("_{kind}{n_auto}");
                l.op["id"] = json!(id);
                id
            }
        };
        if node_ids.contains_key(&id) || scene_ids.contains(&id) {
            return Err(format!("{here}: id \"{id}\" is used twice"));
        }
        let scene = str_field(&l.op, "scene").map(str::to_string);
        if let Some(s) = &scene {
            if !scene_ids.contains(s) {
                return Err(format!("{here}: no scene with id \"{s}\""));
            }
        }
        if let Some(p) = str_field(&l.op, "parent") {
            let (pkind, _) = node_ids.get(p).ok_or_else(|| {
                format!("{here}: parent \"{p}\" must be declared on an earlier line")
            })?;
            if pkind == "text" || pkind == "particles" {
                return Err(format!(
                    "{here}: a {pkind} cannot hold other layers; use a group"
                ));
            }
            // A child lives in its parent's scene.
            let ps = node_scene.get(p).cloned().flatten();
            if scene.is_some() && scene != ps {
                return Err(format!(
                    "{here}: \"{id}\" names scene {:?} but its parent \"{p}\" is in {:?}",
                    scene, ps
                ));
            }
            if let Some(ps) = ps {
                l.op["scene"] = json!(ps);
            }
        }
        node_scene.insert(id.clone(), str_field(&l.op, "scene").map(str::to_string));
        node_ids.insert(id.clone(), (kind.clone(), l.no));

        for (field, list, what) in [("in", IN_FX, "entrance"), ("out", OUT_FX, "exit")] {
            if let Some(fx) = l.op.get(field) {
                if let Some(n) = fx_name(fx) {
                    check_name(what, n, list).map_err(|e| format!("{here}: {e}"))?;
                } else if !(fx.is_object() && (fx.get("from").is_some() || fx.get("to").is_some()))
                {
                    return Err(format!(
                        "{here}: \"{field}\" is a name, {{\"fx\":…}}, or {{\"from\":{{…}}}} / {{\"to\":{{…}}}}"
                    ));
                }
            }
        }
        if let Some(lp) = l.op.get("loop") {
            let items: Vec<&Value> = match lp {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            for it in items {
                let n = fx_name(it)
                    .ok_or_else(|| format!("{here}: each loop is a name or {{\"fx\":…}}"))?;
                check_name("loop", n, LOOP_FX).map_err(|e| format!("{here}: {e}"))?;
            }
        }
        if let Some(k) = l.op.get("keys") {
            let a = k
                .as_array()
                .ok_or_else(|| format!("{here}: keys is a list of {{\"t\":…, props}}"))?;
            for (i, key) in a.iter().enumerate() {
                if key.get("t").is_none() && key.get("at").is_none() {
                    return Err(format!("{here}: keys[{i}] needs \"t\" (or \"at\")"));
                }
            }
        }
        match kind.as_str() {
            "text" => {
                if l.op.get("text").and_then(Value::as_str).is_none() {
                    return Err(format!("{here}: a text needs \"text\""));
                }
            }
            "ui" => {
                let k = str_field(&l.op, "kind")
                    .ok_or_else(|| format!("{here}: a ui needs \"kind\""))?;
                check_name("ui kind", k, UI_KINDS).map_err(|e| format!("{here}: {e}"))?;
                // Pictures a component shows are files next to the script.
                for f in ["src", "avatar_src", "icon_src"] {
                    if let Some(src) = str_field(&l.op, f) {
                        if !src.starts_with("data:") {
                            let uri = data_uri(&resolve(base, src))
                                .map_err(|e| format!("{here}: {f}: {e}"))?;
                            l.op[f] = json!(uri);
                        }
                    }
                }
                // A tile's icon may name one of kaviri's illustrated icons.
                if let Some(ic) = str_field(&l.op, "icon") {
                    if crate::icons::art(ic).is_some() {
                        icon_n += 1;
                        let svg = icon_svg(ic, &format!("ki{icon_n}")).unwrap_or_default();
                        l.op["icon_svg"] = json!(svg);
                    }
                }
            }
            "shape" => {
                let k = str_field(&l.op, "kind").unwrap_or("rect");
                check_name("shape", k, SHAPE_KINDS).map_err(|e| format!("{here}: {e}"))?;
            }
            "particles" => {
                let k = str_field(&l.op, "kind")
                    .ok_or_else(|| format!("{here}: particles need \"kind\""))?;
                check_name("particle kind", k, PARTICLE_KINDS)
                    .map_err(|e| format!("{here}: {e}"))?;
            }
            "icon" => {
                let n = str_field(&l.op, "name")
                    .ok_or_else(|| format!("{here}: an icon needs \"name\""))?;
                icon_n += 1;
                let svg = icon_svg(n, &format!("ki{icon_n}")).ok_or_else(|| {
                    let names: Vec<&str> = crate::icons::ART.iter().map(|a| a.0).collect();
                    format!(
                        "{here}: no built-in icon \"{n}\" (one of: {})",
                        names.join(", ")
                    )
                })?;
                l.op["svg"] = json!(svg);
            }
            "image" => {
                let src = str_field(&l.op, "src")
                    .ok_or_else(|| format!("{here}: an image needs \"src\""))?;
                if !src.starts_with("data:") {
                    let p = resolve(base, src);
                    if p.extension().and_then(|e| e.to_str()) == Some("svg") {
                        let text = std::fs::read_to_string(&p)
                            .map_err(|e| format!("{here}: {}: {e}", p.display()))?;
                        l.op["svg"] = json!(scope_svg_ids(&text, &format!("s{}-", l.no)));
                    } else {
                        l.op["src"] = json!(data_uri(&p).map_err(|e| format!("{here}: {e}"))?);
                    }
                }
            }
            "svg" => {
                if l.op.get("svg").is_none() {
                    let src = str_field(&l.op, "src")
                        .ok_or_else(|| format!("{here}: an svg needs \"svg\" markup or \"src\""))?;
                    let p = resolve(base, src);
                    let text = std::fs::read_to_string(&p)
                        .map_err(|e| format!("{here}: {}: {e}", p.display()))?;
                    l.op["svg"] = json!(text);
                }
                let scoped = scope_svg_ids(
                    str_field(&l.op, "svg").unwrap_or(""),
                    &format!("s{}-", l.no),
                );
                l.op["svg"] = json!(scoped);
            }
            "html" => {
                if l.op.get("html").is_none() {
                    let src = str_field(&l.op, "src")
                        .ok_or_else(|| format!("{here}: an html needs \"html\" or \"src\""))?;
                    let p = resolve(base, src);
                    let text = std::fs::read_to_string(&p)
                        .map_err(|e| format!("{here}: {}: {e}", p.display()))?;
                    l.op["html"] = json!(text);
                }
            }
            _ => {}
        }
        if let Some(t) = l.op.get("trail") {
            if !t.is_object() && !t.is_boolean() {
                return Err(format!(
                    "{here}: trail is true or {{\"len\":…, \"width\":…}}"
                ));
            }
        }
    }

    // Everything that points at a node.
    for l in lines.iter() {
        let here = at(l);
        let kind = l.op["op"].as_str().unwrap_or("");
        match kind {
            "anim" | "act" => {
                let t = str_field(&l.op, "target")
                    .ok_or_else(|| format!("{here}: {kind} needs a \"target\""))?;
                if !node_ids.contains_key(t) {
                    return Err(format!("{here}: no layer with id \"{t}\""));
                }
                if kind == "act" {
                    let d = str_field(&l.op, "do")
                        .ok_or_else(|| format!("{here}: act needs \"do\""))?;
                    check_name("act", d, ACTS).map_err(|e| format!("{here}: {e}"))?;
                    if l.op.get("at").is_none() {
                        return Err(format!("{here}: act needs \"at\""));
                    }
                }
                if kind == "anim" {
                    if let Some(n) = l.op.get("fx").and_then(Value::as_str) {
                        let ok = IN_FX.contains(&n) || OUT_FX.contains(&n) || LOOP_FX.contains(&n);
                        if !ok {
                            return Err(format!(
                                "{here}: unknown fx \"{n}\" (an entrance, exit or loop name)"
                            ));
                        }
                    }
                }
                if let Some(s) = str_field(&l.op, "scene") {
                    if !scene_ids.contains(s) {
                        return Err(format!("{here}: no scene with id \"{s}\""));
                    }
                }
            }
            "camera" | "shake" | "flash" | "sfx" => {
                if let Some(s) = str_field(&l.op, "scene") {
                    if !scene_ids.contains(s) {
                        return Err(format!("{here}: no scene with id \"{s}\""));
                    }
                }
            }
            "background" => {
                let k = str_field(&l.op, "kind").unwrap_or("nebula");
                check_name("background", k, BACKGROUND_KINDS)
                    .map_err(|e| format!("{here}: {e}"))?;
            }
            _ => {}
        }
        if kind == "sfx" {
            let k =
                str_field(&l.op, "kind").ok_or_else(|| format!("{here}: sfx needs \"kind\""))?;
            check_name("sfx", k, synth::SFX).map_err(|e| format!("{here}: {e}"))?;
            if l.op.get("at").is_none() {
                return Err(format!("{here}: sfx needs \"at\""));
            }
        }
        if kind == "flash" && l.op.get("at").is_none() {
            return Err(format!("{here}: flash needs \"at\""));
        }
    }

    let scene_start = |s: Option<&str>| -> f64 {
        s.and_then(|id| scenes.iter().find(|sc| sc.id == id))
            .map(|sc| sc.start)
            .unwrap_or(0.0)
    };

    // Music.
    let mut music = None;
    let mut music_src = None;
    let mut auto_sfx = true;
    if let Some(l) = lines.iter().find(|l| l.op["op"] == "music") {
        let here = at(l);
        auto_sfx =
            l.op.get("auto_sfx")
                .and_then(Value::as_bool)
                .unwrap_or(true);
        let gain = f64_field(&l.op, "gain").unwrap_or(1.0);
        if let Some(src) = str_field(&l.op, "src") {
            let p = resolve(base, src);
            if !p.is_file() {
                return Err(format!("{here}: music file {} not found", p.display()));
            }
            let start = f64_field(&l.op, "start").unwrap_or(0.0);
            music_src = Some((p, start, gain));
        } else {
            music = Some(synth::Music::from_op(&l.op, grid).map_err(|e| format!("{here}: {e}"))?);
        }
    }

    // Duration: explicit, else the scenes, else the music.
    let scenes_end = scenes.iter().map(|s| s.end).fold(0.0, f64::max);
    let music_len = music.as_ref().map(|m| m.length()).unwrap_or(0.0);
    let duration = match f64_field(&v, "duration") {
        Some(d) => d,
        None => {
            let d = scenes_end.max(music_len);
            if d <= 0.0 {
                return Err(
                    "the video has no length: give video a \"duration\", or add scenes".into(),
                );
            }
            d
        }
    };
    if !(0.1..=600.0).contains(&duration) {
        return Err(format!(
            "the video must be between 0.1 and 600 seconds long, got {duration:.2}"
        ));
    }
    if scenes_end > duration + 1e-6 {
        warnings.push(format!(
            "the scenes run to {scenes_end:.2}s but the video is {duration:.2}s; the rest is cut"
        ));
    }
    if music.is_some() && music_len + 0.05 < scenes_end {
        warnings.push(format!(
            "the music is {music_len:.2}s but the scenes run to {scenes_end:.2}s; add a section so the track covers the picture"
        ));
    }

    // Sound cues: written ones, then the ones the picture implies.
    let mut cues: Vec<synth::Cue> = Vec::new();
    for l in lines.iter().filter(|l| l.op["op"] == "sfx") {
        let base_t = scene_start(str_field(&l.op, "scene"));
        cues.push(synth::Cue {
            kind: str_field(&l.op, "kind").unwrap_or("pop").to_string(),
            at: base_t + f64_field(&l.op, "at").unwrap_or(0.0),
            gain: f64_field(&l.op, "gain").unwrap_or(1.0),
            pan: f64_field(&l.op, "pan").unwrap_or(0.0),
            pitch: f64_field(&l.op, "pitch").unwrap_or(1.0),
            dur: f64_field(&l.op, "dur"),
        });
    }
    if auto_sfx && (music.is_some() || music_src.is_some()) {
        for s in &scenes {
            if let Some((k, d)) = &s.tin {
                let kind = match k.as_str() {
                    "cut" => continue,
                    "flash" => "impact",
                    "glitch" => "glitch",
                    _ => "whoosh",
                };
                cues.push(synth::Cue {
                    kind: kind.into(),
                    at: s.start - d / 2.0,
                    gain: 0.55,
                    pan: 0.0,
                    pitch: 1.0,
                    dur: Some(d.max(0.2)),
                });
            }
        }
        for l in lines.iter() {
            let op = &l.op;
            let base_t = scene_start(str_field(op, "scene"));
            match op["op"].as_str().unwrap_or("") {
                "flash" => cues.push(synth::Cue {
                    kind: "impact".into(),
                    at: base_t + f64_field(op, "at").unwrap_or(0.0),
                    gain: 0.6,
                    pan: 0.0,
                    pitch: 1.0,
                    dur: None,
                }),
                "act" => {
                    let target = str_field(op, "target").unwrap_or("");
                    let sc = node_scene.get(target).cloned().flatten();
                    let t0 = scene_start(sc.as_deref()) + f64_field(op, "at").unwrap_or(0.0);
                    if op.get("sfx").and_then(Value::as_bool) == Some(false) {
                        continue;
                    }
                    match op["do"].as_str().unwrap_or("") {
                        "type" => {
                            let text = str_field(op, "text").unwrap_or("");
                            let n = text.chars().count().min(400);
                            let dur = f64_field(op, "dur")
                                .unwrap_or_else(|| n as f64 / f64_field(op, "cps").unwrap_or(28.0));
                            cues.push(synth::Cue {
                                kind: "type".into(),
                                at: t0,
                                gain: 0.35,
                                pan: 0.0,
                                pitch: 1.0,
                                dur: Some(dur),
                            });
                        }
                        "click" => cues.push(synth::Cue {
                            kind: "click".into(),
                            at: t0,
                            gain: 0.6,
                            pan: 0.0,
                            pitch: 1.0,
                            dur: None,
                        }),
                        "toggle" | "select" => cues.push(synth::Cue {
                            kind: "tick".into(),
                            at: t0,
                            gain: 0.45,
                            pan: 0.0,
                            pitch: 1.0,
                            dur: None,
                        }),
                        "check" => cues.push(synth::Cue {
                            kind: "pop".into(),
                            at: t0,
                            gain: 0.45,
                            pan: 0.0,
                            pitch: 1.3,
                            dur: None,
                        }),
                        _ => {}
                    }
                }
                k if NODE_OPS.contains(&k) => {
                    // An entrance may ask for a sound: "in": {"fx": "pop", "sfx": "pop"}.
                    if let Some(inn) = op.get("in") {
                        if let Some(s) = inn.get("sfx").and_then(Value::as_str) {
                            check_name("sfx", s, synth::SFX)
                                .map_err(|e| format!("{}: in.sfx: {e}", at(l)))?;
                            cues.push(synth::Cue {
                                kind: s.into(),
                                at: base_t + f64_field(inn, "at").unwrap_or(0.0),
                                gain: f64_field(inn, "sfx_gain").unwrap_or(0.5),
                                pan: 0.0,
                                pitch: 1.0,
                                dur: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    cues.retain(|c| c.at < duration + 2.0);
    cues.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));

    // Fonts.
    let mut fonts = Vec::new();
    for l in lines.iter().filter(|l| l.op["op"] == "font") {
        let here = at(l);
        let family =
            str_field(&l.op, "family").ok_or_else(|| format!("{here}: font needs \"family\""))?;
        let src = str_field(&l.op, "src").ok_or_else(|| format!("{here}: font needs \"src\""))?;
        let uri = data_uri(&resolve(base, src)).map_err(|e| format!("{here}: {e}"))?;
        fonts.push(json!({
            "family": family,
            "src": uri,
            "weight": l.op.get("weight").cloned().unwrap_or(json!("100 900")),
            "style": l.op.get("style").cloned().unwrap_or(json!("normal")),
        }));
    }

    let theme = lines
        .iter()
        .find(|l| l.op["op"] == "theme")
        .map(|l| l.op.clone())
        .unwrap_or_else(|| json!({}));
    let background = lines
        .iter()
        .find(|l| l.op["op"] == "background")
        .map(|l| l.op.clone())
        .unwrap_or(Value::Null);

    let scenes_json: Vec<Value> = lines
        .iter()
        .filter(|l| l.op["op"] == "scene")
        .zip(scenes.iter())
        .map(|(l, s)| {
            let mut o = l.op.clone();
            o["start"] = json!(s.start);
            o["end"] = json!(s.end);
            o["tin"] = match &s.tin {
                Some((k, d)) => json!({"kind": k, "dur": d}),
                None => Value::Null,
            };
            o
        })
        .collect();
    let pick = |kind: &str| -> Vec<Value> {
        lines
            .iter()
            .filter(|l| l.op["op"] == kind)
            .map(|l| {
                let mut o = l.op.clone();
                o["line"] = json!(l.no);
                o
            })
            .collect()
    };
    let nodes: Vec<Value> = lines
        .iter()
        .filter(|l| NODE_OPS.contains(&l.op["op"].as_str().unwrap_or("")))
        .map(|l| {
            let mut o = l.op.clone();
            o["line"] = json!(l.no);
            o
        })
        .collect();

    let mut vout = Map::new();
    vout.insert("width".into(), json!(width));
    vout.insert("height".into(), json!(height));
    vout.insert("fps".into(), json!(fps));
    vout.insert("duration".into(), json!(duration));
    for k in [
        "pulse",
        "grain",
        "vignette",
        "perspective",
        "seed",
        "motion_blur",
        "bg",
        "letterbox",
    ] {
        if let Some(x) = v.get(k) {
            vout.insert(k.into(), x.clone());
        }
    }
    let beats_of_music = music.as_ref().map(|m| m.energy_map()).unwrap_or_default();
    let spec = json!({
        "video": Value::Object(vout),
        "grid": {"bpm": grid.bpm, "bpb": grid.beats_per_bar, "offset": grid.offset},
        "energy": beats_of_music,
        "theme": theme,
        "fonts": fonts,
        "background": background,
        "scenes": scenes_json,
        "nodes": nodes,
        "anims": pick("anim"),
        "acts": pick("act"),
        "cameras": pick("camera"),
        "shakes": pick("shake"),
        "flashes": pick("flash"),
    });

    Ok(Compiled {
        spec,
        width,
        height,
        fps,
        duration,
        grid,
        scenes,
        music,
        music_src,
        cues,
        warnings,
    })
}

/// The page, as one self-contained document.
pub fn page_html(c: &Compiled, audio_file: Option<&str>, preview: bool) -> String {
    let mut spec = c.spec.clone();
    spec["preview"] = json!(preview);
    spec["audio"] = match audio_file {
        Some(a) => json!(a),
        None => Value::Null,
    };
    // `</` inside a JSON string would close the script element early.
    let json = spec.to_string().replace("</", "<\\/");
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width={w}\">\
<title>kaviri motion</title><style>{css}</style></head>\
<body><div id=\"kv-root\"></div>\
<script>window.addEventListener('error', function (e) {{ window.__KV_ERR = e.message + ' (motion.js line ' + e.lineno + ')'; }});window.__KV_SPEC = {json};</script>\
<script>{js}</script></body></html>\n",
        w = c.width,
        css = RUNTIME_CSS,
        js = RUNTIME_JS,
    )
}

/// What `kaviri motion` was asked to do.
pub struct Opts {
    pub script: String,
    pub out: String,
    pub chromium: Option<String>,
    pub jobs: usize,
    pub fps: Option<f64>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub stills: Vec<f64>,
    pub preview: Option<String>,
    pub check: bool,
    pub audio_out: Option<String>,
    pub mute: bool,
    pub keep_temp: bool,
    pub crf: u32,
}

fn emit(v: &Value) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn fmt_bars(t: f64, g: &Grid) -> String {
    let beats = (t - g.offset) / g.beat();
    let bar = (beats / g.beats_per_bar).floor();
    let beat = beats - bar * g.beats_per_bar;
    format!("bar {} beat {:.2}", bar as i64 + 1, beat + 1.0)
}

/// A summary an agent can read without watching anything.
fn timeline(c: &Compiled) -> Value {
    let scenes: Vec<Value> = c
        .scenes
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "start": (s.start * 1000.0).round() / 1000.0,
                "end": (s.end * 1000.0).round() / 1000.0,
                "at": fmt_bars(s.start, &c.grid),
                "transition": s.tin.as_ref().map(|t| t.0.clone()),
            })
        })
        .collect();
    json!({
        "event": "motion_timeline",
        "width": c.width,
        "height": c.height,
        "fps": c.fps,
        "duration": (c.duration * 1000.0).round() / 1000.0,
        "bpm": c.grid.bpm,
        "bars": ((c.duration - c.grid.offset) / c.grid.bar() * 100.0).round() / 100.0,
        "scenes": scenes,
        "layers": c.spec["nodes"].as_array().map(Vec::len).unwrap_or(0),
        "sound_cues": c.cues.len(),
        "music": if c.music.is_some() { "synth" } else if c.music_src.is_some() { "file" } else { "none" },
    })
}

pub fn run(o: &Opts) -> Result<(), String> {
    let body =
        std::fs::read_to_string(&o.script).map_err(|e| format!("cannot read {}: {e}", o.script))?;
    let base = Path::new(&o.script)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    // Asset paths resolve against the script, like an HTML page's resolve against it.
    let base = if base.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        base
    };
    let mut c = compile(&body, &o.script, &base)?;
    if let Some(f) = o.fps {
        c.fps = f;
        c.spec["video"]["fps"] = json!(f);
    }
    for w in &c.warnings {
        eprintln!("kaviri: warning: {w}");
    }
    emit(&json!({"ok": true, "result": timeline(&c)}));
    if o.check {
        return Ok(());
    }

    let work = cdp::session_dir()?.join(format!("motion-{}", std::process::id()));
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let _cleanup = Cleanup {
        dir: work.clone(),
        keep: o.keep_temp,
    };

    // Sound first: it is quick, and the preview wants it.
    let wav = work.join("music.wav");
    let have_audio = if o.mute { false } else { make_audio(&c, &wav)? };
    if let Some(a) = &o.audio_out {
        if have_audio {
            std::fs::copy(&wav, a).map_err(|e| format!("{a}: {e}"))?;
            eprintln!("kaviri: soundtrack written to {a}");
        }
    }

    if let Some(dir) = &o.preview {
        let d = PathBuf::from(dir);
        std::fs::create_dir_all(&d).map_err(|e| format!("{dir}: {e}"))?;
        let audio = if have_audio {
            std::fs::copy(&wav, d.join("music.wav")).map_err(|e| format!("{dir}: {e}"))?;
            Some("music.wav")
        } else {
            None
        };
        let html = page_html(&c, audio, true);
        std::fs::write(d.join("index.html"), html).map_err(|e| format!("{dir}: {e}"))?;
        emit(
            &json!({"ok": true, "result": {"event": "motion_preview", "path": d.join("index.html")}}),
        );
        if o.stills.is_empty() && o.out.is_empty() {
            return Ok(());
        }
    }

    let page = work.join("index.html");
    std::fs::write(&page, page_html(&c, None, false))
        .map_err(|e| format!("{}: {e}", page.display()))?;

    if !o.stills.is_empty() {
        let paths = render_stills(o, &c, &page)?;
        emit(&json!({"ok": true, "result": {"event": "motion_stills", "paths": paths}}));
        return Ok(());
    }

    let ffmpeg = crate::zoom::find_ffmpeg()?;
    let t0 = o.from.unwrap_or(0.0).max(0.0);
    let t1 = o.to.unwrap_or(c.duration).min(c.duration);
    if t1 <= t0 {
        return Err(format!("--from {t0} is not before --to {t1}"));
    }
    let n = ((t1 - t0) * c.fps).round().max(1.0) as usize;
    let frames_dir = work.join("frames");
    std::fs::create_dir_all(&frames_dir).map_err(|e| e.to_string())?;
    let started = Instant::now();
    render_frames(o, &c, &page, &frames_dir, t0, n)?;
    let render_secs = started.elapsed().as_secs_f64();
    eprintln!(
        "kaviri: {n} frames in {render_secs:.1}s ({:.1} a second); encoding",
        n as f64 / render_secs.max(0.001)
    );
    encode(
        &ffmpeg,
        &c,
        &frames_dir,
        if have_audio { Some(&wav) } else { None },
        t0,
        n,
        &o.out,
        o.crf,
    )?;
    emit(&json!({"ok": true, "result": {
        "event": "motion_rendered",
        "path": o.out,
        "frames": n,
        "duration": ((n as f64 / c.fps) * 1000.0).round() / 1000.0,
        "fps": c.fps,
        "audio": have_audio,
        "render_seconds": (render_secs * 10.0).round() / 10.0,
    }}));
    Ok(())
}

struct Cleanup {
    dir: PathBuf,
    keep: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.keep {
            eprintln!("kaviri: kept {}", self.dir.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// The soundtrack: the synthesized score or the supplied file, with every cue on top.
fn make_audio(c: &Compiled, wav: &Path) -> Result<bool, String> {
    let len = c.duration;
    if let Some(m) = &c.music {
        let mut mix = m.render(len);
        synth::render_cues(&mut mix, &c.cues, c.grid);
        synth::master(&mut mix, len, m.fade_out);
        synth::write_wav(wav, &mix).map_err(|e| format!("{}: {e}", wav.display()))?;
        return Ok(true);
    }
    if let Some((src, start, gain)) = &c.music_src {
        // The file is decoded by ffmpeg, cues mixed in, and the result mastered here.
        let ffmpeg = crate::zoom::find_ffmpeg()?;
        let raw = wav.with_extension("src.f32");
        let out = Command::new(&ffmpeg)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{start}"))
            .arg("-i")
            .arg(src)
            .args(["-t", &format!("{len}")])
            .args(["-f", "f32le", "-ac", "2", "-ar"])
            .arg(format!("{}", synth::RATE))
            .arg(&raw)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "ffmpeg could not decode {}: {}",
                src.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let bytes = std::fs::read(&raw).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&raw);
        let mut mix = synth::Stereo::new(len);
        for (i, ch) in bytes.chunks_exact(8).enumerate() {
            if i >= mix.l.len() {
                break;
            }
            let l = f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]);
            let r = f32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]);
            mix.l[i] = l * *gain as f32;
            mix.r[i] = r * *gain as f32;
        }
        synth::render_cues(&mut mix, &c.cues, c.grid);
        synth::master(&mut mix, len, 1.5);
        synth::write_wav(wav, &mix).map_err(|e| format!("{}: {e}", wav.display()))?;
        return Ok(true);
    }
    if !c.cues.is_empty() {
        let mut mix = synth::Stereo::new(len);
        synth::render_cues(&mut mix, &c.cues, c.grid);
        synth::master(&mut mix, len, 0.3);
        synth::write_wav(wav, &mix).map_err(|e| format!("{}: {e}", wav.display()))?;
        return Ok(true);
    }
    Ok(false)
}

fn file_url(p: &Path) -> Result<String, String> {
    let abs = std::fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(format!("file://{}", abs.display()))
}

/// A browser holding the page, ready to seek.
struct Stage {
    cdp: Cdp,
}

impl Stage {
    fn open(chromium: Option<&str>, c: &Compiled, page: &Path) -> Result<Stage, String> {
        let mut cdp = Cdp::launch(chromium, c.width, c.height, 1.0)?;
        let url = file_url(page)?;
        cdp.clear_events();
        cdp.send("Page.navigate", json!({"url": url}))?;
        cdp.wait_event("Page.loadEventFired", Duration::from_secs(60))?;
        let ready = cdp.evaluate_within(
            "window.KV ? window.KV.ready.then(() => window.KV.info()) : Promise.resolve({error: window.__KV_ERR || 'the motion runtime did not start'})",
            Duration::from_secs(60),
        )?;
        if let Some(e) = ready.get("error").and_then(Value::as_str) {
            return Err(format!("the page could not be built: {e}"));
        }
        if let Some(ws) = ready.get("warnings").and_then(Value::as_array) {
            for w in ws.iter().filter_map(Value::as_str) {
                eprintln!("kaviri: warning: {w}");
            }
        }
        Ok(Stage { cdp })
    }

    fn shoot(&mut self, t: f64, format: &str) -> Result<Vec<u8>, String> {
        let r = self
            .cdp
            .evaluate_within(&format!("KV.seek({t:.6})"), Duration::from_secs(30))?;
        if let Some(e) = r.as_str() {
            if !e.is_empty() {
                return Err(format!("at {t:.3}s: {e}"));
            }
        }
        let mut params = json!({"format": format, "optimizeForSpeed": true});
        if format == "jpeg" {
            params["quality"] = json!(94);
        }
        let shot =
            self.cdp
                .send_within("Page.captureScreenshot", params, Duration::from_secs(60))?;
        let data = shot["data"].as_str().ok_or("screenshot had no data")?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("screenshot base64: {e}"))
    }
}

fn render_stills(o: &Opts, c: &Compiled, page: &Path) -> Result<Vec<String>, String> {
    let mut st = Stage::open(o.chromium.as_deref(), c, page)?;
    let mut out = Vec::new();
    let stem = o
        .out
        .strip_suffix(".png")
        .unwrap_or(o.out.strip_suffix(".mp4").unwrap_or(&o.out))
        .to_string();
    for (i, &t) in o.stills.iter().enumerate() {
        let png = st.shoot(t, "png")?;
        let path = if o.stills.len() == 1 && o.out.ends_with(".png") {
            o.out.clone()
        } else {
            format!("{stem}-{i:02}-{t:.2}s.png")
        };
        std::fs::write(&path, png).map_err(|e| format!("{path}: {e}"))?;
        out.push(path);
    }
    Ok(out)
}

/// Photograph every frame, on several browsers at once.
///
/// Each worker owns a browser and takes the next frame number off a shared
/// counter, so a slow stretch (a particle burst, a blur-heavy transition) is
/// shared out rather than landing on whichever worker drew that chunk.
fn render_frames(
    o: &Opts,
    c: &Compiled,
    page: &Path,
    dir: &Path,
    t0: f64,
    n: usize,
) -> Result<(), String> {
    let jobs = o.jobs.clamp(1, 16).min(n.max(1));
    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let fps = c.fps;
    std::thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::new();
        for _ in 0..jobs {
            let next = Arc::clone(&next);
            let done = Arc::clone(&done);
            let chromium = o.chromium.clone();
            handles.push(scope.spawn(move || -> Result<(), String> {
                let mut st = Stage::open(chromium.as_deref(), c, page)?;
                loop {
                    if cdp::shutting_down() {
                        return Err("interrupted".into());
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        return Ok(());
                    }
                    let t = t0 + i as f64 / fps;
                    let jpg = st.shoot(t, "jpeg")?;
                    let p = dir.join(format!("f{i:06}.jpg"));
                    std::fs::write(&p, jpg).map_err(|e| format!("{}: {e}", p.display()))?;
                    done.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        let started = Instant::now();
        let mut last = Instant::now();
        loop {
            if handles.iter().all(|h| h.is_finished()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            if last.elapsed() > Duration::from_secs(5) {
                last = Instant::now();
                let d = done.load(Ordering::Relaxed);
                let rate = d as f64 / started.elapsed().as_secs_f64().max(0.001);
                let eta = (n.saturating_sub(d)) as f64 / rate.max(0.001);
                eprintln!("kaviri: frame {d}/{n} ({rate:.1}/s, about {eta:.0}s left)");
            }
        }
        let mut first_err = None;
        for h in handles.drain(..) {
            match h.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    first_err.get_or_insert(e);
                }
                Err(_) => {
                    first_err.get_or_insert("a render worker panicked".into());
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn encode(
    ffmpeg: &str,
    c: &Compiled,
    frames: &Path,
    wav: Option<&Path>,
    t0: f64,
    n: usize,
    out: &str,
    crf: u32,
) -> Result<(), String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-hide_banner", "-loglevel", "error"])
        .args(["-framerate", &format!("{}", c.fps)])
        .arg("-i")
        .arg(frames.join("f%06d.jpg"));
    if let Some(w) = wav {
        cmd.args(["-ss", &format!("{t0}")]).arg("-i").arg(w);
    }
    cmd.args([
        "-c:v",
        "libx264",
        "-preset",
        "slow",
        "-crf",
        &crf.to_string(),
        "-pix_fmt",
        "yuv420p",
        "-tune",
        "animation",
        "-movflags",
        "+faststart",
    ]);
    if wav.is_some() {
        cmd.args(["-c:a", "aac", "-b:a", "256k", "-t"])
            .arg(format!("{}", n as f64 / c.fps));
    }
    cmd.arg(out).stdin(Stdio::null());
    let res = cmd.output().map_err(|e| format!("ffmpeg: {e}"))?;
    if !res.status.success() {
        return Err(format!(
            "ffmpeg failed to encode: {}",
            String::from_utf8_lossy(&res.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g() -> Grid {
        Grid {
            bpm: 120.0,
            beats_per_bar: 4.0,
            offset: 0.0,
        }
    }

    #[test]
    fn times_read_in_every_unit_and_sum() {
        let g = g();
        assert_eq!(parse_time(&json!(1.5), &g), Ok(1.5));
        assert_eq!(parse_time(&json!("1.5s"), &g), Ok(1.5));
        assert_eq!(parse_time(&json!("250ms"), &g), Ok(0.25));
        assert_eq!(parse_time(&json!("2b"), &g), Ok(1.0));
        assert_eq!(parse_time(&json!("1bar"), &g), Ok(2.0));
        assert_eq!(parse_time(&json!("2bar+1b"), &g), Ok(4.5));
        assert_eq!(parse_time(&json!("4b - 0.5b"), &g), Ok(1.75));
        assert_eq!(parse_time(&json!("-1b"), &g), Ok(-0.5));
        assert!(parse_time(&json!("3 frames"), &g).is_err());
        assert!(parse_time(&json!("12f"), &g).is_err());
        assert!(parse_time(&json!(""), &g).is_err());
        assert!(parse_time(&json!(true), &g).is_err());
    }

    #[test]
    fn only_time_fields_are_converted() {
        let mut v =
            json!({"at": "2b", "x": "2b", "keys": [{"t": "1bar", "y": 4}], "in": {"dur": "1b"}});
        convert_times(&mut v, &g(), "").unwrap();
        assert_eq!(v["at"], json!(1.0));
        assert_eq!(v["x"], json!("2b"));
        assert_eq!(v["keys"][0]["t"], json!(2.0));
        assert_eq!(v["in"]["dur"], json!(0.5));
    }

    #[test]
    fn scenes_are_laid_end_to_end_on_the_grid() {
        let s = r#"{"op":"video","bpm":120}
{"op":"scene","id":"a","dur":"2bar"}
{"op":"scene","id":"b","dur":"1bar","transition":"zoom"}
{"op":"text","scene":"b","text":"hi","in":"wave"}"#;
        let c = compile(s, "t.jsonl", Path::new(".")).unwrap();
        assert_eq!(c.scenes[0].start, 0.0);
        assert_eq!(c.scenes[1].start, 4.0);
        assert_eq!(c.scenes[1].end, 6.0);
        assert_eq!(c.duration, 6.0);
        assert_eq!(c.scenes[1].tin.as_ref().unwrap().0, "zoom");
    }

    #[test]
    fn a_misspelled_name_is_an_error_that_lists_the_real_ones() {
        let s = r#"{"op":"scene","id":"a","dur":2}
{"op":"text","scene":"a","text":"hi","in":"wobble"}"#;
        let e = compile(s, "t.jsonl", Path::new(".")).err().unwrap();
        assert!(e.contains("t.jsonl:2"), "{e}");
        assert!(e.contains("wobble") && e.contains("wave"), "{e}");
    }

    #[test]
    fn a_target_must_exist_and_a_parent_must_come_first() {
        let s = r#"{"op":"scene","id":"a","dur":2}
{"op":"act","target":"nope","do":"click","at":1}"#;
        assert!(compile(s, "t", Path::new("."))
            .err()
            .unwrap()
            .contains("no layer"));
        let s = r#"{"op":"scene","id":"a","dur":2}
{"op":"text","id":"k","scene":"a","text":"x","parent":"g"}
{"op":"group","id":"g","scene":"a"}"#;
        assert!(compile(s, "t", Path::new("."))
            .err()
            .unwrap()
            .contains("earlier line"));
    }

    #[test]
    fn a_child_inherits_its_parents_scene() {
        let s = r#"{"op":"scene","id":"a","dur":2}
{"op":"group","id":"g","scene":"a"}
{"op":"text","id":"k","text":"x","parent":"g"}"#;
        let c = compile(s, "t", Path::new(".")).unwrap();
        assert_eq!(c.spec["nodes"][1]["scene"], json!("a"));
    }

    #[test]
    fn transitions_and_acts_make_sounds_when_there_is_music() {
        let s = r#"{"op":"music","bpm":120,"sections":[{"bars":4,"part":"drop"}]}
{"op":"scene","id":"a","dur":"2bar"}
{"op":"scene","id":"b","dur":"2bar","transition":"whip"}
{"op":"ui","id":"btn","scene":"b","kind":"button","label":"Go"}
{"op":"act","target":"btn","do":"click","at":"1b"}"#;
        let c = compile(s, "t", Path::new(".")).unwrap();
        let kinds: Vec<&str> = c.cues.iter().map(|q| q.kind.as_str()).collect();
        assert!(kinds.contains(&"whoosh"), "{kinds:?}");
        assert!(kinds.contains(&"click"), "{kinds:?}");
        let click = c.cues.iter().find(|q| q.kind == "click").unwrap();
        assert!((click.at - 4.5).abs() < 1e-9);
    }

    /// Every name the compiler accepts must exist in the runtime, or a script
    /// that validates would render nothing where it asked for something.
    #[test]
    fn every_name_the_compiler_accepts_is_implemented_by_the_runtime() {
        let tables: &[(&str, &[&str])] = &[
            ("IN_FX", IN_FX),
            ("OUT_FX", OUT_FX),
            ("LOOP_FX", LOOP_FX),
            ("TRANSITIONS", TRANSITIONS),
            ("ACTS", ACTS),
            ("UI", UI_KINDS),
            ("SHAPES", SHAPE_KINDS),
            ("PARTICLES", PARTICLE_KINDS),
            ("BACKGROUNDS", BACKGROUND_KINDS),
            ("EASE", EASES),
        ];
        for (table, names) in tables {
            let start = RUNTIME_JS
                .find(&format!("const {table} = {{"))
                .unwrap_or_else(|| panic!("motion.js has no table {table}"));
            let end = start
                + RUNTIME_JS[start..]
                    .find("\n};")
                    .unwrap_or_else(|| panic!("table {table} is not closed"));
            let body = &RUNTIME_JS[start..end];
            for n in names.iter() {
                let quoted = format!("'{n}':");
                let bare = format!("\n  {n}:");
                let method = format!("\n  {n}(");
                assert!(
                    body.contains(&quoted) || body.contains(&bare) || body.contains(&method),
                    "{table} in motion.js has no entry for {n}"
                );
            }
        }
    }

    #[test]
    fn svg_ids_are_scoped_so_two_files_cannot_share_a_gradient() {
        let a = r##"<svg><defs><linearGradient id="g"/></defs><rect fill="url(#g)"/><use href="#g"/><rect data-id="x" grid="1"/></svg>"##;
        let s = scope_svg_ids(a, "s3-");
        assert!(
            s.contains(r#"id="s3-g""#)
                && s.contains("url(#s3-g)")
                && s.contains(r##"href="#s3-g""##),
            "{s}"
        );
        assert!(s.contains(r#"data-id="x""#));
    }

    #[test]
    fn built_in_icons_render_as_standalone_svg() {
        let s = icon_svg("chat", "p1").unwrap();
        assert!(s.starts_with("<svg") && s.contains("p1bg") && !s.contains("\"IDbg"));
        assert!(icon_svg("nope", "p").is_none());
    }
}
