//! The simple way to write a motion video: a brand, a list of beats, an end card.
//!
//! ```jsonl
//! {"op":"brand","name":"acme","accent":"#5b8cff","url":"acme.dev"}
//! {"op":"beat","text":"Demos go [stale.]"}
//! {"op":"beat","text":"Write it down once.","show":{"code":"kaviri record --script demo.jsonl"}}
//! {"op":"end","tagline":"Your demo video is a build artifact."}
//! ```
//!
//! A full motion script asks a writer to choose effects, times, layouts and a
//! score. Most videos want the same choices made well, so this module makes
//! them: each beat becomes a scene of two bars with its headline and one
//! "show" (a screenshot, typing, a code block, a checklist, stats, an orbit of
//! icons, chips or struck-out lines), entrances and transitions rotate so no
//! two scenes move alike, the third beat is the drop with a flash, a shake and
//! a shockwave, the beat before it builds with hyperspace rays, and the music
//! sections are written to fit (intro, build, drop, break, outro). The result
//! is ordinary motion ops, so everything the full format does still applies to
//! any line written alongside.

use serde_json::{json, Value};

/// The ops this module expands. Anything else passes through untouched.
pub const STORY_OPS: &[&str] = &["brand", "beat", "end"];

/// The things a beat can show under its headline.
pub const SHOWS: &[&str] = &[
    "image", "code", "type", "list", "stats", "icons", "chips", "strike",
];

/// A look: how every beat moves, what it sits on, and what the music defaults to.
struct Style {
    name: &'static str,
    entrances: &'static [&'static str],
    transitions: &'static [&'static str],
    /// How the music's drop is cut: its transition, and whether it shakes, bursts or throws confetti.
    drop: &'static str,
    shake: bool,
    burst: bool,
    confetti: bool,
    rays: bool,
    light: bool,
    background: &'static str,
    font: Option<&'static str>,
    weight: u32,
    tracking: f64,
    upper: bool,
    glow: bool,
    mood: &'static str,
    pulse: f64,
    letterbox: f64,
    /// One line for `kaviri motion --styles`.
    about: &'static str,
    italic: bool,
    /// The ground colour, when the style has its own (cream, pure white, near black).
    paper: Option<&'static str>,
    grain: Option<f64>,
    vignette: Option<f64>,
}

/// The looks a brand can ask for with `"style"`. The first is the default.
const STYLES: &[Style] = &[
    Style {
        name: "bold",
        entrances: &[
            "wave", "converge", "mask", "rise", "blur", "wave", "scramble", "mask",
        ],
        transitions: &["zoom", "whip", "slide", "blur", "push", "glitch", "spin"],
        drop: "flash",
        shake: true,
        burst: true,
        confetti: false,
        rays: true,
        light: false,
        background: "nebula",
        font: None,
        weight: 600,
        tracking: -0.035,
        upper: false,
        glow: true,
        mood: "energetic",
        pulse: 0.012,
        letterbox: 0.0,
        about: "big kinetic type, starfield, flash and shockwave on the drop. Dark, energetic",
        italic: false,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "minimal",
        entrances: &["rise", "fade", "mask", "blur"],
        transitions: &["dissolve", "blur", "push", "slide"],
        drop: "blur",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "solid",
        font: None,
        weight: 500,
        tracking: -0.03,
        upper: false,
        glow: false,
        mood: "calm",
        pulse: 0.0,
        letterbox: 0.0,
        about: "quiet fades and slides on a clean light ground, no effects. Light, calm",
        italic: false,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "neon",
        entrances: &["glitch", "scramble", "converge", "wave", "flip"],
        transitions: &["glitch", "whip", "zoom", "spin", "glitch"],
        drop: "glitch",
        shake: true,
        burst: true,
        confetti: false,
        rays: true,
        light: false,
        background: "grid",
        font: None,
        weight: 700,
        tracking: -0.02,
        upper: false,
        glow: true,
        mood: "energetic",
        pulse: 0.018,
        letterbox: 0.0,
        about: "glitch and scramble type on a glowing synthwave grid. Dark, energetic",
        italic: false,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "editorial",
        entrances: &["mask", "fade", "rise", "mask"],
        transitions: &["slide", "push", "dissolve", "slide"],
        drop: "iris",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "solid",
        font: Some("Georgia, 'DejaVu Serif', 'Liberation Serif', 'Times New Roman', serif"),
        weight: 500,
        tracking: -0.02,
        upper: false,
        glow: false,
        mood: "cinematic",
        pulse: 0.0,
        letterbox: 0.0,
        about: "serif type, slow reveals, an iris on the drop, magazine calm. Light, cinematic",
        italic: false,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "playful",
        entrances: &["pop", "bounce", "elastic", "swing", "spin", "wave"],
        transitions: &["spin", "iris", "zoom", "push", "slide"],
        drop: "flash",
        shake: true,
        burst: false,
        confetti: true,
        rays: false,
        light: true,
        background: "mesh",
        font: None,
        weight: 750,
        tracking: -0.03,
        upper: false,
        glow: false,
        mood: "energetic",
        pulse: 0.02,
        letterbox: 0.0,
        about: "bouncy pops, soft colour ground, confetti. Light, energetic",
        italic: false,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "cinematic",
        entrances: &["blur", "mask", "fade", "blur"],
        transitions: &["dissolve", "blur", "zoom", "dissolve"],
        drop: "flash",
        shake: true,
        burst: false,
        confetti: false,
        rays: true,
        light: false,
        background: "aurora",
        font: None,
        weight: 500,
        tracking: 0.06,
        upper: true,
        glow: true,
        mood: "cinematic",
        pulse: 0.006,
        letterbox: 0.1,
        about: "wide capitals, aurora light, letterbox bars, slow blur cuts. Dark, cinematic",
        italic: false,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "brutalist",
        entrances: &["mask", "stretch", "left", "drop"],
        transitions: &["cut", "push", "cut", "slide"],
        drop: "cut",
        shake: true,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "flat",
        font: Some("'Helvetica Neue', Helvetica, Arial, 'Liberation Sans', sans-serif"),
        weight: 800,
        tracking: -0.04,
        upper: true,
        glow: false,
        mood: "energetic",
        pulse: 0.0,
        letterbox: 0.0,
        about: "huge black capitals on pure white, hard cuts, no decoration. Light, energetic",
        italic: false,
        paper: Some("#ffffff"),
        grain: Some(0.0),
        vignette: Some(0.0),
    },
    Style {
        name: "luxury",
        entrances: &["blur", "fade", "mask", "blur"],
        transitions: &["dissolve", "blur", "dissolve", "zoom"],
        drop: "blur",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: false,
        background: "gradient",
        font: Some("Didot, 'Bodoni 72', Georgia, 'DejaVu Serif', serif"),
        weight: 400,
        tracking: 0.08,
        upper: true,
        glow: true,
        mood: "cinematic",
        pulse: 0.0,
        letterbox: 0.0,
        about: "spaced serif capitals, slow dissolves, a soft glow on near black. Dark, cinematic (try a gold accent)",
        italic: false,
        paper: Some("#070608"),
        grain: Some(0.04),
        vignette: Some(0.7),
    },
    Style {
        name: "terminal",
        entrances: &["typewriter", "scramble", "typewriter", "glitch"],
        transitions: &["cut", "glitch", "cut", "slide"],
        drop: "glitch",
        shake: true,
        burst: false,
        confetti: false,
        rays: false,
        light: false,
        background: "flat",
        font: Some("'JetBrains Mono', 'SF Mono', Menlo, 'DejaVu Sans Mono', monospace"),
        weight: 500,
        tracking: 0.0,
        upper: false,
        glow: true,
        mood: "energetic",
        pulse: 0.008,
        letterbox: 0.0,
        about: "monospace type that types and scrambles in, glitch cuts, a black screen. Dark, energetic (try a green accent)",
        italic: false,
        paper: Some("#050705"),
        grain: Some(0.05),
        vignette: Some(0.6),
    },
    Style {
        name: "hype",
        entrances: &["zoom", "left", "right", "stretch", "pop"],
        transitions: &["whip", "zoom", "whip", "spin", "push"],
        drop: "flash",
        shake: true,
        burst: true,
        confetti: false,
        rays: true,
        light: false,
        background: "nebula",
        font: None,
        weight: 800,
        tracking: -0.02,
        upper: true,
        glow: true,
        mood: "energetic",
        pulse: 0.025,
        letterbox: 0.0,
        about: "italic capitals that slam in, whip pans, a hard-hitting beat. Dark, energetic",
        italic: true,
        paper: None,
        grain: None,
        vignette: None,
    },
    Style {
        name: "corporate",
        entrances: &["rise", "fade", "mask", "rise"],
        transitions: &["push", "slide", "dissolve", "push"],
        drop: "zoom",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "gradient",
        font: None,
        weight: 600,
        tracking: -0.025,
        upper: false,
        glow: false,
        mood: "calm",
        pulse: 0.0,
        letterbox: 0.0,
        about: "clean, confident and trustworthy: gentle rises, tidy pushes. Light, calm",
        italic: false,
        paper: Some("#f6f8fb"),
        grain: Some(0.0),
        vignette: Some(0.0),
    },
    Style {
        name: "retro",
        entrances: &["drop", "bounce", "rise", "swing"],
        transitions: &["iris", "slide", "dissolve", "iris"],
        drop: "iris",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "flat",
        font: Some("Georgia, 'DejaVu Serif', 'Liberation Serif', serif"),
        weight: 700,
        tracking: -0.02,
        upper: false,
        glow: false,
        mood: "calm",
        pulse: 0.0,
        letterbox: 0.0,
        about: "warm cream paper, heavy film grain, bouncing serif, iris wipes. Light, calm",
        italic: false,
        paper: Some("#f1e6d0"),
        grain: Some(0.14),
        vignette: Some(0.4),
    },
];

/// The style names, for errors and docs.
pub fn style_names() -> Vec<&'static str> {
    STYLES.iter().map(|s| s.name).collect()
}

/// Every style with what it looks like, for `kaviri motion --styles`.
pub fn style_help() -> String {
    STYLES
        .iter()
        .map(|s| format!("  {:<10} {}", s.name, s.about))
        .collect::<Vec<_>>()
        .join("\n")
}

const MOODS: &[(&str, &str)] = &[
    ("energetic", "pulse"),
    ("cinematic", "cinematic"),
    ("calm", "minimal"),
];

fn hex(c: &str) -> Option<(f64, f64, f64)> {
    let h = c.trim().trim_start_matches('#');
    let h: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(&h, 16).ok()?;
    Some((
        ((n >> 16) & 255) as f64,
        ((n >> 8) & 255) as f64,
        (n & 255) as f64,
    ))
}

fn to_hex(r: f64, g: f64, b: f64) -> String {
    let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

/// `c` mixed towards `with` by `t`.
fn mix(c: &str, with: (f64, f64, f64), t: f64) -> String {
    let (r, g, b) = hex(c).unwrap_or((91.0, 140.0, 255.0));
    to_hex(
        r + (with.0 - r) * t,
        g + (with.1 - g) * t,
        b + (with.2 - b) * t,
    )
}

/// The longest line of a headline in characters, markup not counted.
fn longest_line(text: &str) -> usize {
    text.split('\n')
        .map(|l| {
            l.chars()
                .filter(|c| !matches!(c, '[' | ']' | '{' | '}' | '~'))
                .count()
        })
        .max()
        .unwrap_or(1)
        .max(1)
}

/// A headline size that fits the frame: big for three words, smaller for ten.
fn fit_size(text: &str, width: f64, k: f64, max: f64) -> f64 {
    fit_size_em(text, width, k, max, 0.53)
}

/// As `fit_size`, for type whose average character is `em` wide (capitals and
/// wide tracking run wider than Inter's half an em).
fn fit_size_em(text: &str, width: f64, k: f64, max: f64, em: f64) -> f64 {
    let chars = longest_line(text) as f64;
    // Inter at weight 600 averages a little over half an em per character.
    let fit = width * 0.84 / (chars * em);
    fit.clamp(40.0 * k, max * k).round()
}

fn video_size(v: &Value) -> (f64, f64) {
    match v.get("size") {
        Some(Value::Array(a)) if a.len() == 2 => (
            a[0].as_f64().unwrap_or(1920.0),
            a[1].as_f64().unwrap_or(1080.0),
        ),
        Some(Value::String(s)) => match s.as_str() {
            "720p" => (1280.0, 720.0),
            "4k" => (3840.0, 2160.0),
            "vertical" | "tiktok" | "reels" | "shorts" => (1080.0, 1920.0),
            "square" => (1080.0, 1080.0),
            _ => (1920.0, 1080.0),
        },
        _ => (1920.0, 1080.0),
    }
}

/// A stat written as it should read ("20,641+", "30s", "4.9") split into the
/// number that counts up and the text either side of it.
fn parse_stat(s: &str) -> (String, f64, String, usize) {
    let start = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    let rest = &s[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
        .unwrap_or(rest.len());
    let num = rest[..end].replace(',', "");
    let decimals = num.split('.').nth(1).map(str::len).unwrap_or(0);
    (
        s[..start].to_string(),
        num.parse().unwrap_or(0.0),
        rest[end..].to_string(),
        decimals,
    )
}

struct Out {
    ops: Vec<(usize, Value)>,
}

impl Out {
    fn push(&mut self, line: usize, v: Value) {
        self.ops.push((line, v));
    }
}

/// Expand a script's `brand`, `beat` and `end` lines. A script without any
/// is returned as it was.
pub fn expand(lines: Vec<(usize, Value)>) -> Result<Vec<(usize, Value)>, String> {
    if !lines
        .iter()
        .any(|(_, v)| STORY_OPS.contains(&v["op"].as_str().unwrap_or("")))
    {
        return Ok(lines);
    }
    let first_line = |op: &str| lines.iter().find(|(_, v)| v["op"] == op).cloned();
    let brand = first_line("brand")
        .map(|(_, v)| v)
        .unwrap_or_else(|| json!({}));
    if lines.iter().filter(|(_, v)| v["op"] == "brand").count() > 1 {
        return Err("a script has at most one \"brand\" op".into());
    }
    let video = first_line("video")
        .map(|(_, v)| v)
        .unwrap_or_else(|| json!({}));
    let (w, h) = video_size(&video);
    let k = w.min(h) / 1080.0;
    let vertical = h > w;

    let accent = brand["accent"].as_str().unwrap_or("#5b8cff").to_string();
    if hex(&accent).is_none() {
        return Err(format!(
            "brand.accent must be a hex colour like \"#c7361a\", got \"{accent}\""
        ));
    }
    let style_name = brand["style"].as_str().unwrap_or("bold");
    let st = STYLES
        .iter()
        .find(|s| s.name == style_name)
        .ok_or_else(|| {
            format!(
                "brand.style is one of {}, got \"{style_name}\"",
                style_names().join(", ")
            )
        })?;
    let light = match brand["theme"]
        .as_str()
        .unwrap_or(if st.light { "light" } else { "dark" })
    {
        "light" => true,
        "dark" => false,
        other => {
            return Err(format!(
                "brand.theme is \"light\" or \"dark\", got \"{other}\""
            ))
        }
    };
    let name = brand["name"].as_str().unwrap_or("").to_string();
    let url = brand["url"].as_str().map(str::to_string);
    let (ink, muted) = if light {
        ("#0f0f10", "#5a5a57")
    } else {
        ("#f5f5f4", "#9a9a96")
    };

    let mut out = Out { ops: Vec::new() };
    let mut beats: Vec<(usize, Value)> = Vec::new();
    let mut end: Option<(usize, Value)> = None;
    let has = |op: &str| lines.iter().any(|(_, v)| v["op"] == op);
    let has_video = has("video");
    let has_theme = has("theme");
    let has_bg = has("background");

    for (no, v) in &lines {
        match v["op"].as_str().unwrap_or("") {
            "brand" => {}
            "beat" => beats.push((*no, v.clone())),
            "end" => {
                if end.is_some() {
                    return Err(format!("line {no}: a script has at most one \"end\""));
                }
                end = Some((*no, v.clone()));
            }
            "music" => {}
            _ => out.push(*no, v.clone()),
        }
    }
    if beats.is_empty() && end.is_none() {
        return Err("a brand with no beats: add {\"op\":\"beat\",\"text\":\"…\"} lines".into());
    }
    let brand_line = first_line("brand").map(|(n, _)| n).unwrap_or(1);

    // The look.
    if !has_video {
        out.push(
            brand_line,
            json!({"op": "video", "size": [1920, 1080], "fps": 30, "bpm": 120,
                "pulse": st.pulse, "letterbox": format!("{}%", st.letterbox * 100.0 / 2.0)}),
        );
    }
    let paper = st
        .paper
        .unwrap_or(if light { "#f7f7f5" } else { "#0b0b0e" });
    if !has_theme {
        let mut t = json!({"accent": accent, "accent2": mix(&accent, (255.0, 255.0, 255.0), 0.3)});
        if light {
            t = json!({
                "bg": paper, "bg2": paper, "surface": "#ffffff", "surface2": paper,
                "border": "#dfdfdb", "border_strong": "#c4c4be", "hover": "#ededea",
                "text": ink, "text2": "#3a3a3a", "muted": muted, "faint": "#8a8a85",
                "accent": accent, "ok": "#16653c", "track": "#dfdfdb",
                "shadow": "0 30px 80px -30px rgba(15,15,16,.35), 0 2px 8px rgba(15,15,16,.06)",
            });
        } else {
            t["bg"] = json!(paper);
        }
        if let Some(f) = brand.get("font") {
            t["font"] = f.clone();
        } else if let Some(f) = st.font {
            t["font"] = json!(f);
        }
        t["op"] = json!("theme");
        out.push(brand_line, t);
    }
    if !has_bg {
        let (ar, ag, ab) = hex(&accent).unwrap_or((91.0, 140.0, 255.0));
        let bg = match (brand["background"].as_str().unwrap_or(st.background), light) {
            ("solid", _) => {
                json!({"op": "background", "kind": "gradient", "angle": 160, "spin": 1.5,
                "colors": [paper, if light { mix(&accent, (255.0, 255.0, 255.0), 0.93) } else { mix(&accent, (0.0, 0.0, 0.0), 0.88) }, paper]})
            }
            ("flat", _) => json!({"op": "background", "kind": "solid", "color": paper}),
            ("grid", _) => {
                json!({"op": "background", "kind": "grid", "base": "#07060d", "color": mix(&accent, (0.0, 0.0, 0.0), 0.35),
                "glow": accent, "stars": 60, "speed": 70})
            }
            ("aurora", _) => json!({"op": "background", "kind": "aurora", "base": "#050508",
                "colors": [accent, mix(&accent, (255.0, 255.0, 255.0), 0.4), mix(&to_hex(ab, ar, ag), (0.0, 0.0, 0.0), 0.5)], "stars": 70}),
            ("mesh", true) => json!({"op": "background", "kind": "mesh", "base": "#fbfaf7",
                "colors": [mix(&accent, (255.0, 255.0, 255.0), 0.7), mix(&to_hex(ag, ab, ar), (255.0, 255.0, 255.0), 0.7),
                           mix(&to_hex(ab, ar, ag), (255.0, 255.0, 255.0), 0.75), "#fff4d6"], "speed": 0.9}),
            ("gradient", _) => {
                json!({"op": "background", "kind": "gradient", "angle": 135, "spin": 6,
                "colors": [paper, if light { mix(&accent, (255.0, 255.0, 255.0), 0.86) } else { mix(&accent, (0.0, 0.0, 0.0), 0.6) }, paper]})
            }
            (kind, _) if kind != "nebula" && kind != "mesh" => {
                return Err(format!(
                "brand.background is nebula, mesh, grid, aurora, solid, gradient or flat, got \"{kind}\""
            ))
            }
            _ => json!({}),
        };
        let bg = if bg.get("op").is_some() {
            bg
        } else if light {
            json!({"op": "background", "kind": "mesh", "base": "#f7f7f5",
                   "colors": [mix(&accent, (255.0, 255.0, 255.0), 0.86), "#ededea", "#f4efe8", "#ecebe6"], "speed": 0.5})
        } else {
            json!({"op": "background", "kind": "nebula", "base": "#0b0b0e",
                   "colors": [mix(&accent, (0.0, 0.0, 0.0), 0.72), "#15151c", mix(&accent, (0.0, 0.0, 0.0), 0.82)],
                   "stars": 90, "intensity": 0.75})
        };
        out.push(brand_line, bg);
    }
    // Grain and vignette suit a dark frame; on paper they read as dirt.
    if !has_video {
        if let Some((_, v)) = out.ops.iter_mut().find(|(_, v)| v["op"] == "video") {
            if light {
                v["grain"] = json!(0.03);
                v["vignette"] = json!(0.08);
            }
            if let Some(g) = st.grain {
                v["grain"] = json!(g);
            }
            if let Some(g) = st.vignette {
                v["vignette"] = json!(g);
            }
        }
    }

    // The music: sections written to fit the beats.
    let beat_bars: Vec<f64> = beats
        .iter()
        .map(|(_, b)| b["bars"].as_f64().unwrap_or(2.0).clamp(0.5, 16.0))
        .collect();
    let end_bars = end
        .as_ref()
        .map(|(_, e)| e["bars"].as_f64().unwrap_or(2.0).clamp(1.0, 8.0))
        .unwrap_or(0.0);
    let n = beats.len();
    // The beat the music drops on: the third when there are three or more.
    let drop_at = match n {
        0 => None,
        1 => Some(0),
        2 => Some(1),
        _ => Some(2),
    };
    let mut parts: Vec<(f64, &str)> = Vec::new();
    for (i, b) in beat_bars.iter().enumerate() {
        let part = match drop_at {
            Some(d) if i < d => {
                if i + 1 == d {
                    "build"
                } else {
                    "intro"
                }
            }
            _ if n >= 5 && i + 1 == n => "break",
            _ => "drop",
        };
        parts.push((*b, part));
    }
    if end_bars > 0.0 {
        parts.push((end_bars, "outro"));
    }
    let sections: Vec<Value> = parts
        .iter()
        .map(|(b, p)| json!({"bars": b, "part": p}))
        .collect();
    let music_line = first_line("music");
    match music_line {
        Some((no, mut m)) => {
            if let Some(mood) = m.get("mood").and_then(Value::as_str).map(str::to_string) {
                let style = MOODS
                    .iter()
                    .find(|x| x.0 == mood)
                    .map(|x| x.1)
                    .ok_or_else(|| {
                        format!("line {no}: mood is energetic, cinematic or calm, got \"{mood}\"")
                    })?;
                m["style"] = json!(style);
                m.as_object_mut().map(|o| o.remove("mood"));
            }
            if m.get("sections").is_none() && m.get("src").is_none() {
                m["sections"] = json!(sections);
            }
            out.push(no, m);
        }
        None => {
            let mood = brand["music"].as_str().unwrap_or(st.mood);
            if mood != "none" {
                let style = MOODS
                    .iter()
                    .find(|x| x.0 == mood)
                    .map(|x| x.1)
                    .ok_or_else(|| {
                        format!("brand.music is energetic, cinematic, calm or none, got \"{mood}\"")
                    })?;
                let key = if style == "minimal" { "D" } else { "Am" };
                out.push(
                    brand_line,
                    json!({"op": "music", "style": style, "key": key, "sections": sections}),
                );
            }
        }
    }

    // The beats.
    let head_em = 0.53 + if st.upper { 0.16 } else { 0.0 } + st.tracking.max(0.0);
    let entrances = st.entrances;
    let transitions = st.transitions;
    let mut t_i = 0usize;
    for (i, (no, b)) in beats.iter().enumerate() {
        let no = *no;
        let id = format!("beat{}", i + 1);
        let bars = beat_bars[i];
        let beats_long = bars * 4.0;
        let text = b["text"]
            .as_str()
            .ok_or_else(|| format!("line {no}: a beat needs \"text\", its headline"))?
            .to_string();
        let is_drop = drop_at == Some(i) && n >= 2;
        let is_build = drop_at.is_some_and(|d| d > 0 && i + 1 == d);
        let mut scene = json!({"op": "scene", "id": id, "dur": format!("{bars}bar")});
        if i > 0 {
            if is_drop {
                scene["transition"] = json!({"kind": st.drop, "dur": if st.drop == "flash" || st.drop == "glitch" { "0.5b" } else { "1b" }});
            } else {
                scene["transition"] =
                    json!({"kind": transitions[t_i % transitions.len()], "dur": "1b"});
                t_i += 1;
            }
        }
        out.push(no, scene);
        if is_drop && st.confetti {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "confetti", "at": "0.25b", "count": 140,
                "oy": "55%", "colors": [accent, "#ffcf3f", "#ff5d8f", "#34c77b", "#5b8cff"]}));
        }
        if is_drop && st.shake {
            out.push(
                no,
                json!({"op": "shake", "scene": id, "at": 0, "dur": "1b", "amp": 12}),
            );
        }
        if is_drop && st.burst {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "shockwave", "at": 0, "dur": "2b",
                "rings": 3, "width": 10, "colors": [accent, ink], "blend": if light { "normal" } else { "screen" }}));
            out.push(no, json!({"op": "particles", "scene": id, "kind": "burst", "at": 0, "count": 90,
                "speed": 1100, "colors": [accent, ink], "blend": if light { "normal" } else { "screen" }}));
        }
        if is_build && st.rays {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "rays", "at": format!("{}b", (beats_long * 0.6).round()),
                "dur": format!("{}b", beats_long - (beats_long * 0.6).round()), "count": 150, "speed": 1.5,
                "colors": [accent, ink], "blend": if light { "multiply" } else { "screen" }}));
        }

        let show = b.get("show");
        let fx = entrances[i % entrances.len()];
        let split = if fx == "mask" || fx == "rise" {
            "word"
        } else {
            "char"
        };
        let mut inn = json!({"fx": fx, "at": "0.25b", "dur": "1b", "split": split});
        if split == "word" {
            inn["stagger"] = json!("0.25b");
        }
        let sub = b["sub"].as_str();
        if show.is_none() {
            let size = fit_size_em(&text, w, k, 140.0, head_em);
            out.push(no, json!({"op": "text", "id": format!("{id}_t"), "scene": id, "text": text, "size": size,
                "weight": st.weight, "tracking": st.tracking, "upper": st.upper, "italic": st.italic, "leading": 1.1, "y": if sub.is_some() { "44%" } else { "50%" },
                "in": inn, "loop": {"fx": "glow", "amp": if light || !st.glow { 0 } else { 14 }}}));
            if let Some(s) = sub {
                out.push(no, json!({"op": "text", "scene": id, "text": s, "size": fit_size(s, w, k, 44.0),
                    "weight": 500, "color": muted, "y": "60%", "in": {"fx": "blur", "at": "2b", "dur": "1b"}}));
            }
            continue;
        }
        let show = show.unwrap();
        let kinds: Vec<&str> = SHOWS
            .iter()
            .copied()
            .filter(|s| show.get(*s).is_some())
            .collect();
        // A typed prompt may carry chips (the models it is sent to); they are its option, not a second show.
        let kinds: Vec<&str> = if kinds.contains(&"type") {
            vec!["type"]
        } else {
            kinds
        };
        if kinds.len() != 1 {
            return Err(format!(
                "line {no}: \"show\" holds exactly one of: {} (for example {{\"image\":\"shot.png\"}})",
                SHOWS.join(", ")
            ));
        }
        let size = fit_size_em(&text, w, k, 84.0, head_em);
        out.push(
            no,
            json!({"op": "text", "id": format!("{id}_t"), "scene": id, "fixed": true, "text": text,
            "size": size, "weight": st.weight, "tracking": st.tracking, "upper": st.upper, "italic": st.italic, "leading": 1.1,
            "y": if vertical { "12%" } else { "15%" }, "in": inn}),
        );
        let mut body_y = if vertical { 52.0 } else { 58.0 };
        if let Some(s) = sub {
            out.push(no, json!({"op": "text", "scene": id, "fixed": true, "text": s, "size": fit_size(s, w, k, 30.0),
                "weight": 500, "color": muted, "y": if vertical { "17.5%" } else { "24%" },
                "in": {"fx": "fade", "at": "1b"}}));
            body_y += 2.0;
        }
        let y = format!("{body_y}%");
        let end_b = format!("{}b", (beats_long - 1.5).max(1.0));
        match kinds[0] {
            "image" => {
                let src = show["image"]
                    .as_str()
                    .ok_or_else(|| format!("line {no}: show.image is a file path"))?;
                let fw = if vertical { w * 0.86 } else { w * 0.6 };
                let fh = show["height"].as_f64().map(|x| x * k).unwrap_or(fw * 0.62);
                let frame = show["frame"].as_str().unwrap_or("browser");
                if frame == "none" {
                    out.push(no, json!({"op": "image", "id": format!("{id}_s"), "scene": id, "src": src, "w": fw, "h": fh,
                        "fit": "cover", "radius": 14, "shadow": true, "y": y, "rx": 24, "scale": 0.9,
                        "keys": [{"t": "2b", "rx": 0, "scale": 1, "ease": "outExpo"}],
                        "in": {"fx": "fly", "at": 0, "dur": "1.5b"}, "loop": {"fx": "float", "amp": 6, "period": 4}}));
                } else {
                    let mut win = json!({"op": "ui", "id": format!("{id}_s"), "scene": id, "kind": "window",
                        "w": fw, "h": fh + 38.0, "y": y, "rx": 24, "scale": 0.9,
                        "keys": [{"t": "2b", "rx": 0, "scale": 1, "ease": "outExpo"}],
                        "in": {"fx": "fly", "at": 0, "dur": "1.5b"}, "loop": {"fx": "float", "amp": 6, "period": 4}});
                    win["url"] = json!(show["url"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| url.clone())
                        .unwrap_or_default());
                    out.push(no, win);
                    out.push(no, json!({"op": "image", "parent": format!("{id}_s"), "src": src, "w": fw, "h": fh,
                        "fit": "cover", "anchor": [0, 0], "x": 0, "y": 0}));
                }
            }
            "code" => {
                let code = show["code"]
                    .as_str()
                    .ok_or_else(|| format!("line {no}: show.code is the code, as text"))?;
                let cw = if vertical { w * 0.9 } else { w * 0.62 };
                out.push(no, json!({"op": "ui", "id": format!("{id}_s"), "scene": id, "kind": "code", "code": code,
                    "title": show["title"].as_str().unwrap_or("terminal"), "lang": show["lang"].as_str().unwrap_or("sh"),
                    "size": (23.0 * k).round(), "w": cw, "y": y, "ry": if vertical { 0 } else { -10 },
                    "keys": [{"t": end_b, "ry": 0}], "in": {"fx": "rise", "at": 0, "dur": "1b"}}));
                out.push(
                    no,
                    json!({"op": "act", "target": format!("{id}_s"), "do": "type", "at": "0.5b",
                    "dur": format!("{}b", (beats_long - 2.5).max(1.0))}),
                );
            }
            "type" => {
                let typed = show["type"]
                    .as_str()
                    .ok_or_else(|| format!("line {no}: show.type is the text to type"))?;
                let iw = if vertical { w * 0.7 } else { w * 0.42 };
                let mut input = json!({"op": "ui", "id": format!("{id}_s"), "scene": id, "kind": "input", "w": iw,
                    "y": y, "scale": 1.7 * k, "in": {"fx": "rise", "at": 0, "dur": "1b"},
                    "placeholder": show["placeholder"].as_str().unwrap_or("Ask anything…")});
                if let Some(c) = show.get("chips") {
                    input["chips"] = c.clone();
                }
                out.push(no, input);
                let n_chars = typed.chars().count() as f64;
                let room = ((beats_long - 3.0) * 0.5 * 120.0 / 120.0).max(0.8);
                out.push(
                    no,
                    json!({"op": "act", "target": format!("{id}_s"), "do": "type", "at": "1b",
                    "text": typed, "cps": (n_chars / room).max(12.0)}),
                );
                out.push(no, json!({"op": "act", "target": format!("{id}_s"), "do": "click", "sel": ".kv-send", "at": end_b}));
            }
            "list" => {
                let items: Vec<String> = show["list"]
                    .as_array()
                    .ok_or_else(|| format!("line {no}: show.list is a list of lines"))?
                    .iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect();
                let rows: Vec<Value> = items
                    .iter()
                    .map(|l| json!({"label": l, "check": false}))
                    .collect();
                out.push(no, json!({"op": "ui", "id": format!("{id}_s"), "scene": id, "kind": "list", "items": rows,
                    "w": if vertical { w * 0.5 } else { w * 0.26 }, "y": y, "scale": 2.1 * k,
                    "in": {"fx": "rise", "at": 0, "dur": "1b"}}));
                for (j, _) in items.iter().enumerate() {
                    out.push(no, json!({"op": "act", "target": format!("{id}_s"), "do": "check", "index": j,
                        "at": format!("{}b", 1.5 + j as f64 * ((beats_long - 3.0) / items.len().max(1) as f64).min(1.0))}));
                }
            }
            "stats" => {
                let stats = show["stats"].as_array().ok_or_else(|| format!("line {no}: show.stats is a list of [\"20,641+\", \"happy customers\"] pairs"))?;
                out.push(
                    no,
                    json!({"op": "group", "id": format!("{id}_s"), "scene": id, "y": y,
                    "layout": {"kind": if vertical { "column" } else { "row" }, "gap": 110.0 * k},
                    "cascade": {"fx": "rise", "at": "0.5b", "stagger": "0.35b"}}),
                );
                for (j, s) in stats.iter().enumerate() {
                    let (big, label) = match s {
                        Value::Array(a) => (
                            a.first().and_then(Value::as_str).unwrap_or("0"),
                            a.get(1).and_then(Value::as_str).unwrap_or(""),
                        ),
                        Value::String(x) => (x.as_str(), ""),
                        _ => return Err(format!("line {no}: each stat is [\"value\", \"label\"]")),
                    };
                    let (prefix, value, suffix, decimals) = parse_stat(big);
                    let sid = format!("{id}_s{j}");
                    out.push(no, json!({"op": "ui", "id": sid, "parent": format!("{id}_s"), "kind": "stat", "value": 0,
                        "prefix": prefix, "suffix": suffix, "decimals": decimals, "label": label,
                        "size": (104.0 * k).round(), "w": (if vertical { w * 0.8 } else { w * 0.8 / stats.len().max(1) as f64 }).round(), "color": if j == 0 { accent.clone() } else { ink.to_string() }}));
                    out.push(
                        no,
                        json!({"op": "act", "target": sid, "do": "count", "to": value,
                        "at": format!("{}b", 0.8 + j as f64 * 0.35), "dur": "2.5b"}),
                    );
                }
            }
            "icons" => {
                let names = show["icons"]
                    .as_array()
                    .ok_or_else(|| format!("line {no}: show.icons is a list of icon names"))?;
                let r = if vertical { w * 0.38 } else { w * 0.27 };
                out.push(no, json!({"op": "group", "id": format!("{id}_s"), "scene": id, "y": y,
                    "layout": {"kind": "orbit", "rx": r, "ry": r * 0.3, "period": 7, "depth": 0.45, "tilt": -6},
                    "trail": {"len": 0.5, "width": 5, "color": accent, "opacity": 0.6},
                    "cascade": {"fx": "pop", "at": "0.25b", "stagger": "0.2b"}}));
                for nm in names.iter().filter_map(Value::as_str) {
                    if crate::icons::art(nm).is_some() {
                        out.push(no, json!({"op": "icon", "parent": format!("{id}_s"), "name": nm, "size": (100.0 * k).round()}));
                    } else {
                        let glyph: String = nm.chars().take(1).collect::<String>().to_uppercase();
                        out.push(no, json!({"op": "ui", "parent": format!("{id}_s"), "kind": "tile", "glyph": glyph,
                            "size": (96.0 * k).round(), "bg": [accent, mix(&accent, (0.0, 0.0, 0.0), 0.35)]}));
                    }
                }
            }
            "chips" => {
                let chips = show["chips"]
                    .as_array()
                    .ok_or_else(|| format!("line {no}: show.chips is a list of short labels"))?;
                out.push(
                    no,
                    json!({"op": "group", "id": format!("{id}_s"), "scene": id, "y": y,
                    "layout": {"kind": if vertical { "column" } else { "row" }, "gap": 22.0 * k},
                    "cascade": {"fx": "pop", "at": "1b", "stagger": "0.5b", "sfx": "pop"}}),
                );
                for c in chips.iter().filter_map(Value::as_str) {
                    out.push(
                        no,
                        json!({"op": "ui", "parent": format!("{id}_s"), "kind": "chip", "label": c,
                        "icon": "✓", "color": accent, "size": (40.0 * k).round()}),
                    );
                }
            }
            "strike" => {
                let items = show["strike"].as_array().ok_or_else(|| {
                    format!("line {no}: show.strike is a list of lines to cross out")
                })?;
                out.push(no, json!({"op": "group", "id": format!("{id}_s"), "scene": id, "y": y,
                    "layout": {"kind": "column", "gap": 20.0 * k}, "cascade": {"fx": "rise", "at": "0.5b", "stagger": "0.4b"}}));
                for (j, s) in items.iter().filter_map(Value::as_str).enumerate() {
                    let sid = format!("{id}_x{j}");
                    out.push(no, json!({"op": "text", "id": sid, "parent": format!("{id}_s"), "text": format!("~{s}~"),
                        "size": fit_size(s, w, k, 56.0), "weight": 500, "color": muted, "strike_color": accent}));
                    out.push(
                        no,
                        json!({"op": "act", "target": sid, "do": "strike",
                        "at": format!("{}b", 2.5 + j as f64 * 0.5), "dur": "0.4b"}),
                    );
                    out.push(no, json!({"op": "sfx", "kind": "tick", "scene": id, "at": format!("{}b", 2.5 + j as f64 * 0.5), "pitch": 1.0 + j as f64 * 0.2}));
                }
            }
            _ => unreachable!(),
        }
    }

    // The end card.
    if let Some((no, e)) = end {
        let id = "end";
        let bars = end_bars;
        let title = e["name"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| name.clone());
        if title.is_empty() {
            return Err(format!(
                "line {no}: the end card needs a \"name\" (here or on brand)"
            ));
        }
        out.push(
            no,
            json!({"op": "scene", "id": id, "dur": format!("{bars}bar"), "push": 0.025,
            "transition": {"kind": st.drop, "dur": if st.drop == "flash" || st.drop == "glitch" { "0.5b" } else { "1b" }}}),
        );
        if st.confetti {
            out.push(
                no,
                json!({"op": "particles", "scene": id, "kind": "confetti", "at": "4b", "count": 120,
                "oy": "73%", "colors": [accent, "#ffcf3f", "#ff5d8f", "#34c77b", "#5b8cff"]}),
            );
        }
        if st.shake || st.burst {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "shockwave", "at": 0, "dur": "2.5b", "rings": 2,
            "width": 8, "oy": "42%", "colors": [accent], "blend": if light { "normal" } else { "screen" }}));
        }
        if !light {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "dust", "count": 45, "colors": [accent, "#ffffff"]}));
        }
        let name_size = fit_size_em(&title, w * 0.7, k, 170.0, head_em);
        out.push(
            no,
            json!({"op": "group", "id": "end_lockup", "scene": id, "y": "42%",
            "layout": {"kind": "row", "gap": 34.0 * k}}),
        );
        let mark = (name_size * 0.9).round();
        match e["logo"].as_str().or_else(|| brand["logo"].as_str()) {
            Some(logo) => out.push(
                no,
                json!({"op": "image", "parent": "end_lockup", "src": logo, "w": mark, "h": mark,
                "fit": "contain", "in": {"fx": "pop", "at": 0, "dur": "1b"}}),
            ),
            None => {
                let glyph: String = title.chars().take(1).collect::<String>().to_uppercase();
                out.push(no, json!({"op": "ui", "parent": "end_lockup", "kind": "tile", "glyph": glyph, "size": mark,
                    "bg": [accent, mix(&accent, (0.0, 0.0, 0.0), 0.35)], "in": {"fx": "pop", "at": 0, "dur": "1b"}}));
            }
        }
        out.push(no, json!({"op": "text", "parent": "end_lockup", "text": title, "size": name_size, "weight": st.weight.max(600),
            "tracking": if st.upper { 0.02 } else { -0.045 }, "upper": st.upper, "in": {"fx": "rise", "at": "0.75b", "dur": "1b", "stagger": "0.08b"}}));
        if let Some(t) = e["tagline"].as_str().or_else(|| brand["tagline"].as_str()) {
            out.push(no, json!({"op": "text", "scene": id, "text": t, "size": fit_size(t, w, k, 40.0), "weight": 600,
                "tracking": -0.02, "y": "61%", "in": {"fx": "blur", "at": "2.5b", "dur": "1b"}}));
        }
        if let Some(u) = e["url"].as_str().map(str::to_string).or(url) {
            out.push(no, json!({"op": "ui", "scene": id, "kind": "button", "label": u, "size": (24.0 * k).round(),
                "y": "73%", "in": {"fx": "pop", "at": "4b", "sfx": "pop"}, "shine": true,
                "loop": {"fx": "shine", "period": "4b", "phase": "5b"}}));
        }
    }
    Ok(out.ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<Vec<Value>, String> {
        let lines = src
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, l)| (i + 1, serde_json::from_str::<Value>(l).unwrap()))
            .collect();
        expand(lines).map(|v| v.into_iter().map(|x| x.1).collect())
    }

    #[test]
    fn three_lines_become_a_scored_video() {
        let ops = run(r##"{"op":"brand","name":"acme","accent":"#c7361a"}
{"op":"beat","text":"One"}
{"op":"beat","text":"Two"}
{"op":"beat","text":"Three","show":{"chips":["a","b"]}}
{"op":"end","tagline":"Done."}"##)
        .unwrap();
        let kinds: Vec<&str> = ops.iter().map(|o| o["op"].as_str().unwrap()).collect();
        for k in [
            "video",
            "theme",
            "background",
            "music",
            "scene",
            "text",
            "shake",
            "group",
            "ui",
        ] {
            assert!(kinds.contains(&k), "no {k} in {kinds:?}");
        }
        let music = ops.iter().find(|o| o["op"] == "music").unwrap();
        let parts: Vec<&str> = music["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["part"].as_str().unwrap())
            .collect();
        assert_eq!(parts, ["intro", "build", "drop", "outro"]);
        // The drop lands on the third beat, with a flash.
        let third = ops.iter().filter(|o| o["op"] == "scene").nth(2).unwrap();
        assert_eq!(third["transition"]["kind"], "flash");
    }

    #[test]
    fn every_style_expands_and_a_wrong_one_lists_them() {
        for st in style_names() {
            let src = format!(
                "{{\"op\":\"brand\",\"name\":\"a\",\"style\":\"{st}\"}}\n{{\"op\":\"beat\",\"text\":\"x\"}}\n{{\"op\":\"beat\",\"text\":\"y\"}}\n{{\"op\":\"end\"}}"
            );
            let ops = run(&src).unwrap_or_else(|e| panic!("{st}: {e}"));
            assert!(ops.iter().any(|o| o["op"] == "background"), "{st}");
        }
        let e = run(r#"{"op":"brand","name":"a","style":"grunge"}
{"op":"beat","text":"x"}"#)
        .err()
        .unwrap();
        assert!(e.contains("editorial") && e.contains("grunge"), "{e}");
    }

    #[test]
    fn a_script_without_story_ops_is_untouched() {
        let ops = run(r#"{"op":"scene","id":"a","dur":2}"#).unwrap();
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn a_show_with_two_things_is_an_error_that_lists_the_options() {
        let e = run(r#"{"op":"beat","text":"x","show":{"image":"a.png","code":"b"}}"#)
            .err()
            .unwrap();
        assert!(e.contains("line 1") && e.contains("stats"), "{e}");
    }

    #[test]
    fn stats_split_into_the_number_and_its_dressing() {
        assert_eq!(
            parse_stat("20,641+"),
            (String::new(), 20641.0, "+".into(), 0)
        );
        assert_eq!(parse_stat("$4.9M"), ("$".into(), 4.9, "M".into(), 1));
        assert_eq!(parse_stat("30s"), (String::new(), 30.0, "s".into(), 0));
    }

    #[test]
    fn headlines_shrink_to_fit_the_frame() {
        assert_eq!(fit_size("Hi", 1920.0, 1.0, 140.0), 140.0);
        let long = fit_size(
            "A much longer headline that would not fit at full size",
            1920.0,
            1.0,
            140.0,
        );
        assert!((40.0..70.0).contains(&long), "{long}");
    }
}
