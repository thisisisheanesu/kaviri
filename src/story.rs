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
    "image", "photo", "code", "type", "list", "stats", "number", "roll", "icons", "chips",
    "strike", "stack", "clip", "devices", "cycle", "wall", "float",
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
    /// Beats take turns on the light and the dark ground.
    alternate: bool,
    /// The two grounds when a style alternates or a beat asks for the other theme.
    paper_light: Option<&'static str>,
    paper_dark: Option<&'static str>,
    /// A second face for the [accent] words: italic serif inside a sans headline.
    accent_font: Option<&'static str>,
    accent_italic: bool,
    /// The [accent] words painted with a gradient from the accent to the second accent.
    accent_gradient: bool,
    /// A field of sparkles under the end card.
    sparkles: bool,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
    },
    Style {
        name: "announcement",
        entrances: &["rise", "blur", "mask", "dots", "rise"],
        transitions: &["slide", "push", "dissolve", "slide", "push"],
        drop: "push",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "flat",
        font: None,
        weight: 500,
        tracking: -0.035,
        upper: false,
        glow: false,
        mood: "energetic",
        pulse: 0.0,
        letterbox: 0.0,
        about: "cream and forest-green scenes in turn, italic serif accent words, huge counting numbers. Light and dark, energetic (try a mint accent)",
        italic: false,
        paper: Some("#f2efe8"),
        grain: Some(0.02),
        vignette: Some(0.0),
        alternate: true,
        paper_light: Some("#f2efe8"),
        paper_dark: Some("#06170f"),
        accent_font: Some("Georgia, 'DejaVu Serif', 'Times New Roman', serif"),
        accent_italic: true,
        accent_gradient: false,
        sparkles: false,
    },
    Style {
        name: "launch",
        entrances: &["blur", "rise", "mask", "blur", "wave"],
        transitions: &["blur", "dissolve", "zoom", "blur", "dissolve"],
        drop: "zoom",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: false,
        background: "flat",
        font: None,
        weight: 300,
        tracking: -0.035,
        upper: false,
        glow: true,
        mood: "energetic",
        pulse: 0.006,
        letterbox: 0.0,
        about: "pure black, thin elegant type, accent words in a pink-to-violet gradient, odometer numbers, sparkles. Dark, energetic",
        italic: false,
        paper: Some("#000000"),
        grain: Some(0.025),
        vignette: Some(0.3),
        alternate: false,
        paper_light: None,
        paper_dark: Some("#000000"),
        accent_font: None,
        accent_italic: false,
        accent_gradient: true,
        sparkles: true,
    },
    Style {
        name: "gallery",
        entrances: &["blur", "rise", "fade", "blur", "mask"],
        transitions: &["zoom", "blur", "dissolve", "zoom", "push"],
        drop: "zoom",
        shake: false,
        burst: false,
        confetti: false,
        rays: false,
        light: true,
        background: "pastel",
        font: None,
        weight: 500,
        tracking: -0.035,
        upper: false,
        glow: false,
        mood: "calm",
        pulse: 0.0,
        letterbox: 0.0,
        about: "soft pastel light, words among floating screenshots, a camera gliding over the product. Light, calm",
        italic: false,
        paper: Some("#f6f5f3"),
        grain: Some(0.015),
        vignette: Some(0.0),
        alternate: false,
        paper_light: None,
        paper_dark: None,
        accent_font: None,
        accent_italic: false,
        accent_gradient: false,
        sparkles: false,
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
/// Breaks a one-line headline into lines of about a dozen letters, so it can be set large in
/// a narrow frame. `[accent]`, `{muted}` and `~struck~` markup carries across the break.
fn wrap_short(text: &str) -> String {
    let n = longest_line(text);
    if text.contains('\n') || n <= 14 {
        return text.to_string();
    }
    let lines = n.div_ceil(12).min(3);
    let target = n / lines;
    // Markup open at a break is closed before it and opened again after it.
    let (mut out, mut run, mut open) = (String::new(), 0, Vec::<char>::new());
    for c in text.chars() {
        match c {
            '[' | '{' => open.push(c),
            ']' | '}' => {
                open.pop();
            }
            '~' if open.last() == Some(&'~') => {
                open.pop();
            }
            '~' => open.push('~'),
            _ => {}
        }
        if c == ' ' && run >= target {
            out.extend(open.iter().rev().map(|o| match o {
                '[' => ']',
                '{' => '}',
                _ => '~',
            }));
            out.push('\n');
            out.extend(open.iter());
            run = 0;
            continue;
        }
        if !matches!(c, '[' | ']' | '{' | '}' | '~') {
            run += 1;
        }
        out.push(c);
    }
    out
}

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
    // The floor gives way to a maximum below it, so a small line can ask to stay small.
    fit.clamp((40.0 * k).min(max * k), max * k).round()
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
    // The second accent a gradient runs to: given, or a violet that sits well after most accents.
    let accent2 = brand["accent2"].as_str().unwrap_or("#a78bfa").to_string();
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
            ("pastel", _) => json!({"op": "background", "kind": "mesh", "base": "#f6f5f3",
                "colors": ["#fcd5c2", "#c9d6ff", "#e7d6ff", mix(&accent, (255.0, 255.0, 255.0), 0.75)], "speed": 0.5}),
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
            (kind, _) if kind != "nebula" && kind != "mesh" && kind != "pastel" => {
                return Err(format!(
                "brand.background is nebula, mesh, grid, aurora, solid, gradient, flat or pastel, got \"{kind}\""
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
            // Light and dark beats in one video: a heavy vignette muddies the light ones.
            let mixed = st.alternate || beats.iter().any(|(_, b)| b.get("theme").is_some());
            if mixed {
                let cur = v["vignette"].as_f64().unwrap_or(0.55);
                v["vignette"] = json!(cur.min(0.15));
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
    // intro: "logo" opens on the mark alone for a bar, under the start of the music.
    let logo_intro = brand["intro"].as_str() == Some("logo");
    if logo_intro {
        parts.push((1.0, "intro"));
    }
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
    let mut t_i = 0usize;
    if logo_intro {
        out.push(
            brand_line,
            json!({"op": "scene", "id": "intro", "dur": "1bar", "push": 0.03}),
        );
        let mark = (220.0 * k).round();
        match brand["logo"].as_str() {
            Some(logo) => out.push(brand_line, json!({"op": "image", "scene": "intro", "src": logo, "w": mark, "h": mark,
                "fit": "contain", "ry": -110, "scale": 0.6, "keys": [{"t": "2b", "ry": 0, "scale": 1, "ease": "outBack"}],
                "in": {"fx": "fade", "at": 0, "dur": "0.5b"}})),
            None => {
                let glyph: String = name.chars().take(1).collect::<String>().to_uppercase();
                out.push(brand_line, json!({"op": "ui", "scene": "intro", "kind": "tile", "glyph": glyph, "size": mark,
                    "bg": [accent, mix(&accent, (0.0, 0.0, 0.0), 0.35)], "ry": -110, "scale": 0.6,
                    "keys": [{"t": "2b", "ry": 0, "scale": 1, "ease": "outBack"}], "in": {"fx": "fade", "at": 0, "dur": "0.5b"}}));
            }
        }
    }
    for (i, (no, b)) in beats.iter().enumerate() {
        let no = *no;
        let id = format!("beat{}", i + 1);
        let bars = beat_bars[i];
        let beats_long = bars * 4.0;
        let text = b["text"]
            .as_str()
            .ok_or_else(|| format!("line {no}: a beat needs \"text\", its headline"))?
            .to_string();
        // A phone frame is narrow: a long headline wraps to short lines and stays big.
        let text = if vertical { wrap_short(&text) } else { text };
        // A beat may borrow another style's motion, and sit on the other ground.
        let bst = match b["style"].as_str() {
            Some(name) => STYLES.iter().find(|s| s.name == name).ok_or_else(|| {
                format!(
                    "line {no}: style is one of {}, got \"{name}\"",
                    style_names().join(", ")
                )
            })?,
            None => st,
        };
        let beat_light = match b["theme"].as_str() {
            Some("light") => true,
            Some("dark") => false,
            Some(other) => {
                return Err(format!(
                    "line {no}: theme is \"light\" or \"dark\", got \"{other}\""
                ))
            }
            None if st.alternate => {
                if i % 2 == 0 {
                    light
                } else {
                    !light
                }
            }
            None => light,
        };
        let (b_ink, b_muted) = if beat_light {
            ("#0f0f10", "#5a5a57")
        } else {
            ("#f5f5f4", "#9a9a96")
        };
        let head_em = 0.53 + if bst.upper { 0.16 } else { 0.0 } + bst.tracking.max(0.0);
        let is_drop = drop_at == Some(i) && n >= 2;
        let is_build = drop_at.is_some_and(|d| d > 0 && i + 1 == d);
        let mut scene = json!({"op": "scene", "id": id, "dur": format!("{bars}bar")});
        if beat_light != light {
            scene["bg"] = json!(ground(bst, st, beat_light));
        }
        if i > 0 || logo_intro {
            if is_drop {
                scene["transition"] = json!({"kind": bst.drop, "dur": if bst.drop == "flash" || bst.drop == "glitch" { "0.5b" } else { "1b" }});
            } else {
                scene["transition"] =
                    json!({"kind": bst.transitions[t_i % bst.transitions.len()], "dur": "1b"});
                t_i += 1;
            }
        }
        out.push(no, scene);
        let blend = if beat_light { "normal" } else { "screen" };
        if is_drop && bst.confetti {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "confetti", "at": "0.25b", "count": 140,
                "oy": "55%", "colors": [accent, "#ffcf3f", "#ff5d8f", "#34c77b", "#5b8cff"]}));
        }
        if is_drop && bst.shake {
            out.push(
                no,
                json!({"op": "shake", "scene": id, "at": 0, "dur": "1b", "amp": 12}),
            );
        }
        if is_drop && bst.burst {
            out.push(
                no,
                json!({"op": "particles", "scene": id, "kind": "shockwave", "at": 0, "dur": "2b",
                "rings": 3, "width": 10, "colors": [accent, b_ink], "blend": blend}),
            );
            out.push(
                no,
                json!({"op": "particles", "scene": id, "kind": "burst", "at": 0, "count": 90,
                "speed": 1100, "colors": [accent, b_ink], "blend": blend}),
            );
        }
        if is_build && bst.rays {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "rays", "at": format!("{}b", (beats_long * 0.6).round()),
                "dur": format!("{}b", beats_long - (beats_long * 0.6).round()), "count": 150, "speed": 1.5,
                "colors": [accent, b_ink], "blend": if beat_light { "multiply" } else { "screen" }}));
        }

        // What the beat shows: nothing, one thing, or two side by side.
        let shows: Vec<&Value> = match b.get("show") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a.iter().collect(),
            Some(one) => vec![one],
        };
        if shows.len() > 2 {
            return Err(format!(
                "line {no}: a beat shows at most two things side by side, got {}",
                shows.len()
            ));
        }
        let mut kinds = Vec::new();
        for sh in &shows {
            let ks: Vec<&str> = SHOWS
                .iter()
                .copied()
                .filter(|s| sh.get(*s).is_some())
                .collect();
            // A typed prompt may carry chips (the models it is sent to); they are its option, not a second show.
            // Some shows carry another's name as an option: a prompt's chips, a wall's stats.
            let ks: Vec<&str> = if ks.contains(&"type") {
                vec!["type"]
            } else if ks.contains(&"wall") {
                vec!["wall"]
            } else {
                ks
            };
            if ks.len() != 1 {
                return Err(format!(
                    "line {no}: each show holds exactly one of: {} (for example {{\"image\":\"shot.png\"}}); for two things use a list: \"show\":[{{…}},{{…}}]",
                    SHOWS.join(", ")
                ));
            }
            kinds.push(ks[0]);
        }
        // A photo on its own fills the frame, and the headline sits on it.
        let full_photo = shows.len() == 1 && kinds[0] == "photo";

        let fx = bst.entrances[i % bst.entrances.len()];
        let split = if fx == "mask" || fx == "rise" {
            "word"
        } else if SPLIT_WHOLE.contains(&fx) {
            "none"
        } else {
            "char"
        };
        let mut inn = json!({"fx": fx, "at": "0.25b", "dur": "1b", "split": split});
        if split == "word" {
            inn["stagger"] = json!("0.25b");
        }
        let head = |size: f64, y: &str, fixed: bool, color: &str| -> Value {
            let mut t = json!({"op": "text", "id": format!("{id}_t"), "scene": id, "text": text, "size": size,
                "weight": bst.weight, "tracking": bst.tracking, "upper": bst.upper, "italic": bst.italic,
                "leading": 1.1, "y": y, "in": inn, "color": color});
            if fixed {
                t["fixed"] = json!(true);
            }
            // A borrowed style brings its typeface with it.
            if !std::ptr::eq(bst, st) {
                if let Some(f) = bst.font {
                    t["font"] = json!(f);
                }
            }
            if let Some(f) = bst.accent_font.or(st.accent_font) {
                t["accent_font"] = json!(f);
                t["accent_italic"] = json!(bst.accent_italic || st.accent_italic);
            }
            if bst.accent_gradient || st.accent_gradient {
                t["accent_gradient"] = json!([accent, accent2]);
            }
            t
        };
        let sub = b["sub"].as_str();
        if shows.is_empty() || full_photo {
            if full_photo {
                // One photo, or a list that changes on the beat behind the headline.
                let srcs: Vec<String> = match &shows[0]["photo"] {
                    Value::String(s) => vec![s.clone()],
                    Value::Array(a) => a
                        .iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect(),
                    _ => {
                        return Err(format!(
                            "line {no}: show.photo is a file path, or a list of them"
                        ))
                    }
                };
                let every = (beats_long / srcs.len().max(1) as f64).max(0.5);
                for (j, src) in srcs.iter().enumerate() {
                    let t0 = j as f64 * every;
                    out.push(no, json!({"op": "image", "scene": id, "src": src, "w": w, "h": h, "fit": "cover",
                        "scale": 1.12, "keys": [{"at": format!("{t0}b"), "dur": format!("{}b", every + 1.0), "scale": 1.0, "ease": "outCubic"}],
                        "in": {"fx": if j == 0 { "fade" } else { "blur" }, "at": format!("{t0}b"), "dur": "0.5b"}}));
                }
                out.push(
                    no,
                    json!({"op": "shape", "scene": id, "kind": "rect", "w": w, "h": h, "radius": 0,
                    "fill": ["rgba(0,0,0,.05)", "rgba(0,0,0,.45)"], "angle": 180}),
                );
            }
            let color = if full_photo { "#ffffff" } else { b_ink };
            let mut t = head(
                fit_size_em(&text, w, k, 140.0, head_em),
                if sub.is_some() { "44%" } else { "50%" },
                false,
                color,
            );
            t["loop"] = json!({"fx": "glow", "amp": if beat_light || !bst.glow || full_photo { 0 } else { 14 }});
            if full_photo {
                t["shadow"] = json!(true);
            }
            out.push(no, t);
            if let Some(s) = sub {
                out.push(no, json!({"op": "text", "scene": id, "text": s, "size": fit_size(s, w, k, if vertical { 56.0 } else { 44.0 }),
                    "weight": 500, "color": if full_photo { "rgba(255,255,255,.85)" } else { b_muted }, "y": "60%",
                    "in": {"fx": "blur", "at": "2b", "dur": "1b"}}));
            }
            continue;
        }
        // Two layouts that own the whole beat: a lead-in over a word that keeps changing, and a
        // headline over a tilted wall of pictures.
        if shows.len() == 1 && kinds[0] == "cycle" {
            let words = strings(&shows[0]["cycle"], "cycle", no)?;
            if words.is_empty() {
                return Err(format!("line {no}: show.cycle needs at least one word"));
            }
            out.push(
                no,
                head(fit_size_em(&text, w, k, 56.0, head_em), "40%", false, b_ink),
            );
            let longest = words.iter().map(|x| x.chars().count()).max().unwrap_or(1) as f64;
            let size = (w * 0.8 / (longest * 0.55)).min(170.0 * k).round();
            let every = ((beats_long - 1.0) / words.len() as f64).max(0.75);
            for (j, word) in words.iter().enumerate() {
                let t0 = 0.75 + j as f64 * every;
                let last = j + 1 == words.len();
                let mut t = json!({"op": "text", "scene": id, "text": format!("[{word}]"), "size": size,
                    "weight": bst.weight, "tracking": bst.tracking, "y": "55%",
                    "in": {"fx": "blur", "at": format!("{t0}b"), "dur": "0.6b", "split": "none"}});
                if !last {
                    t["out"] = json!({"fx": "blur", "at": format!("{}b", t0 + every - 0.35), "dur": "0.35b", "split": "none"});
                }
                if bst.accent_gradient || st.accent_gradient {
                    t["accent_gradient"] = json!([accent, accent2]);
                }
                if let Some(f) = bst.accent_font.or(st.accent_font) {
                    t["accent_font"] = json!(f);
                    t["accent_italic"] = json!(bst.accent_italic || st.accent_italic);
                }
                out.push(no, t);
            }
            // Pictures that drift at the edges while the word changes.
            if let Some(Value::Array(pics)) = shows[0].get("around") {
                let spots = [
                    (18.0, 22.0, -8.0),
                    (82.0, 24.0, 7.0),
                    (16.0, 78.0, 6.0),
                    (84.0, 76.0, -6.0),
                ];
                for (j, pic) in pics.iter().filter_map(Value::as_str).take(4).enumerate() {
                    let (x, y, r) = spots[j];
                    out.push(no, json!({"op": "image", "scene": id, "src": pic, "w": w * 0.2, "h": w * 0.13,
                        "fit": "cover", "radius": 12, "shadow": true, "x": format!("{x}%"), "y": format!("{y}%"),
                        "rotate": r, "ry": if x < 50.0 { 18 } else { -18 }, "opacity": 0.85,
                        "in": {"fx": "fly", "at": format!("{}b", 0.5 + j as f64 * 0.4), "dur": "1.5b"},
                        "loop": {"fx": "float", "amp": 10, "period": 4.0 + j as f64 * 0.5}}));
                }
            }
            continue;
        }
        if shows.len() == 1 && kinds[0] == "float" {
            let pics = strings(&shows[0]["float"], "float", no)?;
            let mut t = head(
                fit_size_em(&text, w * 0.62, k, 130.0, head_em),
                if sub.is_some() { "46%" } else { "50%" },
                false,
                b_ink,
            );
            t["layer"] = json!(10);
            out.push(no, t);
            if let Some(s) = sub {
                out.push(no, json!({"op": "text", "scene": id, "text": s, "size": fit_size(s, w * 0.6, k, 40.0),
                    "weight": 500, "color": b_muted, "y": "57%", "layer": 10, "in": {"fx": "blur", "at": "1.5b", "dur": "1b"}}));
            }
            let spots = [
                (14.0, 20.0, -6.0),
                (86.0, 18.0, 5.0),
                (12.0, 80.0, 5.0),
                (88.0, 82.0, -5.0),
                (50.0, 12.0, 2.0),
                (50.0, 88.0, -2.0),
            ];
            for (j, pic) in pics.iter().take(6).enumerate() {
                let (x, y, r) = spots[j];
                let from_x = if x < 50.0 {
                    -200.0
                } else if x > 50.0 {
                    200.0
                } else {
                    0.0
                };
                out.push(no, json!({"op": "image", "scene": id, "src": pic, "w": w * 0.17, "h": w * 0.11,
                    "fit": "cover", "radius": 12, "shadow": true, "x": format!("{x}%"), "y": format!("{y}%"), "rotate": r,
                    "in": {"from": {"x": from_x, "y": (y - 50.0) * 4.0, "opacity": 0, "scale": 0.7, "blur": 8}, "at": format!("{}b", 0.25 + j as f64 * 0.3), "dur": "1.5b", "ease": "outExpo"},
                    "loop": {"fx": "float", "amp": 8, "period": 3.5 + j as f64 * 0.4}}));
            }
            continue;
        }
        if shows.len() == 1 && kinds[0] == "wall" {
            let pics = strings(&shows[0]["wall"], "wall", no)?;
            let cols = 4usize;
            let cw = w * 0.26;
            let ch = cw * 0.64;
            out.push(no, json!({"op": "group", "id": format!("{id}_w"), "scene": id, "y": "58%", "rotate": -12, "rx": 32,
                "layout": {"kind": "grid", "cols": cols, "gap": 28.0 * k, "cell": [cw, ch]},
                "cascade": {"fx": "fly", "at": 0, "stagger": "0.08b", "dur": "1.5b", "order": "center"},
                "loop": {"fx": "drift", "vy": -40},
                "opacity": match (beat_light, shows[0].get("stats").is_some()) { (true, false) => 0.9, (true, true) => 0.4, (false, _) => 0.5 }}));
            let n_tiles = pics.len().max(1).div_ceil(cols) * cols;
            for j in 0..n_tiles.max(8) {
                let pic = &pics[j % pics.len().max(1)];
                out.push(
                    no,
                    json!({"op": "image", "parent": format!("{id}_w"), "src": pic, "w": cw, "h": ch,
                    "fit": "cover", "radius": 12, "shadow": true}),
                );
            }
            out.push(no, json!({"op": "shape", "scene": id, "fixed": true, "kind": "glow", "d": w * 0.8,
                "color": ground(bst, st, beat_light), "intensity": 1, "blend": "normal", "y": "48%"}));
            // Big numbers over the wall, the headline above them.
            if let Some(Value::Array(stats)) = shows[0].get("stats") {
                out.push(
                    no,
                    head(fit_size_em(&text, w, k, 64.0, head_em), "32%", true, b_ink),
                );
                out.push(no, json!({"op": "group", "id": format!("{id}_ws"), "scene": id, "fixed": true, "y": "54%",
                    "layout": {"kind": "row", "gap": 140.0 * k}, "cascade": {"fx": "rise", "at": "0.75b", "stagger": "0.35b"}}));
                for (j, st2) in stats.iter().enumerate() {
                    let (big, label) = match st2 {
                        Value::Array(a) => (
                            a.first().and_then(Value::as_str).unwrap_or("0"),
                            a.get(1).and_then(Value::as_str).unwrap_or(""),
                        ),
                        Value::String(x) => (x.as_str(), ""),
                        _ => return Err(format!("line {no}: each stat is [\"value\", \"label\"]")),
                    };
                    let (prefix, value, suffix, decimals) = parse_stat(big);
                    let sid = format!("{id}_ws{j}");
                    out.push(no, json!({"op": "ui", "id": sid, "parent": format!("{id}_ws"), "kind": "stat", "value": 0,
                        "prefix": prefix, "suffix": suffix, "decimals": decimals, "label": label,
                        "size": (150.0 * k).round(), "w": (w * 0.3).round(), "color": b_ink}));
                    out.push(
                        no,
                        json!({"op": "act", "target": sid, "do": "count", "to": value,
                        "at": format!("{}b", 1.0 + j as f64 * 0.35), "dur": "2.5b"}),
                    );
                }
                continue;
            }
            let mut t = head(
                fit_size_em(&text, w, k, 120.0, head_em),
                if sub.is_some() { "44%" } else { "48%" },
                true,
                b_ink,
            );
            t["shadow"] = json!(true);
            out.push(no, t);
            if let Some(s) = sub {
                out.push(no, json!({"op": "text", "scene": id, "fixed": true, "text": s, "size": fit_size(s, w, k, 36.0),
                    "weight": 500, "color": b_muted, "y": "57%", "in": {"fx": "fade", "at": "1.5b"}}));
            }
            continue;
        }
        let head_size = fit_size_em(&text, w, k, if vertical { 120.0 } else { 84.0 }, head_em);
        // On a phone frame the headline hangs from the top, however many lines it wraps to, and
        // the line under it and the show follow it down.
        let head_h = head_size * 1.1 * text.split('\n').count() as f64 / h * 100.0;
        let head_y = if vertical {
            (7.0 + head_h / 2.0).max(12.0)
        } else {
            15.0
        };
        out.push(no, head(head_size, &format!("{head_y:.1}%"), true, b_ink));
        let mut body_y = if vertical {
            52.0_f64.max(head_y + head_h / 2.0 + 30.0)
        } else {
            58.0
        };
        // A two-line headline takes more of the top, so what it shows sits lower.
        if text.contains('\n') && !vertical {
            body_y += 4.0;
        }
        if let Some(s) = sub {
            let sub_y = if vertical {
                head_y + head_h / 2.0 + 3.5
            } else {
                24.0
            };
            out.push(no, json!({"op": "text", "scene": id, "fixed": true, "text": s, "size": fit_size(s, w, k, if vertical { 40.0 } else { 30.0 }),
                "weight": 500, "color": b_muted, "y": format!("{sub_y:.1}%"),
                "in": {"fx": "fade", "at": "1b"}}));
            body_y += 2.0;
        }
        for (j, sh) in shows.iter().enumerate() {
            let two = shows.len() == 2;
            let (cx, span, y) = match (two, vertical) {
                (false, _) => (0.5, 1.0, body_y),
                (true, false) => (if j == 0 { 0.27 } else { 0.73 }, 0.46, body_y),
                (true, true) => (0.5, 1.0, if j == 0 { 40.0 } else { 72.0 }),
            };
            let slot = Slot {
                no,
                id: if two { format!("{id}_{j}") } else { id.clone() },
                scene: id.clone(),
                x: format!("{}%", cx * 100.0),
                y: format!("{y}%"),
                span: w * span,
                w,
                h,
                k,
                vertical,
                beats_long,
                accent: accent.clone(),
                ink: b_ink,
                muted: b_muted,
                url: url.clone(),
                delay: if two && j == 1 { 0.5 } else { 0.0 },
                compact: two,
                ground: ground(bst, st, beat_light),
                accent2: accent2.clone(),
                gradient: bst.accent_gradient || st.accent_gradient,
            };
            build_show(&mut out, &slot, kinds[j], sh)?;
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
        if st.sparkles {
            // A field of sparkles, dense at the foot of the frame, twinkling in the brand colours.
            out.push(no, json!({"op": "particles", "scene": id, "kind": "stars", "count": 260, "vx": 6, "size": 1.6,
                "colors": [accent, accent2, "#ffffff"], "y": "80%", "h": h * 0.4, "in": {"fx": "fade", "at": "0.5b", "dur": "2b"}}));
            out.push(no, json!({"op": "particles", "scene": id, "kind": "dust", "count": 40, "colors": [accent, accent2]}));
        } else if !light {
            out.push(no, json!({"op": "particles", "scene": id, "kind": "dust", "count": 45, "colors": [accent, "#ffffff"]}));
        }
        let head_em = 0.53 + if st.upper { 0.16 } else { 0.0 } + st.tracking.max(0.0);
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
            let mut tl = json!({"op": "text", "scene": id, "text": t, "size": fit_size(t, w, k, if vertical { 60.0 } else { 44.0 }), "weight": 600,
                "tracking": -0.02, "y": "61%", "in": {"fx": "blur", "at": "2.5b", "dur": "1b"}});
            if let Some(f) = st.accent_font {
                tl["accent_font"] = json!(f);
                tl["accent_italic"] = json!(st.accent_italic);
            }
            if st.accent_gradient {
                tl["accent_gradient"] = json!([accent, accent2]);
            }
            out.push(no, tl);
        }
        // A row of links above the lockup, like the site's own navigation.
        if let Some(Value::Array(links)) = e.get("links") {
            out.push(no, json!({"op": "group", "id": "end_links", "scene": id, "y": "27%",
                "layout": {"kind": "row", "gap": 44.0 * k}, "cascade": {"fx": "fade", "at": "3b", "stagger": "0.15b"}}));
            for l in links.iter().filter_map(Value::as_str) {
                out.push(no, json!({"op": "text", "parent": "end_links", "text": l, "size": (if vertical { 30.0 } else { 20.0 } * k).round(),
                    "weight": 500, "color": muted}));
            }
        }
        if let Some(u) = e["url"].as_str().map(str::to_string).or(url) {
            out.push(no, json!({"op": "ui", "scene": id, "kind": "button", "label": u, "size": (if vertical { 34.0 } else { 24.0 } * k).round(),
                "y": "73%", "in": {"fx": "pop", "at": "4b", "sfx": "pop"}, "shine": true,
                "loop": {"fx": "shine", "period": "4b", "phase": "5b"}}));
        }
    }
    Ok(out.ops)
}

/// Entrances that animate a headline as one block rather than letter by letter.
const SPLIT_WHOLE: &[&str] = &["dots", "fade", "zoom", "fly"];

/// Where one show goes: its scene, its centre, how much width it has, and the look around it.
struct Slot {
    no: usize,
    /// A prefix for the ids this show creates.
    id: String,
    scene: String,
    x: String,
    y: String,
    span: f64,
    w: f64,
    h: f64,
    k: f64,
    vertical: bool,
    beats_long: f64,
    accent: String,
    ink: &'static str,
    muted: &'static str,
    url: Option<String>,
    /// Beats to wait, so the second of two shows lands after the first.
    delay: f64,
    /// Half the frame: smaller type, fewer columns.
    compact: bool,
    /// The colour behind this show, for fades that have to match it.
    ground: String,
    accent2: String,
    /// Paint big numbers with the accent gradient.
    gradient: bool,
}

impl Slot {
    fn at(&self, beats: f64) -> String {
        format!("{}b", beats + self.delay)
    }
}

fn strings(v: &Value, what: &str, no: usize) -> Result<Vec<String>, String> {
    Ok(v.as_array()
        .ok_or_else(|| format!("line {no}: show.{what} is a list"))?
        .iter()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect())
}

fn build_show(out: &mut Out, sl: &Slot, kind: &str, show: &Value) -> Result<(), String> {
    let no = sl.no;
    let id = &sl.id;
    let scene = &sl.scene;
    let k = sl.k;
    let span = sl.span;
    let beats_long = sl.beats_long;
    let end_b = format!("{}b", (beats_long - 1.5).max(1.0));
    let scale_c = if sl.compact { 0.72 } else { 1.0 };
    match kind {
        "image" | "photo" => {
            let src = show[kind]
                .as_str()
                .ok_or_else(|| format!("line {no}: show.{kind} is a file path"))?;
            let fw = if sl.vertical {
                span * 0.86
            } else if sl.compact {
                span * 0.92
            } else {
                span * 0.6
            };
            let fh = show["height"]
                .as_f64()
                .map(|x| x * k)
                .unwrap_or(fw * if kind == "photo" { 0.66 } else { 0.62 });
            let frame = if kind == "photo" {
                "none"
            } else {
                show["frame"].as_str().unwrap_or("browser")
            };
            // pan: the camera leans in on the picture and travels across it, left to right.
            if show["pan"].as_bool().unwrap_or(false) && !sl.compact {
                let dx = fw * 0.22;
                out.push(no, json!({"op": "camera", "scene": scene, "keys": [
                    {"t": sl.at(1.5), "zoom": 1.0, "x": 0},
                    {"t": sl.at(2.5), "zoom": 1.8, "x": -dx, "y": -fh * 0.1, "ease": "inOutExpo"},
                    {"t": format!("{}b", beats_long - 1.25), "zoom": 1.8, "x": dx, "y": fh * 0.12, "ease": "inOutCubic"},
                    {"t": format!("{}b", beats_long - 0.25), "zoom": 1.1, "x": 0, "y": 0, "ease": "inOutExpo"}]}));
            }
            if frame == "none" {
                out.push(no, json!({"op": "image", "id": format!("{id}_s"), "scene": scene, "src": src, "w": fw, "h": fh,
                    "fit": "cover", "radius": 14, "shadow": true, "x": sl.x, "y": sl.y, "rx": 24, "scale": 0.9,
                    "keys": [{"t": sl.at(2.0), "rx": 0, "scale": 1, "ease": "outExpo"}],
                    "in": {"fx": "fly", "at": sl.at(0.0), "dur": "1.5b"}, "loop": {"fx": "float", "amp": 6, "period": 4}}));
            } else {
                let mut win = json!({"op": "ui", "id": format!("{id}_s"), "scene": scene, "kind": "window",
                    "w": fw, "h": fh + 38.0, "x": sl.x, "y": sl.y, "rx": 24, "scale": 0.9,
                    "keys": [{"t": sl.at(2.0), "rx": 0, "scale": 1, "ease": "outExpo"}],
                    "in": {"fx": "fly", "at": sl.at(0.0), "dur": "1.5b"}, "loop": {"fx": "float", "amp": 6, "period": 4}});
                win["url"] = json!(show["url"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| sl.url.clone())
                    .unwrap_or_default());
                out.push(no, win);
                out.push(
                    no,
                    json!({"op": "image", "parent": format!("{id}_s"), "src": src, "w": fw, "h": fh,
                    "fit": "cover", "anchor": [0, 0], "x": 0, "y": 0}),
                );
            }
        }
        "code" => {
            let code = show["code"]
                .as_str()
                .ok_or_else(|| format!("line {no}: show.code is the code, as text"))?;
            // On a phone frame the code is set as large as its longest line allows.
            let code_size = if sl.vertical {
                let longest = code
                    .lines()
                    .map(|l| l.chars().count())
                    .max()
                    .unwrap_or(1)
                    .max(1) as f64;
                (span * 0.9 * 0.86 / (longest * 0.62)).clamp(23.0 * k, 34.0 * k)
            } else {
                23.0 * k * scale_c
            };
            let cw = if sl.vertical {
                span * 0.9
            } else if sl.compact {
                span * 0.94
            } else {
                span * 0.62
            };
            out.push(no, json!({"op": "ui", "id": format!("{id}_s"), "scene": scene, "kind": "code", "code": code,
                "title": show["title"].as_str().unwrap_or("terminal"), "lang": show["lang"].as_str().unwrap_or("sh"),
                "size": code_size.round(), "w": cw, "x": sl.x, "y": sl.y, "ry": if sl.vertical || sl.compact { 0 } else { -10 },
                "keys": [{"t": end_b, "ry": 0}], "in": {"fx": "rise", "at": sl.at(0.0), "dur": "1b"}}));
            out.push(
                no,
                json!({"op": "act", "target": format!("{id}_s"), "do": "type", "at": sl.at(0.5),
                "dur": format!("{}b", (beats_long - 2.5 - sl.delay).max(1.0))}),
            );
        }
        "type" => {
            let typed = show["type"]
                .as_str()
                .ok_or_else(|| format!("line {no}: show.type is the text to type"))?;
            let iw = if sl.vertical {
                span * 0.7
            } else if sl.compact {
                span * 0.62
            } else {
                span * 0.42
            };
            let mut input = json!({"op": "ui", "id": format!("{id}_s"), "scene": scene, "kind": "input", "w": iw,
                "x": sl.x, "y": sl.y, "scale": 1.7 * k * scale_c, "in": {"fx": "rise", "at": sl.at(0.0), "dur": "1b"},
                "placeholder": show["placeholder"].as_str().unwrap_or("Ask anything…")});
            if let Some(c) = show.get("chips") {
                input["chips"] = c.clone();
            }
            out.push(no, input);
            let n_chars = typed.chars().count() as f64;
            let room = ((beats_long - 3.0 - sl.delay) * 0.5).max(0.8);
            out.push(
                no,
                json!({"op": "act", "target": format!("{id}_s"), "do": "type", "at": sl.at(1.0),
                "text": typed, "cps": (n_chars / room).max(12.0)}),
            );
            out.push(no, json!({"op": "act", "target": format!("{id}_s"), "do": "click", "sel": ".kv-send", "at": end_b}));
        }
        "list" => {
            let items = strings(&show["list"], "list", no)?;
            let rows: Vec<Value> = items
                .iter()
                .map(|l| json!({"label": l, "check": false}))
                .collect();
            out.push(no, json!({"op": "ui", "id": format!("{id}_s"), "scene": scene, "kind": "list", "items": rows,
                "w": if sl.vertical { span * 0.38 } else if sl.compact { span * 0.5 } else { span * 0.26 },
                "x": sl.x, "y": sl.y, "scale": 2.1 * k * scale_c, "in": {"fx": "rise", "at": sl.at(0.0), "dur": "1b"}}));
            for (j, _) in items.iter().enumerate() {
                out.push(no, json!({"op": "act", "target": format!("{id}_s"), "do": "check", "index": j,
                    "at": sl.at(1.5 + j as f64 * ((beats_long - 3.0) / items.len().max(1) as f64).min(1.0))}));
            }
        }
        "stats" => {
            let stats = show["stats"].as_array().ok_or_else(|| {
                format!(
                    "line {no}: show.stats is a list of [\"20,641+\", \"happy customers\"] pairs"
                )
            })?;
            let column = sl.vertical || sl.compact;
            out.push(no, json!({"op": "group", "id": format!("{id}_s"), "scene": scene, "x": sl.x, "y": sl.y,
                "layout": {"kind": if column { "column" } else { "row" }, "gap": if column { 30.0 * k } else { 110.0 * k }},
                "cascade": {"fx": "rise", "at": sl.at(0.5), "stagger": "0.35b"}}));
            for (j, st) in stats.iter().enumerate() {
                let (big, label) = match st {
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
                    "size": (104.0 * k * if column && sl.compact { 0.75 } else { 1.0 }).round(),
                    "w": (if column { span * 0.8 } else { span * 0.8 / stats.len().max(1) as f64 }).round(),
                    "color": if j == 0 { sl.accent.clone() } else { sl.ink.to_string() }}));
                out.push(
                    no,
                    json!({"op": "act", "target": sid, "do": "count", "to": value,
                    "at": sl.at(0.8 + j as f64 * 0.35), "dur": "2.5b"}),
                );
            }
        }
        "icons" => {
            let names = show["icons"]
                .as_array()
                .ok_or_else(|| format!("line {no}: show.icons is a list of icon names"))?;
            let r = if sl.vertical {
                span * 0.38
            } else if sl.compact {
                span * 0.36
            } else {
                span * 0.27
            };
            out.push(no, json!({"op": "group", "id": format!("{id}_s"), "scene": scene, "x": sl.x, "y": sl.y,
                "layout": {"kind": "orbit", "rx": r, "ry": r * 0.3, "period": 7, "depth": 0.45, "tilt": -6},
                "trail": {"len": 0.5, "width": 5, "color": sl.accent, "opacity": 0.6},
                "cascade": {"fx": "pop", "at": sl.at(0.25), "stagger": "0.2b"}}));
            for nm in names.iter().filter_map(Value::as_str) {
                if crate::icons::art(nm).is_some() {
                    out.push(no, json!({"op": "icon", "parent": format!("{id}_s"), "name": nm, "size": (100.0 * k * scale_c).round()}));
                } else {
                    let glyph: String = nm.chars().take(1).collect::<String>().to_uppercase();
                    out.push(no, json!({"op": "ui", "parent": format!("{id}_s"), "kind": "tile", "glyph": glyph,
                        "size": (96.0 * k * scale_c).round(), "bg": [sl.accent, mix(&sl.accent, (0.0, 0.0, 0.0), 0.35)]}));
                }
            }
        }
        "chips" => {
            let chips = strings(&show["chips"], "chips", no)?;
            let column = sl.vertical || sl.compact;
            out.push(
                no,
                json!({"op": "group", "id": format!("{id}_s"), "scene": scene, "x": sl.x, "y": sl.y,
                "layout": {"kind": if column { "column" } else { "row" }, "gap": 22.0 * k},
                "cascade": {"fx": "pop", "at": sl.at(1.0), "stagger": "0.5b", "sfx": "pop"}}),
            );
            for c in chips {
                out.push(
                    no,
                    json!({"op": "ui", "parent": format!("{id}_s"), "kind": "chip", "label": c,
                    "icon": "✓", "color": sl.accent, "size": (40.0 * k * scale_c).round()}),
                );
            }
        }
        "strike" => {
            let items = strings(&show["strike"], "strike", no)?;
            out.push(no, json!({"op": "group", "id": format!("{id}_s"), "scene": scene, "x": sl.x, "y": sl.y,
                "layout": {"kind": "column", "gap": 20.0 * k}, "cascade": {"fx": "rise", "at": sl.at(0.5), "stagger": "0.4b"}}));
            for (j, s) in items.iter().enumerate() {
                let sid = format!("{id}_x{j}");
                out.push(no, json!({"op": "text", "id": sid, "parent": format!("{id}_s"), "text": format!("~{s}~"),
                    "size": fit_size(s, span, k, if sl.vertical { 72.0 } else { 56.0 } * scale_c), "weight": 500, "color": sl.muted, "strike_color": sl.accent}));
                out.push(no, json!({"op": "act", "target": sid, "do": "strike", "at": sl.at(2.5 + j as f64 * 0.5), "dur": "0.4b"}));
                out.push(no, json!({"op": "sfx", "kind": "tick", "scene": scene, "at": sl.at(2.5 + j as f64 * 0.5), "pitch": 1.0 + j as f64 * 0.2}));
            }
        }
        // A number too big for the frame's comfort, counting up to where it lands.
        "number" => {
            let big = show["number"]
                .as_str()
                .ok_or_else(|| format!("line {no}: show.number is text like \"$400M\""))?;
            let (prefix, value, suffix, decimals) = parse_stat(big);
            let chars = big.chars().count().max(1) as f64;
            // Heavy figures run wider than text: about 0.66em each.
            let size = (span * 0.86 / (chars * 0.66)).min(sl.h * 0.42).round();
            let from = show["from"].as_f64().unwrap_or(value * 0.6);
            let pad = show["pad"].as_bool().unwrap_or(false);
            let from = if pad {
                show["from"].as_f64().unwrap_or(0.0)
            } else {
                from
            };
            let mut stat = json!({"op": "ui", "id": format!("{id}_s"), "scene": scene, "kind": "stat", "value": from,
                "prefix": prefix, "suffix": suffix, "decimals": decimals, "size": size, "x": sl.x, "y": sl.y,
                "color": sl.ink, "w": span * 0.95, "style": "letter-spacing:-0.04em", "pad": pad,
                "in": {"fx": "blur", "at": sl.at(0.0), "dur": "1b"}});
            if sl.gradient {
                stat["gradient"] = json!([sl.accent, sl.accent2]);
            }
            out.push(no, stat);
            // A line that climbs across the frame as the number does, lit at its tip.
            if show["graph"].as_bool().unwrap_or(false) {
                let gw = sl.w;
                let gh = sl.h * 0.5;
                let mut d = String::from("M0 ");
                d.push_str(&format!("{:.0}", gh));
                for step in 1..=40 {
                    let x = step as f64 / 40.0;
                    let y = gh * (1.0 - (x.powf(2.6) * 0.92 + 0.02 * (x * 19.0).sin().abs()));
                    d.push_str(&format!(" L{:.0} {:.0}", x * gw, y));
                }
                out.push(no, json!({"op": "shape", "scene": scene, "kind": "path", "d": d, "viewBox": format!("0 0 {gw:.0} {gh:.0}"),
                    "w": gw, "h": gh, "stroke": sl.accent, "stroke_width": 3, "y": "75%", "layer": -1, "glow": 10, "glow_color": sl.accent,
                    "in": {"fx": "draw", "at": sl.at(0.25), "dur": format!("{}b", (beats_long * 0.55).max(2.0)), "ease": "outExpo"}}));
            }
            out.push(no, json!({"op": "act", "target": format!("{id}_s"), "do": "count", "to": value, "from": from,
                "at": sl.at(0.25), "dur": format!("{}b", (beats_long * 0.55).max(2.0)), "ease": "outExpo"}));
            if let Some(label) = show["label"].as_str() {
                out.push(no, json!({"op": "ui", "scene": scene, "kind": "chip", "label": label, "icon": "●", "color": sl.accent,
                    "size": (32.0 * k).round(), "x": sl.x, "y": offset(&sl.y, size * 0.66),
                    "in": {"fx": "rise", "at": sl.at(2.0)}}));
            }
        }
        // A drum of lines rolling past a centre line, one per beat, like a slot machine settling.
        "roll" => {
            let items = strings(&show["roll"], "roll", no)?;
            if items.is_empty() {
                return Err(format!("line {no}: show.roll needs at least one line"));
            }
            let longest = items.iter().map(|s| s.chars().count()).max().unwrap_or(1) as f64;
            let size = (span * 0.9 / (longest * 0.55))
                .clamp(40.0 * k, 110.0 * k * scale_c)
                .round();
            let gap = size * 0.28;
            let row = size * 1.1 + gap;
            let n = items.len() as f64;
            let gid = format!("{id}_s");
            // The column starts centred on its first line and steps up one line a beat.
            let first = (n - 1.0) / 2.0 * row;
            let mut keys = Vec::new();
            let steps = (n - 1.0).max(0.0) as usize;
            let every = ((beats_long - 2.0) / steps.max(1) as f64).clamp(0.5, 1.0);
            for step in 0..=steps {
                let t = 0.75 + step as f64 * every;
                keys.push(json!({"t": format!("{}b", t + sl.delay), "y": offset(&sl.y, first - step as f64 * row), "ease": "snap"}));
            }
            out.push(
                no,
                json!({"op": "group", "id": gid, "scene": scene, "x": sl.x,
                "y": offset(&sl.y, first),
                "layout": {"kind": "column", "gap": gap}, "keys": keys,
                "cascade": {"fx": "rise", "at": sl.at(0.0), "stagger": "0.08b"}}),
            );
            for (j, line) in items.iter().enumerate() {
                let sid = format!("{id}_r{j}");
                out.push(no, json!({"op": "text", "id": sid, "parent": gid, "text": line, "size": size, "weight": 600,
                    "tracking": -0.03, "color": sl.ink}));
                // Each line is dim except while it sits on the centre line.
                let on = 0.75 + j as f64 * every;
                out.push(no, json!({"op": "anim", "target": sid, "keys": [
                    {"t": "0b", "opacity": if j == 0 { 1.0 } else { 0.22 }},
                    {"t": format!("{}b", on + sl.delay - 0.01), "opacity": if j == 0 { 1.0 } else { 0.22 }},
                    {"t": format!("{}b", on + sl.delay + 0.3), "opacity": 1.0},
                    {"t": format!("{}b", on + every + sl.delay), "opacity": if j + 1 == items.len() { 1.0 } else { 0.22 }}]}));
            }
            if !sl.compact {
                // Soft fades top and bottom, so the drum reads as a window onto a longer list.
                let (r, g, b0) = hex(&sl.ground).unwrap_or((11.0, 11.0, 14.0));
                let ground = format!("rgba({},{},{}", r as u8, g as u8, b0 as u8);
                // Solid over the headline, fading towards the centre line.
                for (y0, a, m, b) in [("19%", 1.0, 1.0, 0.0), ("90%", 0.0, 1.0, 1.0)] {
                    out.push(no, json!({"op": "shape", "scene": scene, "kind": "rect", "w": sl.w, "h": sl.h * 0.38, "radius": 0,
                        "fill": [format!("{ground},{a})"), format!("{ground},{m})"), format!("{ground},{m})"), format!("{ground},{b})")], "angle": 180, "y": y0, "layer": 5}));
                }
            }
        }
        // A recording, kaviri's own or any video, shown frame by frame.
        "clip" => {
            let src = show["clip"]
                .as_str()
                .ok_or_else(|| format!("line {no}: show.clip is a video file"))?;
            let ch = sl.h * if sl.compact { 0.56 } else { 0.62 };
            let mut c = json!({"op": "clip", "id": format!("{id}_s"), "scene": scene, "src": src, "h": ch,
                "max_w": span * if sl.compact { 0.94 } else { 0.8 }, "start": sl.at(0.0),
                "fill": format!("{}b", beats_long - sl.delay - 0.5),
                "x": sl.x, "y": sl.y, "radius": 18, "shadow": true, "rx": 18, "scale": 0.92,
                "keys": [{"t": sl.at(2.0), "rx": 0, "scale": 1, "ease": "outExpo"}],
                "in": {"fx": "fly", "at": sl.at(0.0), "dur": "1.5b"}, "loop": {"fx": "float", "amp": 5, "period": 4}});
            if let Some(l) = show["label"].as_str() {
                c["label"] = json!(l);
            }
            out.push(no, c);
        }
        // Several recordings side by side: the same take on every device.
        "devices" => {
            let srcs = strings(&show["devices"], "devices", no)?;
            let labels: Vec<String> = show
                .get("labels")
                .map(|l| strings(l, "labels", no))
                .transpose()?
                .unwrap_or_default();
            let gid = format!("{id}_s");
            out.push(
                no,
                json!({"op": "group", "id": gid, "scene": scene, "x": sl.x, "y": sl.y,
                "layout": {"kind": "row", "gap": 44.0 * k}}),
            );
            let ch = sl.h * if sl.compact { 0.5 } else { 0.58 };
            let n = srcs.len().max(1) as f64;
            // Side by side on a phone frame, the devices share its width.
            let max_w = if sl.vertical {
                (span * 0.92 - 44.0 * k * (n - 1.0)) / n
            } else {
                span * 0.9 / n * 1.6
            };
            for (j, src) in srcs.iter().enumerate() {
                let t0 = 0.25 + j as f64 * 0.5 + sl.delay;
                let mut c = json!({"op": "clip", "id": format!("{id}_d{j}"), "parent": gid, "src": src, "h": ch,
                    "max_w": max_w, "start": format!("{t0}b"),
                    "fill": format!("{}b", beats_long - t0 - 0.25),
                    "radius": 16, "shadow": true,
                    "in": {"fx": "fly", "at": sl.at(0.25 + j as f64 * 0.5), "dur": "1.5b"},
                    "loop": {"fx": "float", "amp": 6, "period": 3.6 + j as f64 * 0.4}});
                if let Some(l) = labels.get(j) {
                    c["label"] = json!(l);
                }
                out.push(no, c);
            }
        }
        "cycle" | "wall" | "float" => {
            return Err(format!(
                "line {no}: {kind} takes the whole beat; give it its own beat rather than a pair"
            ));
        }
        // Photo cards stacked and fanned, rising into place one after another.
        "stack" => {
            let srcs = strings(&show["stack"], "stack", no)?;
            let cw = if sl.compact { span * 0.5 } else { span * 0.3 };
            let ch = cw * 0.66;
            let gid = format!("{id}_s");
            out.push(
                no,
                json!({"op": "group", "id": gid, "scene": scene, "x": sl.x, "y": sl.y}),
            );
            let n = srcs.len() as f64;
            for (j, src) in srcs.iter().enumerate() {
                let o = j as f64 - (n - 1.0) / 2.0;
                out.push(no, json!({"op": "image", "parent": gid, "src": src, "w": cw, "h": ch, "fit": "cover",
                    "radius": 14, "shadow": true, "x": o * cw * 0.16, "y": o * ch * 0.12, "rotate": o * 4.0,
                    "in": {"fx": "rise", "at": sl.at(0.3 + j as f64 * 0.35), "dur": "1b"},
                    "loop": {"fx": "float", "amp": 5, "period": 3.5 + j as f64 * 0.4}}));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// A percentage position moved by some pixels, in the form the runtime reads: "58%+120".
fn offset(pct: &str, px: f64) -> String {
    format!("{pct}{:+}", px.round())
}

/// The ground a beat sits on when it is not the brand's: cream or near black.
fn ground(bst: &Style, st: &Style, light: bool) -> String {
    let pick = |s: &Style| {
        if light {
            s.paper_light.or(if s.light { s.paper } else { None })
        } else {
            s.paper_dark.or(if s.light { None } else { s.paper })
        }
    };
    pick(bst)
        .or_else(|| pick(st))
        .unwrap_or(if light { "#f7f7f5" } else { "#0b0b0e" })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_headlines_for_phones() {
        assert_eq!(wrap_short("Stop [doing that.]"), "Stop [doing]\n[that.]");
        assert_eq!(wrap_short("Short line"), "Short line");
        assert_eq!(
            wrap_short("Still re-recording your [demo video?]"),
            "Still re-recording\nyour [demo video?]"
        );
    }

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
    fn beats_mix_two_shows_and_their_own_style_and_ground() {
        let ops = run(r##"{"op":"brand","name":"a","style":"bold"}
{"op":"beat","text":"x","style":"terminal","theme":"light","show":[{"roll":["a","b"]},{"number":"$4.2M"}]}
{"op":"end"}"##)
        .unwrap();
        let t = ops.iter().find(|o| o["id"] == "beat1_t").unwrap();
        assert!(t["font"].as_str().unwrap().contains("Mono"), "{t}");
        let scene = ops
            .iter()
            .find(|o| o["op"] == "scene" && o["id"] == "beat1")
            .unwrap();
        assert!(
            scene["bg"].is_string(),
            "a light beat in a dark video gets its own ground"
        );
        assert!(ops.iter().any(|o| o["id"] == "beat1_0_s"));
        assert!(ops
            .iter()
            .any(|o| o["id"] == "beat1_1_s" && o["kind"] == "stat"));
        let e = run(r##"{"op":"beat","text":"x","show":[{"chips":["a"]},{"chips":["b"]},{"chips":["c"]}]}"##)
            .err()
            .unwrap();
        assert!(e.contains("at most two"), "{e}");
    }

    #[test]
    fn announcement_takes_turns_on_the_two_grounds() {
        let ops = run(r##"{"op":"brand","name":"a","style":"announcement"}
{"op":"beat","text":"one"}
{"op":"beat","text":"two"}
{"op":"beat","text":"three"}"##)
        .unwrap();
        let scenes: Vec<&Value> = ops.iter().filter(|o| o["op"] == "scene").collect();
        assert!(scenes[0]["bg"].is_null() && scenes[2]["bg"].is_null());
        assert_eq!(scenes[1]["bg"], "#06170f");
        let t = ops.iter().find(|o| o["id"] == "beat1_t").unwrap();
        assert_eq!(t["accent_italic"], true);
    }

    #[test]
    fn the_newer_shows_expand() {
        let ops = run(
            r##"{"op":"brand","name":"a","style":"launch","intro":"logo"}
{"op":"beat","text":"x","show":{"cycle":["one.","two."],"around":["p.png"]}}
{"op":"beat","text":"y","show":{"wall":["p.png"],"stats":[["197","things"]]}}
{"op":"beat","text":"z","show":{"number":"1,000,000","pad":true,"graph":true}}
{"op":"beat","text":"w","show":{"float":["p.png","q.png"]}}
{"op":"beat","text":"v","show":{"devices":["a.mp4","b.mp4"],"labels":["iOS","Android"]}}
{"op":"end","links":["Docs"]}"##,
        )
        .unwrap();
        let has = |f: &dyn Fn(&Value) -> bool| ops.iter().any(f);
        assert!(has(&|o| o["id"] == "intro" && o["op"] == "scene"));
        assert!(has(&|o| o["accent_gradient"].is_array()));
        assert!(has(&|o| o["kind"] == "stat" && o["pad"] == true));
        assert!(has(&|o| o["kind"] == "path"));
        assert!(has(&|o| o["op"] == "clip" && o["label"] == "Android"));
        assert!(has(&|o| o["id"] == "end_links"));
        let music = ops.iter().find(|o| o["op"] == "music").unwrap();
        assert_eq!(music["sections"][0]["part"], "intro");
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
