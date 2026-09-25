//! Device frames: the take filmed as if it were on a Mac, a Windows or Linux
//! desktop, an Android phone, an iPhone, or either phone's emulator.
//!
//! A frame is two things. The *layout* says where the content lands and what
//! silhouette the backdrop plate cuts its window from, so the handset or window
//! casts the shadow rather than the bare content. The *chrome* is everything
//! drawn on top: title bars, tab strips, status bars, bezels, a menu bar and a
//! dock. The chrome is HTML, rendered once by the same headless browser that
//! filmed the take, into a PNG that is transparent wherever the content or the
//! wallpaper should show. That buys real fonts, real favicons and crisp
//! vector icons at any output size, and costs one screenshot per take.
//!
//! Every size in the chrome is written in the units of the thing it imitates:
//! CSS pixels of the page for a desktop window, points of a reference handset
//! (393 wide for iOS, 412 for Android) for a phone, points of a 1470 wide
//! screen for a menu bar or dock. CSS `zoom` carries each one to video pixels,
//! which keeps text sharp where a transform would scale a bitmap.

use crate::backdrop::Layout;
use base64::Engine;
use serde_json::Value;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ the spec

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Os {
    Macos,
    Windows,
    Linux,
    Android,
    Ios,
    AndroidEmulator,
    IosSimulator,
}

pub const FRAMES: &[(&str, Os, &str)] = &[
    (
        "macos",
        Os::Macos,
        "a macOS window: traffic lights, a browser or an app title bar",
    ),
    (
        "windows",
        Os::Windows,
        "a Windows 11 window: minimise, maximise and close on the right",
    ),
    (
        "linux",
        Os::Linux,
        "a GNOME window: a header bar with round buttons",
    ),
    (
        "android",
        Os::Android,
        "an Android handset: punch-hole camera, status bar, gesture bar",
    ),
    (
        "ios",
        Os::Ios,
        "an iPhone: Dynamic Island, status bar, home indicator",
    ),
    (
        "android-emulator",
        Os::AndroidEmulator,
        "the Android Emulator: the handset with its side toolbar",
    ),
    (
        "ios-simulator",
        Os::IosSimulator,
        "the iOS Simulator: the handset under a device title bar",
    ),
];

impl Os {
    pub fn parse(s: &str) -> Option<Os> {
        let s = s.to_ascii_lowercase();
        let alias = match s.as_str() {
            "mac" | "osx" => "macos",
            "win" | "win11" => "windows",
            "gnome" | "ubuntu" => "linux",
            "iphone" => "ios",
            "emulator" => "android-emulator",
            "simulator" => "ios-simulator",
            other => other,
        };
        FRAMES.iter().find(|f| f.0 == alias).map(|f| f.1)
    }

    pub fn name(self) -> &'static str {
        FRAMES
            .iter()
            .find(|f| f.1 == self)
            .map(|f| f.0)
            .unwrap_or("?")
    }

    pub fn is_phone(self) -> bool {
        !matches!(self, Os::Macos | Os::Windows | Os::Linux)
    }

    fn is_ios(self) -> bool {
        matches!(self, Os::Ios | Os::IosSimulator)
    }

    /// The wallpaper `--background auto` means under this frame.
    pub fn wallpaper(self) -> &'static str {
        match self {
            Os::Macos => "hills",
            Os::Windows => "bloom",
            Os::Linux => "aubergine",
            Os::Android | Os::AndroidEmulator => "material",
            Os::Ios | Os::IosSimulator => "aurora",
        }
    }

    /// The desktop an emulator runs on when `--desktop on` does not name one.
    fn host(self) -> Os {
        match self {
            Os::IosSimulator => Os::Macos,
            Os::Android | Os::Ios | Os::AndroidEmulator => Os::Macos,
            d => d,
        }
    }

    /// Viewport, scale and video size for a phone frame when the command line
    /// did not choose one. A handset filmed at a laptop's width is a phone
    /// showing a desktop site, which is never what was meant. The emulators
    /// run on a desktop, so their take is landscape.
    pub fn default_shape(self) -> Option<Shape> {
        match self {
            Os::Ios => Some(((393, 852), 3.0, (1080, 1920))),
            Os::Android => Some(((412, 915), 2.625, (1080, 1920))),
            Os::IosSimulator => Some(((393, 852), 3.0, (1920, 1080))),
            Os::AndroidEmulator => Some(((412, 915), 2.625, (1920, 1080))),
            _ => None,
        }
    }

    /// What the browser claims to be, so a site serves its phone layout.
    pub fn user_agent(self) -> Option<&'static str> {
        if !self.is_phone() {
            return None;
        }
        Some(if self.is_ios() {
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_2 like Mac OS X) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/18.2 Mobile/15E148 Safari/604.1"
        } else {
            "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36"
        })
    }
}

/// Viewport, capture scale and video size.
pub type Shape = ((u32, u32), f64, (u32, u32));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// Tabs and an address bar, showing the page's own title, icon and URL.
    Browser,
    /// A bare title bar (desktop) or the full screen (phone), as an installed app.
    App,
    /// The phone's own screen recording: the screen edge to edge, no bezel and
    /// no wallpaper, with the recording indicator in the status bar.
    Recording,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Auto,
    Light,
    Dark,
}

#[derive(Clone, Debug)]
pub enum Icon {
    /// The page's own favicon, or a letter tile when it has none.
    Auto,
    None,
    Builtin(&'static str),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub os: Os,
    pub style: Style,
    pub theme: Theme,
    pub title: Option<String>,
    pub url: Option<String>,
    pub icon: Icon,
    /// The desktop drawn around the window: its menu bar, dock or taskbar.
    pub desktop: Option<Os>,
    pub dock: Option<Vec<DockItem>>,
    /// Whether the dock (or taskbar) is drawn: `None` follows `desktop`,
    /// `Some(true)` draws it on a desktop frame even without the menu bar.
    pub show_dock: Option<bool>,
    pub dock_pos: DockPos,
    /// Tile size in points; macOS ships 16 to 128 and defaults near 48.
    pub dock_size: f64,
    pub icon_set: IconSet,
    /// The colour the `tinted` set is drawn in.
    pub icon_tint: String,
    /// The title the emulators show for the handset.
    pub device_name: Option<String>,
    pub clock: Option<String>,
    pub battery: u8,
}

/// One tile in a dock: a built-in icon, or a finished icon image of the
/// caller's own (a real app's icon they have the rights to), drawn as is.
#[derive(Clone, Debug, PartialEq)]
pub enum DockItem {
    Builtin(&'static str),
    File(PathBuf),
}

/// Which edge the dock sits on. The Windows taskbar only knows the bottom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockPos {
    Bottom,
    Left,
    Right,
}

pub fn parse_dock_pos(s: &str) -> Result<DockPos, String> {
    match s {
        "bottom" => Ok(DockPos::Bottom),
        "left" => Ok(DockPos::Left),
        "right" => Ok(DockPos::Right),
        o => Err(format!(
            "--dock-position must be bottom, left or right, got {o}"
        )),
    }
}

/// How the built-in icons are drawn: one glyph set, several finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconSet {
    /// Gradient tiles, white glyphs: the default.
    Color,
    /// The same hues washed out, glyphs in the deep colour.
    Pastel,
    /// Near-black tiles with the glyph in its colour, as dark-mode icons are.
    Dark,
    /// Light grey tiles, graphite glyphs, no colour at all.
    Mono,
    /// Every tile in one colour, `--icon-tint`.
    Tinted,
    /// Frosted translucent tiles over the wallpaper, white glyphs.
    Glass,
    /// White tiles with the glyph drawn in its colour.
    Outline,
}

pub const ICON_SETS: &[(&str, IconSet, &str)] = &[
    (
        "color",
        IconSet::Color,
        "gradient tiles, white glyphs (default)",
    ),
    (
        "pastel",
        IconSet::Pastel,
        "soft washed-out tiles, deep-colour glyphs",
    ),
    (
        "dark",
        IconSet::Dark,
        "near-black tiles, glyphs in their colour",
    ),
    ("mono", IconSet::Mono, "light grey tiles, graphite glyphs"),
    (
        "tinted",
        IconSet::Tinted,
        "every tile in one colour (--icon-tint)",
    ),
    (
        "glass",
        IconSet::Glass,
        "frosted translucent tiles over the wallpaper",
    ),
    (
        "outline",
        IconSet::Outline,
        "white tiles, coloured line glyphs",
    ),
];

pub fn parse_icon_set(s: &str) -> Result<IconSet, String> {
    ICON_SETS
        .iter()
        .find(|x| x.0 == s)
        .map(|x| x.1)
        .ok_or_else(|| {
            format!(
                "--icon-set must be one of {}, got {s}",
                ICON_SETS.iter().map(|x| x.0).collect::<Vec<_>>().join(", ")
            )
        })
}

/// A colour for `--icon-tint`: #rgb or #rrggbb, nothing that could escape
/// into the chrome's markup.
pub fn parse_tint(s: &str) -> Result<String, String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if (hex.len() == 3 || hex.len() == 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(format!("#{hex}"))
    } else {
        Err(format!(
            "--icon-tint must be a hex colour like #7c5cff, got {s}"
        ))
    }
}

/// Named groups `--dock` accepts alongside single icons.
pub const DOCK_GROUPS: &[(&str, &[&str])] = &[
    (
        "dev",
        &["files", "browser", "terminal", "code", "chat", "settings"],
    ),
    (
        "creative",
        &["files", "photos", "camera", "video", "music", "notes"],
    ),
    (
        "office",
        &["files", "mail", "calendar", "notes", "chat", "browser"],
    ),
    (
        "social",
        &["chat", "camera", "photos", "video", "music", "mail"],
    ),
    ("media", &["music", "video", "photos", "camera"]),
    ("minimal", &["files", "browser", "settings"]),
];

impl Spec {
    /// Viewport, scale and video size when the command line chose none. A
    /// screen recording is the phone's own screen: 9:19.5, and the page gets
    /// what the status bar and home indicator leave of it.
    pub fn default_shape(&self) -> Option<Shape> {
        match (self.os, self.style) {
            (Os::Ios, Style::Recording) => Some(((393, 764), 3.0, (1080, 2344))),
            (Os::Android, Style::Recording) => Some(((412, 855), 2.625, (1080, 2400))),
            (os, _) => os.default_shape(),
        }
    }

    /// The desktop whose dock is drawn, if any.
    pub fn dock_shell(&self) -> Option<Os> {
        match (self.show_dock, self.desktop) {
            (Some(false), _) => None,
            (_, Some(d)) => Some(d),
            (Some(true), None) if !self.os.is_phone() => Some(self.os),
            _ => None,
        }
    }

    pub fn new(os: Os) -> Spec {
        Spec {
            os,
            style: if os.is_phone() {
                Style::App
            } else {
                Style::Browser
            },
            theme: Theme::Auto,
            title: None,
            url: None,
            icon: Icon::Auto,
            desktop: None,
            dock: None,
            show_dock: None,
            dock_pos: DockPos::Bottom,
            dock_size: 52.0,
            icon_set: IconSet::Color,
            icon_tint: "#7c5cff".into(),
            device_name: None,
            clock: None,
            battery: 100,
        }
    }
}

pub fn parse_style(s: &str) -> Result<Style, String> {
    match s {
        "browser" => Ok(Style::Browser),
        "app" | "window" => Ok(Style::App),
        "recording" | "screen" => Ok(Style::Recording),
        o => Err(format!(
            "--frame-style must be browser, app or recording, got {o}"
        )),
    }
}

pub fn parse_theme(s: &str) -> Result<Theme, String> {
    match s {
        "auto" => Ok(Theme::Auto),
        "light" => Ok(Theme::Light),
        "dark" => Ok(Theme::Dark),
        o => Err(format!(
            "--frame-theme must be auto, light or dark, got {o}"
        )),
    }
}

pub fn parse_icon(s: &str) -> Result<Icon, String> {
    match s {
        "auto" | "favicon" => Ok(Icon::Auto),
        "none" | "off" => Ok(Icon::None),
        name => {
            if let Some(b) = builtin_icon(name) {
                return Ok(Icon::Builtin(b.0));
            }
            let p = PathBuf::from(name);
            if p.is_file() {
                Ok(Icon::File(p))
            } else {
                Err(format!(
                    "--frame-icon {name} is neither an icon name nor a file\n\nIcons: {}",
                    icon_names().join(", ")
                ))
            }
        }
    }
}

/// `--dock`: on, off, or a list of icons, groups and image files. Returns
/// whether the dock is shown and its tiles, `None` meaning the system's own
/// default set.
pub fn parse_dock(s: &str) -> Result<(bool, Option<Vec<DockItem>>), String> {
    match s {
        "off" | "none" => return Ok((false, None)),
        "on" | "default" | "" => return Ok((true, None)),
        _ => {}
    }
    let mut out: Vec<DockItem> = Vec::new();
    let mut push = |i: DockItem| {
        if !out.contains(&i) {
            out.push(i);
        }
    };
    for n in s.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        if let Some((_, group)) = DOCK_GROUPS.iter().find(|g| g.0 == n) {
            for i in group.iter() {
                push(DockItem::Builtin(
                    builtin_icon(i).map(|b| b.0).unwrap_or("files"),
                ));
            }
        } else if let Some(b) = builtin_icon(n) {
            push(DockItem::Builtin(b.0));
        } else if Path::new(n).is_file() {
            push(DockItem::File(PathBuf::from(n)));
        } else {
            return Err(format!(
                "--dock: {n} is not an icon, a group or an image file\n\nIcons: {}\nGroups: {}",
                icon_names().join(", "),
                DOCK_GROUPS
                    .iter()
                    .map(|g| g.0)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    Ok((true, Some(out)))
}

/// `--desktop`: off, on (the frame's own system) or a system by name.
pub fn parse_desktop(s: &str, os: Option<Os>) -> Result<Option<Os>, String> {
    match s {
        "off" | "none" => Ok(None),
        "on" => Ok(Some(os.map(Os::host).unwrap_or(Os::Macos))),
        name => match Os::parse(name) {
            Some(o) if !o.is_phone() => Ok(Some(o)),
            _ => Err(format!(
                "--desktop must be on, off, macos, windows or linux, got {name}"
            )),
        },
    }
}

/// Everything that chooses a frame, as text, from either the command line
/// or a `frame` op in a script. One parser for both, so a script and a flag
/// can never disagree about what a word means.
#[derive(Clone, Debug, Default)]
pub struct FrameOpts {
    pub platform: Option<String>,
    pub style: Option<String>,
    pub theme: Option<String>,
    pub icon: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub desktop: Option<String>,
    pub dock: Option<String>,
    pub dock_pos: Option<String>,
    pub dock_size: Option<f64>,
    pub icon_set: Option<String>,
    pub icon_tint: Option<String>,
    pub device_name: Option<String>,
    pub clock: Option<String>,
    pub battery: Option<u8>,
    /// A `frame` op may carry the backdrop too, since the two are chosen together.
    pub background: Option<String>,
}

/// The fields a `frame` op accepts, with the flag each one mirrors.
pub const FRAME_OP_FIELDS: &[(&str, &str)] = &[
    ("platform", "--frame"),
    ("style", "--frame-style"),
    ("theme", "--frame-theme"),
    ("icon", "--frame-icon"),
    ("title", "--frame-title"),
    ("url", "--frame-url"),
    ("desktop", "--desktop"),
    ("dock", "--dock"),
    ("dock_position", "--dock-position"),
    ("dock_size", "--dock-size"),
    ("icon_set", "--icon-set"),
    ("icon_tint", "--icon-tint"),
    ("device_name", "--device-name"),
    ("clock", "--clock"),
    ("battery", "--battery"),
    ("background", "--background"),
];

impl FrameOpts {
    /// Read a `{"op":"frame", ...}` line. Unknown fields are refused: a typo
    /// in a platform declaration is a take on the wrong device.
    pub fn from_op(v: &Value) -> Result<FrameOpts, String> {
        let obj = v.as_object().ok_or("frame: not an object")?;
        for k in obj.keys() {
            if k != "op" && k != "frame" && !FRAME_OP_FIELDS.iter().any(|f| f.0 == k) {
                return Err(format!(
                    "frame: no field \"{k}\"; it takes {}",
                    FRAME_OP_FIELDS
                        .iter()
                        .map(|f| f.0)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        let text = |k: &str| -> Result<Option<String>, String> {
            match &v[k] {
                Value::Null => Ok(None),
                Value::String(s) => Ok(Some(s.clone())),
                Value::Bool(b) => Ok(Some(if *b { "on" } else { "off" }.into())),
                Value::Number(n) => Ok(Some(n.to_string())),
                o => Err(format!("frame: {k} must be a string, got {o}")),
            }
        };
        let num = |k: &str| -> Result<Option<f64>, String> {
            match &v[k] {
                Value::Null => Ok(None),
                Value::Number(n) => Ok(n.as_f64()),
                Value::String(s) => s
                    .parse()
                    .map(Some)
                    .map_err(|_| format!("frame: {k} must be a number, got {s}")),
                o => Err(format!("frame: {k} must be a number, got {o}")),
            }
        };
        let battery = match num("battery")? {
            None => None,
            Some(b) if (0.0..=100.0).contains(&b) => Some(b as u8),
            Some(b) => return Err(format!("frame: battery must be 0 to 100, got {b}")),
        };
        let dock_size = match num("dock_size")? {
            None => None,
            Some(d) if (24.0..=128.0).contains(&d) => Some(d),
            Some(d) => return Err(format!("frame: dock_size must be 24 to 128, got {d}")),
        };
        Ok(FrameOpts {
            platform: text("platform")?.or(text("frame")?),
            style: text("style")?,
            theme: text("theme")?,
            icon: text("icon")?,
            title: text("title")?,
            url: text("url")?,
            desktop: text("desktop")?,
            dock: text("dock")?,
            dock_pos: text("dock_position")?,
            dock_size,
            icon_set: text("icon_set")?,
            icon_tint: text("icon_tint")?,
            device_name: text("device_name")?,
            clock: text("clock")?.map(|c| c.replace("\\n", "\n")),
            battery,
            background: text("background")?,
        })
    }

    /// The spec these options describe, or `None` for no frame.
    pub fn build(&self) -> Result<Option<Spec>, String> {
        let os = match self.platform.as_deref() {
            None | Some("none") | Some("off") => {
                let stray = self.desktop.as_deref().is_some_and(|d| d != "off")
                    || self.style.is_some()
                    || self.dock.is_some()
                    || self.dock_pos.is_some();
                // Said rather than ignored: a dock asked for with no frame is
                // a take that comes out without it.
                if stray {
                    return Err("desktop, style and the dock options need a frame".into());
                }
                return Ok(None);
            }
            Some(name) => Os::parse(name)
                .ok_or_else(|| format!("unknown frame: {name}\n\nFrames:\n{}", help()))?,
        };
        let mut spec = Spec::new(os);
        if let Some(st) = &self.style {
            spec.style = parse_style(st)?;
        }
        if spec.style == Style::Recording && !matches!(os, Os::Ios | Os::Android) {
            return Err(format!(
                "the recording style is a phone's own screen recording, so it needs the ios or \
                 android frame, not {}",
                os.name()
            ));
        }
        if let Some(t) = &self.theme {
            spec.theme = parse_theme(t)?;
        }
        if let Some(i) = &self.icon {
            spec.icon = parse_icon(i)?;
        }
        spec.title = self.title.clone();
        spec.url = self.url.clone();
        spec.desktop = parse_desktop(self.desktop.as_deref().unwrap_or("off"), Some(os))?;
        if let Some(d) = &self.dock {
            let (show, icons) = parse_dock(d)?;
            spec.show_dock = Some(show);
            spec.dock = icons;
        }
        if let Some(p) = &self.dock_pos {
            spec.dock_pos = parse_dock_pos(p)?;
        }
        if let Some(d) = self.dock_size {
            spec.dock_size = d;
        }
        if let Some(i) = &self.icon_set {
            spec.icon_set = parse_icon_set(i)?;
        }
        if let Some(t) = &self.icon_tint {
            spec.icon_tint = parse_tint(t)?;
        }
        if spec.show_dock == Some(true) && spec.dock_shell().is_none() {
            return Err(
                "a dock on a phone frame needs a desktop: a handset has no dock of its own \
                 here, so the dock belongs to the desktop the emulator runs on"
                    .into(),
            );
        }
        if spec.dock_shell() == Some(Os::Windows) && spec.dock_pos != DockPos::Bottom {
            return Err(
                "the Windows taskbar only sits at the bottom; drop the dock position".into(),
            );
        }
        spec.device_name = self.device_name.clone();
        spec.clock = self.clock.clone();
        spec.battery = self.battery.unwrap_or(100);
        Ok(Some(spec))
    }
}

pub fn help() -> String {
    let mut s = FRAMES
        .iter()
        .map(|f| format!("  {:<17} {}", f.0, f.2))
        .collect::<Vec<_>>()
        .join("\n");
    s.push_str("\n  none              no frame (default)");
    s.push_str("\n\nIcons (--frame-icon, --dock):\n  ");
    s.push_str(&icon_names().join(", "));
    s.push_str("\n\nDock groups (--dock, mixable with icons, e.g. dev,maps):\n");
    for (n, g) in DOCK_GROUPS {
        s.push_str(&format!("  {:<9} {}\n", n, g.join(", ")));
    }
    s.push_str("\nIcon sets (--icon-set):\n");
    for (n, _, about) in ICON_SETS {
        s.push_str(&format!("  {n:<9} {about}\n"));
    }
    s.push_str(
        "\n  --frame-icon also takes auto (the page's favicon), none, or a .png/.svg/.jpg file",
    );
    s
}

// ------------------------------------------------------------ the page's side

/// What the chrome shows of the page, read from it when the take stops.
#[derive(Clone, Debug, Default)]
pub struct PageInfo {
    pub title: String,
    pub url: String,
    /// A data: URL, or an http(s) URL the chrome page can load itself.
    pub icon: Option<String>,
    pub top_bg: String,
    pub bottom_bg: String,
    pub top_dark: bool,
    pub bottom_dark: bool,
    pub dark: bool,
    /// Whether the wallpaper under the menu bar is dark, which is what sets
    /// the bar's ink on macOS: white over a dark picture, black over a light
    /// one. Unknown until the backdrop is chosen; `None` follows the theme.
    pub wall_dark: Option<bool>,
}

/// Title, URL, favicon and the colours at the top and bottom edges.
///
/// A phone's status bar is the colour of the page under it (or its
/// theme-color), so the colours are sampled rather than guessed. The favicon
/// is fetched here, inside the page, where it is same-origin; a data URL then
/// renders in the chrome page whatever that page's origin is.
pub const PAGE_JS: &str = r#"(async () => {
  const lum = (c) => {
    const m = (c || '').match(/[\d.]+/g);
    if (!m || m.length < 3) return 1;
    const a = m.length > 3 ? +m[3] : 1;
    if (a === 0) return null;
    return (0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2]) / 255;
  };
  const bgAt = (y) => {
    let el = document.elementFromPoint(innerWidth / 2, y);
    while (el) {
      const c = getComputedStyle(el).backgroundColor;
      if (lum(c) !== null) return c;
      el = el.parentElement;
    }
    for (const e of [document.body, document.documentElement]) {
      if (!e) continue;
      const c = getComputedStyle(e).backgroundColor;
      if (lum(c) !== null) return c;
    }
    return 'rgb(255, 255, 255)';
  };
  const theme = document.querySelector('meta[name="theme-color"]');
  const top = (theme && theme.content) || bgAt(1);
  const bottom = bgAt(innerHeight - 2);
  const probe = document.createElement('i');
  probe.style.color = top;
  document.body && document.body.appendChild(probe);
  const topRgb = getComputedStyle(probe).color;
  probe.remove();
  const links = [...document.querySelectorAll('link[rel~="icon"], link[rel="apple-touch-icon"]')];
  const score = (l) => (/svg/.test(l.type || l.href) ? 1000 : parseInt((l.sizes && l.sizes.value) || '16', 10) || 16);
  links.sort((a, b) => score(b) - score(a));
  let icon = links.length ? links[0].href : (/^https?:/.test(location.protocol) ? location.origin + '/favicon.ico' : '');
  let data = null;
  if (icon && !icon.startsWith('file:')) {
    try {
      const r = await fetch(icon);
      const b = r.ok ? await r.blob() : null;
      if (b && b.size && /image|octet/.test(b.type)) {
        data = await new Promise((res) => { const f = new FileReader(); f.onload = () => res(f.result); f.readAsDataURL(b); });
      }
    } catch (e) {}
  }
  return {
    // A declared icon that could not be fetched is still worth a try from
    // the chrome page; a guessed /favicon.ico that 404s is not.
    title: document.title || '', url: location.href, icon: data || (links.length ? icon : ''),
    top_bg: top, bottom_bg: bottom,
    top_dark: lum(topRgb) < 0.5, bottom_dark: lum(bottom) < 0.5,
    dark: lum(bgAt(innerHeight / 2)) < 0.5,
  };
})()"#;

impl PageInfo {
    pub fn from_value(v: &Value) -> PageInfo {
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let b = |k: &str| v[k].as_bool().unwrap_or(false);
        let raw_icon = s("icon");
        PageInfo {
            title: s("title"),
            url: s("url"),
            icon: resolve_icon_url(&raw_icon),
            top_bg: or_white(s("top_bg")),
            bottom_bg: or_white(s("bottom_bg")),
            top_dark: b("top_dark"),
            bottom_dark: b("bottom_dark"),
            dark: b("dark"),
            wall_dark: None,
        }
    }
}

fn or_white(s: String) -> String {
    if s.is_empty() {
        "#ffffff".into()
    } else {
        s
    }
}

/// A file:// favicon cannot be fetched from inside the page, so it is read here.
fn resolve_icon_url(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if let Some(path) = raw.strip_prefix("file://") {
        let path = percent_decode(path);
        return file_data_url(Path::new(&path)).ok();
    }
    Some(raw.to_string())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn file_data_url(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

// ------------------------------------------------------------------ geometry

/// Integer video-pixel rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Rect {
    fn right(&self) -> i64 {
        self.x + self.w
    }
    fn bottom(&self) -> i64 {
        self.y + self.h
    }
}

/// Where every part of a frame lands in the video.
#[derive(Clone, Debug)]
pub struct Geometry {
    pub out: (u32, u32),
    /// Video pixels per CSS pixel of the page.
    pub k: f64,
    pub content: Rect,
    /// The window or handset, content included.
    pub body: Rect,
    pub body_radius: f64,
    /// Handset only: the bezel's thickness.
    pub bezel: i64,
    /// The emulator's side toolbar or the simulator's title bar.
    pub extra: Option<(Rect, f64)>,
    /// Desktop only: points to video pixels for the menu bar, dock and taskbar.
    pub s: f64,
    pub top_bar: i64,
    pub bottom_bar: i64,
    /// A dock up one side. Only the tests read these: the dock is placed
    /// from the spec, and they prove the window was kept clear of it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub left_bar: i64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub right_bar: i64,
    /// The menu bar or top bar, and the dock or taskbar, where there are
    /// any: frosted glass over the wallpaper.
    pub top_rect: Option<Rect>,
    pub dock_rect: Option<(Rect, f64)>,
}

impl Geometry {
    pub fn layout(&self) -> Layout {
        let mut outline = vec![rf(self.body, self.body_radius)];
        if let Some((r, rad)) = self.extra {
            outline.push(rf(r, rad));
        }
        let pane = |r: Rect, radius: f64, blur: f64| crate::backdrop::Frost {
            x: r.x as f64,
            y: r.y as f64,
            w: r.w as f64,
            h: r.h as f64,
            radius,
            blur,
        };
        let mut frost = Vec::new();
        if let Some(r) = self.top_rect {
            frost.push(pane(r, 0.0, 26.0 * self.s));
        }
        if let Some((r, radius)) = self.dock_rect {
            frost.push(pane(r, radius, 20.0 * self.s));
        }
        Layout {
            content: (self.content.w as u32, self.content.h as u32),
            origin: (self.content.x as u32, self.content.y as u32),
            outline,
            frost,
        }
    }
}

fn rf(r: Rect, rad: f64) -> (f64, f64, f64, f64, f64) {
    (r.x as f64, r.y as f64, r.w as f64, r.h as f64, rad)
}

/// A handset's reference width in points: every phone measurement is written
/// against it, and scaled by the page's actual width.
fn ref_width(os: Os) -> f64 {
    if os.is_ios() {
        393.0
    } else {
        412.0
    }
}

/// Insets around the content, in the frame's own units (see the module note):
/// top, right, bottom, left, the outer corner radius and the bezel.
struct Insets {
    t: f64,
    r: f64,
    b: f64,
    l: f64,
    radius: f64,
    bezel: f64,
}

fn insets(spec: &Spec) -> Insets {
    let browser = spec.style == Style::Browser;
    match spec.os {
        Os::Macos | Os::Windows | Os::Linux => {
            let t = if browser {
                80.0
            } else {
                match spec.os {
                    Os::Macos => 30.0,
                    Os::Windows => 32.0,
                    _ => 46.0,
                }
            };
            let radius = match spec.os {
                Os::Macos => 11.0,
                Os::Windows => 8.0,
                _ => 12.0,
            };
            Insets {
                t,
                r: 0.0,
                b: 0.0,
                l: 0.0,
                radius,
                bezel: 0.0,
            }
        }
        os if spec.style == Style::Recording => {
            // The screen alone: status bar above, home indicator below.
            let (t, b) = if os.is_ios() {
                (54.0, 34.0)
            } else {
                (36.0, 24.0)
            };
            Insets {
                t,
                r: 0.0,
                b,
                l: 0.0,
                radius: 0.0,
                bezel: 0.0,
            }
        }
        os if os.is_ios() => {
            let bezel = 15.0;
            Insets {
                t: bezel + 54.0,
                r: bezel,
                b: bezel + if browser { 86.0 } else { 34.0 },
                l: bezel,
                radius: 55.0 + bezel,
                bezel,
            }
        }
        _ => {
            let bezel = 12.0;
            Insets {
                t: bezel + 36.0 + if browser { 60.0 } else { 0.0 },
                r: bezel,
                b: bezel + 24.0,
                l: bezel,
                radius: 40.0 + bezel,
                bezel,
            }
        }
    }
}

/// The menu bar or top bar, in points.
fn top_bar_pt(d: Os) -> f64 {
    match d {
        Os::Windows => 0.0,
        Os::Linux => 32.0,
        _ => 25.0,
    }
}

/// The dock's proportions, in points, all from the tile size.
///
/// Measured off the macOS dock: a tile's canvas carries its own margin, so
/// tiles sit edge to edge with no gap and still read as spaced; the shelf is
/// a fifth deeper than a tile, its corners about a third of a tile.
struct DockDims {
    pad: f64,
    depth: f64,
    sep: f64,
    radius: f64,
    margin: f64,
}

fn dock_dims(size: f64) -> DockDims {
    DockDims {
        pad: size * 0.1,
        depth: size * 1.2,
        sep: size * 0.3,
        radius: size * 0.36,
        margin: size * 0.08,
    }
}

/// Whether the dock carries the app in front as well as its pinned icons.
fn dock_has_app(spec: &Spec) -> bool {
    spec.os.is_phone() || (spec.style == Style::App && !matches!(spec.icon, Icon::None))
}

/// The dock's length along its edge, in points.
fn dock_len_pt(spec: &Spec, shell: Os) -> f64 {
    let d = dock_dims(spec.dock_size);
    let pinned = spec
        .dock
        .as_ref()
        .map(Vec::len)
        .unwrap_or_else(|| default_dock(shell).len());
    let app = if dock_has_app(spec) { 1 } else { 0 };
    // macOS ends every dock with a separator and the Trash.
    let (trash, sep) = if shell == Os::Macos {
        (1, d.sep)
    } else {
        (0, 0.0)
    };
    (pinned + app + trash) as f64 * spec.dock_size + 2.0 * d.pad + sep
}

/// How much of the screen edge the dock or taskbar takes, in points.
fn dock_depth_pt(shell: Os, size: f64) -> f64 {
    match shell {
        Os::Windows => 48.0,
        _ => {
            let d = dock_dims(size);
            d.depth + d.margin * 2.0
        }
    }
}

const EMU_GAP: f64 = 20.0;
const EMU_W: f64 = 52.0;
const SIM_GAP: f64 = 10.0;
const SIM_H: f64 = 52.0;

fn even_i(v: f64) -> i64 {
    let n = v.round().max(2.0) as i64;
    n - n % 2
}

/// Fit the framed take into the video.
pub fn geometry(spec: &Spec, out_w: u32, out_h: u32, css_w: u32, css_h: u32) -> Geometry {
    let (ow, oh) = (out_w as f64, out_h as f64);
    let (pw, ph) = (css_w as f64, css_h as f64);
    let ins = insets(spec);
    // Frame units per CSS pixel: 1 on a desktop, the page's width against the
    // reference handset's on a phone.
    let u = if spec.os.is_phone() {
        pw / ref_width(spec.os)
    } else {
        1.0
    };
    let (ex_r, ex_t) = match spec.os {
        Os::AndroidEmulator => (EMU_GAP + EMU_W, 0.0),
        Os::IosSimulator => (0.0, SIM_GAP + SIM_H),
        _ => (0.0, 0.0),
    };

    let s = ow.max(oh) / 1470.0;
    let top_bar = spec
        .desktop
        .map(|d| (top_bar_pt(d) * s).round() as i64)
        .unwrap_or(0);
    let (mut bottom_bar, mut left_bar, mut right_bar) = (0, 0, 0);
    if let Some(shell) = spec.dock_shell() {
        let depth = (dock_depth_pt(shell, spec.dock_size) * s).round() as i64;
        match (shell, spec.dock_pos) {
            (Os::Windows, _) | (_, DockPos::Bottom) => bottom_bar = depth,
            (_, DockPos::Left) => left_bar = depth,
            (_, DockPos::Right) => right_bar = depth,
        }
    }
    let short = ow.min(oh);
    let framed_desk = spec.desktop.is_some() || spec.dock_shell().is_some();
    let pad = if spec.style == Style::Recording {
        0.0
    } else {
        (short * if framed_desk { 0.05 } else { 0.055 }).max(12.0)
    };
    let avail_x = left_bar as f64 + pad;
    let avail_y = top_bar as f64 + pad;
    let avail_w = (ow - left_bar as f64 - right_bar as f64 - 2.0 * pad).max(16.0);
    let avail_h = (oh - top_bar as f64 - bottom_bar as f64 - 2.0 * pad).max(16.0);

    // Total extent in CSS px of the page.
    let total_w = pw + (ins.l + ins.r + ex_r) * u;
    let total_h = ph + (ins.t + ins.b + ex_t) * u;
    let k0 = (avail_w / total_w).min(avail_h / total_h);
    let cw = even_i(k0 * pw).min(out_w as i64 - out_w as i64 % 2);
    let ch = even_i(k0 * ph).min(out_h as i64 - out_h as i64 % 2);
    let k = cw as f64 / pw;
    let px = |v: f64| (v * u * k).round() as i64;
    let (it, ir, ib, il) = (px(ins.t), px(ins.r), px(ins.b), px(ins.l));
    let (exr, ext) = (px(ex_r), px(ex_t));
    let full_w = il + cw + ir + exr;
    let full_h = ext + it + ch + ib;
    let bx = avail_x + (avail_w - full_w as f64) / 2.0;
    let by = avail_y + (avail_h - full_h as f64) / 2.0;
    // Positions round to even but may be zero: a screen recording starts at
    // the frame's very edge, where even_i's two pixel floor left a seam.
    let even0 = |v: f64| ((v / 2.0).round() * 2.0).max(0.0) as i64;
    let cx = even0(bx + il as f64);
    let cy = even0(by + ext as f64 + it as f64);
    let content = Rect {
        x: cx,
        y: cy,
        w: cw,
        h: ch,
    };
    let body = Rect {
        x: cx - il,
        y: cy - it,
        w: il + cw + ir,
        h: it + ch + ib,
    };
    let body_radius = ins.radius * u * k;
    let extra = match spec.os {
        Os::AndroidEmulator => {
            let h = ((12.0 * 44.0 + 20.0) * u * k).round() as i64;
            Some((
                Rect {
                    x: body.right() + px(EMU_GAP),
                    y: body.y,
                    w: px(EMU_W),
                    h: h.min(body.h),
                },
                10.0 * u * k,
            ))
        }
        Os::IosSimulator => Some((
            Rect {
                x: body.x,
                y: body.y - ext,
                w: body.w,
                h: px(SIM_H),
            },
            14.0 * u * k,
        )),
        _ => None,
    };
    Geometry {
        out: (out_w, out_h),
        k,
        content,
        body,
        body_radius,
        bezel: px(ins.bezel),
        extra,
        s,
        top_bar,
        bottom_bar,
        left_bar,
        right_bar,
        top_rect: (top_bar > 0).then_some(Rect {
            x: 0,
            y: 0,
            w: out_w as i64,
            h: top_bar,
        }),
        dock_rect: spec.dock_shell().map(|shell| {
            let (ow, oh) = (out_w as i64, out_h as i64);
            if shell == Os::Windows {
                return (
                    Rect {
                        x: 0,
                        y: oh - bottom_bar,
                        w: ow,
                        h: bottom_bar,
                    },
                    0.0,
                );
            }
            let d = dock_dims(spec.dock_size);
            let len = (dock_len_pt(spec, shell) * s).round() as i64;
            let depth = (d.depth * s).round() as i64;
            let margin = (d.margin * s).round() as i64;
            // Up a side, centred on the space under the menu bar.
            let mid = top_bar + (oh - top_bar - len) / 2;
            let r = match spec.dock_pos {
                DockPos::Bottom => Rect {
                    x: (ow - len) / 2,
                    y: oh - depth - margin,
                    w: len,
                    h: depth,
                },
                DockPos::Left => Rect {
                    x: margin,
                    y: mid,
                    w: depth,
                    h: len,
                },
                DockPos::Right => Rect {
                    x: ow - depth - margin,
                    y: mid,
                    w: depth,
                    h: len,
                },
            };
            (r, d.radius * s)
        }),
    }
}

// --------------------------------------------------------------------- icons

/// (name, gradient top, gradient bottom, glyph drawn in a 24 box, white).
const ICONS: &[(&str, &str, &str, &str)] = &[
    (
        "browser",
        "#5fb4ff",
        "#1f6fe5",
        r#"<circle cx="12" cy="12" r="8.5"/><ellipse cx="12" cy="12" rx="3.8" ry="8.5"/><path d="M3.5 12h17M5 7.5h14M5 16.5h14"/>"#,
    ),
    (
        "files",
        "#7cc8ff",
        "#2d86e0",
        r#"<path d="M3.5 7.5a1.5 1.5 0 0 1 1.5-1.5h4l2 2h8a1.5 1.5 0 0 1 1.5 1.5v8a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 17.5z"/><path d="M3.5 10.5h17"/>"#,
    ),
    (
        "mail",
        "#63b8ff",
        "#1a6fe0",
        r#"<rect x="3.5" y="6" width="17" height="12.5" rx="2"/><path d="m4 7 8 6 8-6"/>"#,
    ),
    (
        "chat",
        "#62e684",
        "#1ea94a",
        r#"<path d="M12 4.5c4.7 0 8.5 3 8.5 6.8s-3.8 6.7-8.5 6.7c-1 0-2-.1-2.8-.4L5 19.5l1.2-3.6C4.5 14.7 3.5 13.1 3.5 11.3 3.5 7.5 7.3 4.5 12 4.5z"/>"#,
    ),
    (
        "music",
        "#ff7b8f",
        "#ec2a52",
        r#"<path d="M9 17.5V6.5l10-2v11"/><circle cx="6.8" cy="17.5" r="2.3"/><circle cx="16.8" cy="15.5" r="2.3"/>"#,
    ),
    (
        "photos",
        "#ffc46b",
        "#ff5f6d",
        r#"<circle cx="12" cy="7.6" r="3.2"/><circle cx="16.4" cy="12" r="3.2"/><circle cx="12" cy="16.4" r="3.2"/><circle cx="7.6" cy="12" r="3.2"/>"#,
    ),
    (
        "calendar",
        "#ff6b61",
        "#e0332b",
        r#"<rect x="4" y="5.5" width="16" height="14" rx="2.5"/><path d="M4 9.5h16M8.5 3.5v4M15.5 3.5v4"/><path d="M8.5 13.5h2M13.5 13.5h2M8.5 16.5h2"/>"#,
    ),
    (
        "notes",
        "#ffe27a",
        "#f5ae00",
        r#"<path d="M6 4.5h12v15H6z"/><path d="M8.5 9h7M8.5 12h7M8.5 15h4.5"/>"#,
    ),
    (
        "terminal",
        "#4a4a4a",
        "#141414",
        r#"<path d="m6 8 4 4-4 4M12 16.5h6"/>"#,
    ),
    (
        "code",
        "#6d97ff",
        "#3a4fd8",
        r#"<path d="m8.5 7.5-4.5 4.5 4.5 4.5M15.5 7.5l4.5 4.5-4.5 4.5M13.5 5.5l-3 13"/>"#,
    ),
    (
        "settings",
        "#a3abb4",
        "#5b636c",
        r#"<circle cx="12" cy="12" r="2.8"/><circle cx="12" cy="12" r="7" stroke-width="3.4" stroke-dasharray="2.75 2.75"/><circle cx="12" cy="12" r="5.4"/>"#,
    ),
    (
        "maps",
        "#79e39a",
        "#20ab96",
        r#"<path d="M12 20s-6-5.6-6-10.2a6 6 0 0 1 12 0C18 14.4 12 20 12 20z"/><circle cx="12" cy="9.8" r="2.2"/>"#,
    ),
    (
        "camera",
        "#8a8a8a",
        "#3b3b3b",
        r#"<path d="M4 8.5A1.5 1.5 0 0 1 5.5 7h2.5l1.5-2h5l1.5 2h2.5A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"/><circle cx="12" cy="12.8" r="3.4"/>"#,
    ),
    (
        "store",
        "#6cb6ff",
        "#2766ee",
        r#"<path d="M5.5 8.5h13l-1 11h-11z"/><path d="M9 10.5V7a3 3 0 0 1 6 0v3.5"/>"#,
    ),
    (
        "video",
        "#ff8a50",
        "#e2410c",
        r#"<rect x="3.5" y="6" width="17" height="12" rx="3"/><path d="m10.5 9.5 4.5 2.5-4.5 2.5z" fill="white"/>"#,
    ),
    (
        "ai",
        "#be94ff",
        "#6f3fe8",
        r#"<path d="M12 3.5c.6 4.2 2.6 6.4 7 8.5-4.4 2.1-6.4 4.3-7 8.5-.6-4.2-2.6-6.4-7-8.5 4.4-2.1 6.4-4.3 7-8.5z"/>"#,
    ),
    (
        "game",
        "#ff6ecb",
        "#b92f8a",
        r#"<rect x="3.5" y="8" width="17" height="9" rx="4.5"/><path d="M8 10.5v4M6 12.5h4"/><circle cx="15.5" cy="11.5" r=".6" fill="white"/><circle cx="17" cy="13.5" r=".6" fill="white"/>"#,
    ),
    (
        "device",
        "#9aa6b8",
        "#46505e",
        r#"<rect x="7" y="3.5" width="10" height="17" rx="2.2"/><path d="M10.5 6h3"/>"#,
    ),
    (
        "wallet",
        "#4dd0a8",
        "#138a6c",
        r#"<rect x="3.5" y="6.5" width="17" height="12" rx="2.5"/><path d="M3.5 10h17"/><circle cx="16" cy="14.2" r="1" fill="white"/>"#,
    ),
];

fn builtin_icon(
    name: &str,
) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
    ICONS.iter().find(|i| i.0 == name)
}

pub fn icon_names() -> Vec<&'static str> {
    ICONS.iter().map(|i| i.0).collect()
}

fn hex_rgb(h: &str) -> (f64, f64, f64) {
    let h = h.trim_start_matches('#');
    let h = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        h.to_string()
    };
    let v =
        |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(128) as f64;
    (v(0), v(2), v(4))
}

/// Mix a colour towards white (t > 0) or black (t < 0).
fn shade(h: &str, t: f64) -> String {
    let (r, g, b) = hex_rgb(h);
    let (tr, tg, tb) = if t >= 0.0 {
        (255.0, 255.0, 255.0)
    } else {
        (0.0, 0.0, 0.0)
    };
    let t = t.abs();
    format!(
        "#{:02x}{:02x}{:02x}",
        (r + (tr - r) * t).round() as u8,
        (g + (tg - g) * t).round() as u8,
        (b + (tb - b) * t).round() as u8
    )
}

/// A built-in icon as a rounded app tile, finished in the chosen set.
/// A superellipse (n = 5), the continuous-corner shape macOS draws its icons
/// in. A rounded rect's corner starts abruptly; this one eases in.
fn squircle(cx: f64, cy: f64, a: f64) -> String {
    let n = 5.0;
    let mut d = String::new();
    for i in 0..96 {
        let t = i as f64 / 96.0 * std::f64::consts::TAU;
        let (c, s) = (t.cos(), t.sin());
        let x = cx + a * c.signum() * c.abs().powf(2.0 / n);
        let y = cy + a * s.signum() * s.abs().powf(2.0 / n);
        d.push_str(&format!("{}{x:.2} {y:.2}", if i == 0 { "M" } else { "L" }));
    }
    d.push('Z');
    d
}

fn tile_svg(name: &str, uid: &str, set: IconSet, tint: &str, mac: bool) -> String {
    if set == IconSet::Color {
        if let Some(t) = art_tile(name, uid, mac) {
            return t;
        }
    }
    let (n, a, b, glyph) = builtin_icon(name).copied().unwrap_or(ICONS[0]);
    let (top, bottom, ink, rim, extra) = match set {
        IconSet::Color => (
            a.to_string(),
            b.to_string(),
            "#fff".to_string(),
            "rgba(255,255,255,.25)",
            "",
        ),
        IconSet::Pastel => (
            shade(a, 0.62),
            shade(b, 0.5),
            shade(b, -0.1),
            "rgba(255,255,255,.5)",
            "",
        ),
        IconSet::Dark => (
            "#2e2e31".into(),
            "#161618".into(),
            shade(a, 0.1),
            "rgba(255,255,255,.12)",
            "",
        ),
        IconSet::Mono => (
            "#f2f2f4".into(),
            "#cfd0d5".into(),
            "#3a3a3c".into(),
            "rgba(0,0,0,.08)",
            "",
        ),
        IconSet::Tinted => (
            shade(tint, -0.35),
            shade(tint, -0.7),
            shade(tint, 0.55),
            "rgba(255,255,255,.16)",
            "",
        ),
        IconSet::Glass => (
            "rgba(255,255,255,.34)".into(),
            "rgba(255,255,255,.12)".into(),
            "#fff".into(),
            "rgba(255,255,255,.6)",
            r#"<path d="M8 20c6-8 22-12 48-8" stroke="rgba(255,255,255,.35)" stroke-width="2" fill="none" stroke-linecap="round"/>"#,
        ),
        IconSet::Outline => (
            "#ffffff".into(),
            "#f1f2f5".into(),
            b.to_string(),
            "rgba(0,0,0,.1)",
            "",
        ),
    };
    if mac {
        /*
         * The macOS icon grid: the body is 824 of a 1024 canvas, so about
         * 81%, sits a touch above centre, and casts a short soft shadow.
         * A sheen fades out down the top half, and the glyph is drawn a
         * little heavier than on a flat tile so it holds at dock size.
         */
        let body = squircle(32.0, 31.0, 26.0);
        return format!(
            r##"<svg viewBox="0 0 64 64" width="100%" height="100%" style="overflow:visible"><defs><linearGradient id="g{uid}{n}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient><linearGradient id="h{uid}{n}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".28"/><stop offset=".5" stop-color="#fff" stop-opacity="0"/></linearGradient><filter id="f{uid}{n}" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="1.1" stdDeviation="1.1" flood-color="#000" flood-opacity=".32"/></filter></defs><path d="{body}" fill="url(#g{uid}{n})" filter="url(#f{uid}{n})"/>{extra}<path d="{body}" fill="url(#h{uid}{n})"/><path d="{body}" fill="none" stroke="{rim}" stroke-width=".6"/><g transform="translate(15.4 14.4) scale(1.3833)" fill="none" stroke="{ink}" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{glyph}</g></svg>"##
        )
        .replace("fill=\"white\"", &format!("fill=\"{ink}\""));
    }
    format!(
        r##"<svg viewBox="0 0 64 64" width="100%" height="100%"><defs><linearGradient id="g{uid}{n}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient></defs><rect x="2" y="2" width="60" height="60" rx="14" fill="url(#g{uid}{n})"/>{extra}<rect x="2.5" y="2.5" width="59" height="59" rx="13.5" fill="none" stroke="{rim}"/><g transform="translate(12 12) scale(1.6667)" fill="none" stroke="{ink}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{glyph}</g></svg>"##
    )
    .replace("fill=\"white\"", &format!("fill=\"{ink}\""))
}

/// A built-in glyph on its own, for a tab or a title bar.
fn glyph_svg(name: &str, color: &str) -> String {
    let (_, a, _, glyph) = builtin_icon(name).copied().unwrap_or(ICONS[0]);
    let _ = color;
    format!(
        r##"<svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke="{a}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{glyph}</svg>"##
    )
}

/// The app's own icon, resolved to something drawable: a tile or an image.
enum AppIcon {
    None,
    Builtin(&'static str),
    Image(String),
    Letter(char),
}

fn app_icon(spec: &Spec, page: &PageInfo, title: &str) -> AppIcon {
    match &spec.icon {
        Icon::None => AppIcon::None,
        Icon::Builtin(n) => AppIcon::Builtin(n),
        Icon::File(p) => match file_data_url(p) {
            Ok(d) => AppIcon::Image(d),
            Err(_) => AppIcon::Letter(first_letter(title)),
        },
        Icon::Auto => match &page.icon {
            Some(u) => AppIcon::Image(u.clone()),
            None => AppIcon::Letter(first_letter(title)),
        },
    }
}

fn first_letter(s: &str) -> char {
    s.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('K')
}

/// Small icon for a tab or title bar, `px` points square.
fn small_icon(icon: &AppIcon, px: f64, accent: &str) -> String {
    match icon {
        AppIcon::None => String::new(),
        AppIcon::Builtin(n) => format!(
            r#"<span class="ico" style="width:{px}px;height:{px}px">{}</span>"#,
            glyph_svg(n, accent)
        ),
        AppIcon::Image(src) => format!(
            r#"<img class="ico" src="{}" onerror="this.style.visibility='hidden'" style="width:{px}px;height:{px}px;object-fit:contain">"#,
            esc(src)
        ),
        AppIcon::Letter(c) => format!(
            r#"<span class="ico" style="width:{px}px;height:{px}px;border-radius:50%;background:{accent};color:#fff;font-size:{}px;font-weight:700;display:inline-flex;align-items:center;justify-content:center">{}</span>"#,
            px * 0.62,
            esc(&c.to_string())
        ),
    }
}

/// How a dock draws its tiles.
#[derive(Clone, Copy)]
struct Look<'a> {
    set: IconSet,
    tint: &'a str,
    /// macOS tiles: a squircle inset in its canvas, with a drop shadow.
    mac: bool,
}

/// One dock tile. A caller's own icon file is a finished icon, drawn as is.
fn dock_tile(item: &DockItem, px: f64, uid: &str, look: Look) -> String {
    match item {
        DockItem::Builtin(n) => big_icon(&AppIcon::Builtin(n), px, uid, look),
        DockItem::File(p) => match file_data_url(p) {
            Ok(src) => format!(
                r#"<img src="{}" onerror="this.style.visibility='hidden'" style="width:{px}px;height:{px}px;object-fit:contain;display:block">"#,
                esc(&src)
            ),
            Err(_) => String::new(),
        },
    }
}

/// A built-in icon's illustration on its tile: clipped to a squircle inset
/// in the canvas on macOS, a rounded square filling it elsewhere, with the
/// tile's own shadow and a faint top light.
fn art_tile(name: &str, uid: &str, mac: bool) -> Option<String> {
    let art = crate::icons::art(name)?;
    let id = format!("a{uid}{name}");
    let body = art.replace("ID", &id);
    let clip = if mac {
        squircle(50.0, 50.0, 50.0)
    } else {
        "M22 0H78A22 22 0 0 1 100 22V78A22 22 0 0 1 78 100H22A22 22 0 0 1 0 78V22A22 22 0 0 1 22 0Z"
            .into()
    };
    // The macOS grid: the body is about 81% of the canvas, a touch high.
    let (tx, ty, sc) = if mac {
        (6.0, 5.0, 0.52)
    } else {
        (2.0, 2.0, 0.6)
    };
    Some(format!(
        r##"<svg viewBox="0 0 64 64" width="100%" height="100%" style="overflow:visible;display:block"><defs><clipPath id="{id}c"><path d="{clip}"/></clipPath><filter id="{id}sh" x="-25%" y="-25%" width="150%" height="160%"><feDropShadow dx="0" dy="1.8" stdDeviation="1.8" flood-color="#000" flood-opacity=".22"/></filter><filter id="{id}t" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="2.2" stdDeviation="2.2" flood-color="#000" flood-opacity=".3"/></filter><linearGradient id="{id}l" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".16"/><stop offset=".45" stop-color="#fff" stop-opacity="0"/></linearGradient></defs><g transform="translate({tx} {ty}) scale({sc})"><g filter="url(#{id}t)"><g clip-path="url(#{id}c)">{body}<rect width="100" height="100" fill="url(#{id}l)"/></g></g><path d="{clip}" fill="none" stroke="#000" stroke-opacity=".1" stroke-width="1"/></g></svg>"##
    ))
}

/// Large icon for a dock or taskbar, `px` points square.
fn big_icon(icon: &AppIcon, px: f64, uid: &str, look: Look) -> String {
    match icon {
        AppIcon::None => String::new(),
        AppIcon::Builtin(n) => format!(
            r#"<span style="width:{px}px;height:{px}px;display:block">{}</span>"#,
            tile_svg(n, uid, look.set, look.tint, look.mac)
        ),
        AppIcon::Image(src) if look.mac => format!(
            r#"<span style="width:{px}px;height:{px}px;display:flex;align-items:center;justify-content:center"><span style="width:81%;height:81%;display:flex;align-items:center;justify-content:center;background:{};border-radius:23%;box-shadow:0 .5px 1.5px rgba(0,0,0,.28),inset 0 0 0 .5px rgba(0,0,0,.08)"><img src="{}" onerror="this.style.visibility='hidden'" style="width:66%;height:66%;object-fit:contain"></span></span>"#,
            match look.set {
                IconSet::Dark => "#1f1f22".to_string(),
                IconSet::Glass => "rgba(255,255,255,.3)".to_string(),
                IconSet::Tinted => shade(look.tint, -0.5),
                _ => "linear-gradient(#ffffff,#eceef1)".to_string(),
            },
            esc(src)
        ),
        AppIcon::Image(src) => format!(
            r#"<span style="width:{px}px;height:{px}px;display:flex;align-items:center;justify-content:center;background:{};border-radius:{}px;box-shadow:inset 0 0 0 1px rgba(0,0,0,.08)"><img src="{}" onerror="this.style.visibility='hidden'" style="width:70%;height:70%;object-fit:contain"></span>"#,
            match look.set {
                IconSet::Dark => "#1f1f22".to_string(),
                IconSet::Glass => "rgba(255,255,255,.3)".to_string(),
                IconSet::Tinted => shade(look.tint, -0.5),
                _ => "#fff".to_string(),
            },
            px * 0.225,
            esc(src)
        ),
        AppIcon::Letter(c) if look.mac => format!(
            r#"<span style="width:{px}px;height:{px}px;display:flex;align-items:center;justify-content:center"><span style="width:81%;height:81%;display:flex;align-items:center;justify-content:center;background:linear-gradient(#6b7bff,#3a3fd0);color:#fff;border-radius:23%;box-shadow:0 .5px 1.5px rgba(0,0,0,.3),inset 0 0 0 .5px rgba(255,255,255,.2);font:600 {}px {FONT}">{}</span></span>"#,
            px * 0.42,
            esc(&c.to_string())
        ),
        AppIcon::Letter(c) => format!(
            r#"<span style="width:{px}px;height:{px}px;display:flex;align-items:center;justify-content:center;background:linear-gradient(#5b6cff,#3a3fd0);color:#fff;border-radius:{}px;font-weight:700;font-size:{}px;font-family:{FONT}">{}</span>"#,
            px * 0.225,
            px * 0.5,
            esc(&c.to_string())
        ),
    }
}

// ------------------------------------------------------------------- the html

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// What an address bar shows: host and path, no scheme; a local file by name.
fn display_url(url: &str) -> (String, String) {
    if let Some(rest) = url.strip_prefix("file://") {
        let name = rest.rsplit('/').next().unwrap_or(rest);
        return ("File".into(), format!("  {}", percent_decode(name)));
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    match rest.find('/') {
        Some(i) => {
            let path = &rest[i..];
            (
                rest[..i].to_string(),
                if path == "/" {
                    String::new()
                } else {
                    path.to_string()
                },
            )
        }
        None => (rest.to_string(), String::new()),
    }
}

fn host_only(url: &str) -> String {
    let (h, _) = display_url(url);
    if h == "File" {
        display_url(url).1.trim().to_string()
    } else {
        h
    }
}

struct Palette {
    strip: &'static str,
    bar: &'static str,
    omni: &'static str,
    text: &'static str,
    dim: &'static str,
    line: &'static str,
    title_bg: &'static str,
    title_text: &'static str,
}

fn palette(os: Os, dark: bool) -> Palette {
    let chrome = if dark {
        (
            "#202124", "#35363a", "#202124", "#e8eaed", "#9aa0a6", "#2a2b2e",
        )
    } else {
        (
            "#dfe3e8", "#ffffff", "#eff1f4", "#1f1f1f", "#5f6368", "#d3d6db",
        )
    };
    let (title_bg, title_text) = match (os, dark) {
        (Os::Windows, false) => ("#f3f3f3", "#1a1a1a"),
        (Os::Windows, true) => ("#202020", "#ffffff"),
        (Os::Linux, false) => ("#ebebeb", "#2e2e2e"),
        (Os::Linux, true) => ("#303030", "#ffffff"),
        (_, false) => ("#ececec", "#3a3a3a"),
        (_, true) => ("#2b2b2d", "#dedede"),
    };
    Palette {
        strip: chrome.0,
        bar: chrome.1,
        omni: chrome.2,
        text: chrome.3,
        dim: chrome.4,
        line: chrome.5,
        title_bg,
        title_text,
    }
}

const FONT: &str = "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI Variable', 'Segoe UI', Inter, Cantarell, Roboto, 'Noto Sans', 'Helvetica Neue', Arial, system-ui, sans-serif";

/// A box at a video-pixel rect whose inside is laid out in `unit` video pixels
/// per point.
fn part(r: Rect, unit: f64, style: &str, inner: &str) -> String {
    format!(
        r#"<div style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:hidden;{style}"><div style="zoom:{unit};width:{}px;height:{}px;position:relative">{inner}</div></div>"#,
        r.x,
        r.y,
        r.w,
        r.h,
        r.w as f64 / unit,
        r.h as f64 / unit,
    )
}

fn traffic_lights(size: f64, gap: f64) -> String {
    ["#ff5f57", "#febc2e", "#28c840"]
        .iter()
        .map(|c| format!(
            r#"<span style="width:{size}px;height:{size}px;border-radius:50%;background:{c};box-shadow:inset 0 0 0 .5px rgba(0,0,0,.18);display:inline-block;margin-right:{gap}px"></span>"#
        ))
        .collect()
}

fn svg(w: f64, h: f64, view: &str, color: &str, body: &str) -> String {
    format!(
        r#"<svg width="{w}" height="{h}" viewBox="{view}" fill="none" stroke="{color}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" style="display:block;flex:none">{body}</svg>"#
    )
}

/// Minimise, maximise, close: Windows draws hairlines, GNOME round buttons.
fn window_controls(os: Os, color: &str, h: f64, dark: bool) -> String {
    match os {
        Os::Windows => {
            let b = |glyph: &str| {
                format!(
                    r#"<span style="width:46px;height:{h}px;display:flex;align-items:center;justify-content:center">{}</span>"#,
                    svg(10.0, 10.0, "0 0 10 10", color, glyph)
                        .replace("stroke-width=\"1.6\"", "stroke-width=\"1\"")
                )
            };
            format!(
                r#"<div style="display:flex;height:{h}px;flex:none">{}{}{}</div>"#,
                b(r#"<path d="M0 5.5h10"/>"#),
                b(r#"<rect x=".5" y=".5" width="9" height="9" rx="1"/>"#),
                b(r#"<path d="m.5.5 9 9M9.5.5l-9 9"/>"#)
            )
        }
        _ => {
            let bg = if dark { "#4a4a4a" } else { "#dadada" };
            let b = |glyph: &str| {
                format!(
                    r#"<span style="width:24px;height:24px;border-radius:50%;background:{bg};display:flex;align-items:center;justify-content:center;margin-left:12px">{}</span>"#,
                    svg(10.0, 10.0, "0 0 10 10", color, glyph)
                )
            };
            format!(
                r#"<div style="display:flex;align-items:center;flex:none;padding:0 12px 0 0">{}{}{}</div>"#,
                b(r#"<path d="M2 7h6"/>"#),
                b(r#"<rect x="2" y="2" width="6" height="6" rx=".5"/>"#),
                b(r#"<path d="m2.5 2.5 5 5M7.5 2.5l-5 5"/>"#)
            )
        }
    }
}

/// Tabs and an address bar, drawn Chromium's way with the system's controls.
fn browser_bar(os: Os, p: &Palette, dark: bool, icon: &AppIcon, title: &str, url: &str) -> String {
    let (host, path) = display_url(url);
    let lead = if os == Os::Macos {
        format!(
            r#"<div style="position:absolute;left:14px;top:14px;display:flex">{}</div>"#,
            traffic_lights(12.0, 8.0)
        )
    } else {
        String::new()
    };
    let pad_left = if os == Os::Macos { 84 } else { 8 };
    let controls = if os == Os::Macos {
        String::new()
    } else {
        format!(
            r#"<div style="margin-left:auto;height:40px;display:flex;align-items:center">{}</div>"#,
            window_controls(os, p.text, 40.0, dark)
        )
    };
    let nav = |d: &str| {
        format!(
            r#"<span style="width:32px;height:32px;display:flex;align-items:center;justify-content:center;flex:none">{}</span>"#,
            svg(18.0, 18.0, "0 0 24 24", p.dim, d)
        )
    };
    let tab_icon = match icon {
        AppIcon::None => small_icon(&AppIcon::Letter(first_letter(title)), 16.0, "#5b6cff"),
        i => small_icon(i, 16.0, "#5b6cff"),
    };
    format!(
        r#"<div style="height:100%;font-family:{FONT};color:{text};">
<div style="height:40px;background:{strip};display:flex;align-items:flex-end;padding-left:{pad_left}px;position:relative">{lead}
  <div style="height:34px;width:min(240px,46%);background:{bar};border-radius:10px 10px 0 0;display:flex;align-items:center;gap:8px;padding:0 10px 0 12px;font-size:12px;flex:none">{tab_icon}<span style="flex:1;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{title}</span>{close}</div>
  <span style="width:28px;height:34px;display:flex;align-items:center;justify-content:center;flex:none">{plus}</span>{controls}
</div>
<div style="height:40px;background:{bar};display:flex;align-items:center;gap:2px;padding:0 6px;box-shadow:inset 0 -1px 0 {line}">
  {back}{fwd}{reload}
  <div style="flex:1;min-width:0;height:30px;border-radius:15px;background:{omni};display:flex;align-items:center;gap:9px;padding:0 14px;font-size:14px;margin:0 6px">{tune}<span style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis"><span>{host}</span><span style="color:{dim}">{path}</span></span></div>
  <span style="width:26px;height:26px;border-radius:50%;background:linear-gradient(135deg,#7aa7ff,#5b6cff);flex:none;margin:0 4px"></span>{kebab}
</div></div>"#,
        text = p.text,
        strip = p.strip,
        bar = p.bar,
        omni = p.omni,
        line = p.line,
        dim = p.dim,
        title = esc(if title.is_empty() { "New Tab" } else { title }),
        close = svg(
            12.0,
            12.0,
            "0 0 12 12",
            p.dim,
            r#"<path d="m3 3 6 6M9 3 3 9"/>"#
        ),
        plus = svg(
            14.0,
            14.0,
            "0 0 14 14",
            p.dim,
            r#"<path d="M7 2v10M2 7h10"/>"#
        ),
        back = nav(r#"<path d="M19 12H5m6-6-6 6 6 6"/>"#),
        fwd = nav(r#"<path d="M5 12h14m-6-6 6 6-6 6"/>"#),
        reload = nav(r#"<path d="M19 12a7 7 0 1 1-2.1-5M19 4.5V9h-4.5"/>"#),
        tune = svg(
            15.0,
            15.0,
            "0 0 24 24",
            p.dim,
            r#"<path d="M4 8h9m4 0h3M4 16h3m4 0h9"/><circle cx="15" cy="8" r="2"/><circle cx="9" cy="16" r="2"/>"#
        ),
        kebab = svg(
            18.0,
            18.0,
            "0 0 24 24",
            p.dim,
            r#"<circle cx="12" cy="5.5" r=".8"/><circle cx="12" cy="12" r=".8"/><circle cx="12" cy="18.5" r=".8"/>"#
        ),
        host = esc(&host),
        path = esc(&path),
    )
}

/// An installed app's title bar.
fn app_bar(os: Os, p: &Palette, dark: bool, icon: &AppIcon, title: &str, h: f64) -> String {
    let t = esc(title);
    match os {
        Os::Macos => format!(
            r#"<div style="height:100%;background:{bg};box-shadow:inset 0 -1px 0 {line};display:flex;align-items:center;justify-content:center;position:relative;font:600 13px {FONT};color:{fg}"><div style="position:absolute;left:13px;top:{top}px;display:flex">{lights}</div><span style="max-width:70%;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{t}</span></div>"#,
            bg = p.title_bg,
            fg = p.title_text,
            line = if dark { "#111" } else { "#d0d0d0" },
            top = (h - 12.0) / 2.0,
            lights = traffic_lights(12.0, 8.0),
        ),
        Os::Windows => format!(
            r#"<div style="height:100%;background:{bg};display:flex;align-items:center;font:12px {FONT};color:{fg}"><span style="display:flex;align-items:center;gap:10px;padding-left:12px;flex:1;min-width:0">{ico}<span style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{t}</span></span>{ctl}</div>"#,
            bg = p.title_bg,
            fg = p.title_text,
            ico = small_icon(icon, 16.0, "#0067c0"),
            ctl = window_controls(os, p.title_text, h, dark),
        ),
        _ => format!(
            r#"<div style="height:100%;background:{bg};box-shadow:inset 0 -1px 0 {line};display:flex;align-items:center;justify-content:center;position:relative;font:700 14px {FONT};color:{fg}"><span style="position:absolute;left:14px;top:{top}px">{ico}</span><span style="max-width:60%;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{t}</span><div style="position:absolute;right:0;top:0;height:100%;display:flex;align-items:center">{ctl}</div></div>"#,
            bg = p.title_bg,
            fg = p.title_text,
            line = if dark { "#1f1f1f" } else { "#d6d6d6" },
            top = (h - 18.0) / 2.0,
            ico = small_icon(icon, 18.0, "#3584e4"),
            ctl = window_controls(os, p.title_text, h, dark),
        ),
    }
}

fn battery_ios(level: u8, fg: &str) -> String {
    let w = 20.0 * level.min(100) as f64 / 100.0;
    let fill = if level <= 20 { "#ff3b30" } else { fg };
    format!(
        r#"<svg width="27" height="13" viewBox="0 0 27 13" style="display:block;flex:none"><rect x=".5" y=".5" width="23" height="12" rx="3.8" fill="none" stroke="{fg}" stroke-opacity=".4"/><rect x="2" y="2" width="{w}" height="9" rx="2.4" fill="{fill}"/><path d="M25 4.5v4c.8-.3 1.3-1.1 1.3-2s-.5-1.7-1.3-2z" fill="{fg}" fill-opacity=".45"/></svg>"#
    )
}

fn signal_bars(fg: &str) -> String {
    format!(
        r#"<svg width="18" height="12" viewBox="0 0 18 12" style="display:block;flex:none" fill="{fg}"><rect x="0" y="8" width="3" height="4" rx=".8"/><rect x="5" y="5.5" width="3" height="6.5" rx=".8"/><rect x="10" y="3" width="3" height="9" rx=".8"/><rect x="15" y="0" width="3" height="12" rx=".8"/></svg>"#
    )
}

fn wifi(fg: &str, w: f64) -> String {
    format!(
        r#"<svg width="{w}" height="{h}" viewBox="0 0 16 12" style="display:block;flex:none" fill="{fg}"><path d="M8 2.4c2.3 0 4.4.9 6 2.3l1.2-1.3A10.4 10.4 0 0 0 8 .6C5.2.6 2.7 1.7.8 3.4L2 4.7a8.6 8.6 0 0 1 6-2.3z"/><path d="M8 5.9c1.4 0 2.6.5 3.6 1.3l1.2-1.3A7.2 7.2 0 0 0 8 4.1c-1.8 0-3.5.7-4.8 1.8l1.2 1.3c1-.8 2.2-1.3 3.6-1.3z"/><path d="M8 9.2c.5 0 1 .2 1.3.5L8 11.2 6.7 9.7c.3-.3.8-.5 1.3-.5z"/></svg>"#,
        h = w * 0.75
    )
}

fn fg_on(dark_bg: bool) -> &'static str {
    if dark_bg {
        "#ffffff"
    } else {
        "#000000"
    }
}

/// The iOS status bar. In a screen recording the clock sits in the red
/// recording pill and there is no Dynamic Island: it is hardware, a hole in
/// the glass, and a recording is of the pixels behind it.
fn ios_status(page: &PageInfo, clock: &str, battery: u8, recording: bool) -> String {
    let fg = fg_on(page.top_dark);
    let clock = if recording {
        format!(
            r#"<span style="background:#ff3b30;color:#fff;border-radius:999px;padding:2px 10px 2px 10px">{}</span>"#,
            esc(clock)
        )
    } else {
        esc(clock)
    };
    let island = if recording {
        ""
    } else {
        r#"<div style="position:absolute;left:50%;top:11px;width:124px;height:36px;margin-left:-62px;border-radius:18px;background:#000"></div>"#
    };
    format!(
        r#"<div style="height:100%;background:{bg};position:relative;font:600 17px {FONT};color:{fg};letter-spacing:-.2px">
<div style="position:absolute;left:0;top:0;width:36%;height:100%;display:flex;align-items:center;justify-content:center;padding:4px 0 0 12px;box-sizing:border-box">{clock}</div>
{island}
<div style="position:absolute;right:0;top:0;width:36%;height:100%;display:flex;align-items:center;justify-content:center;gap:6px;padding:4px 14px 0 0;box-sizing:border-box">{sig}{wifi}{bat}</div></div>"#,
        bg = page.top_bg,
        sig = signal_bars(fg),
        wifi = wifi(fg, 17.0),
        bat = battery_ios(battery, fg),
    )
}

/// The Android status bar. A screen recording shows the red record icon
/// beside the clock, and no camera hole, for the same reason as on iOS.
fn android_status(page: &PageInfo, clock: &str, battery: u8, recording: bool) -> String {
    let fg = fg_on(page.top_dark);
    let rec = if recording {
        r##"<svg width="16" height="16" viewBox="0 0 16 16" style="display:block;margin-left:8px"><circle cx="8" cy="8" r="7" fill="none" stroke="#ff453a" stroke-width="1.6"/><circle cx="8" cy="8" r="3.6" fill="#ff453a"/></svg>"##
    } else {
        ""
    };
    let hole = if recording {
        ""
    } else {
        r#"<div style="position:absolute;left:50%;top:9px;width:20px;height:20px;margin-left:-10px;border-radius:50%;background:#050505;box-shadow:0 0 0 1.5px rgba(128,128,128,.25)"></div>"#
    };
    let level = 13.0 * battery.min(100) as f64 / 100.0;
    format!(
        r#"<div style="height:100%;background:{bg};position:relative;font:500 14px Roboto,{FONT};color:{fg}">
<div style="position:absolute;left:22px;top:0;height:100%;display:flex;align-items:center">{clock}{rec}</div>
{hole}
<div style="position:absolute;right:20px;top:0;height:100%;display:flex;align-items:center;gap:6px">{wifi}<svg width="14" height="14" viewBox="0 0 14 14" style="display:block" fill="{fg}"><path d="M14 0v14H0z"/></svg><svg width="9" height="15" viewBox="0 0 9 15" style="display:block"><rect x="2.8" y="0" width="3.4" height="2" rx=".5" fill="{fg}"/><rect x=".75" y="1.75" width="7.5" height="12.5" rx="1.4" fill="none" stroke="{fg}" stroke-width="1.3"/><rect x="2" y="{ly}" width="5" height="{level}" rx=".5" fill="{fg}"/></svg></div></div>"#,
        bg = page.top_bg,
        clock = esc(clock),
        wifi = wifi(fg, 15.0),
        ly = 14.0 - level - 0.3,
    )
}

/// A rounded rect as a path, for rings cut with the even-odd rule.
fn rr(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    format!(
        "M{} {y}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}H{}A{r} {r} 0 0 1 {x} {}V{}A{r} {r} 0 0 1 {} {y}Z",
        x + r,
        x + w - r,
        x + w,
        y + r,
        y + h - r,
        x + w - r,
        y + h,
        x + r,
        y + h - r,
        y + r,
        x + r
    )
}

/// The handset itself: a metal band round the edge, the black glass border
/// inside it, and the side buttons standing proud of the band. iPhone in
/// natural titanium with its action button, volume, side button and camera
/// control; Pixel in obsidian with its power key over a volume rocker.
/// `unit` is video pixels per point.
fn hardware(os: Os, b: Rect, bezel: i64, radius: f64, unit: f64) -> String {
    let ios = os.is_ios();
    let band = if ios { 3.4 } else { 2.8 } * unit;
    let m = (3.0 * unit).ceil();
    let (w, h, bz) = (b.w as f64, b.h as f64, bezel as f64);
    let metal = if ios {
        r##"<stop offset="0" stop-color="#d8d3ca"/><stop offset=".07" stop-color="#8f8a82"/><stop offset=".5" stop-color="#bdb7ad"/><stop offset=".93" stop-color="#8a857d"/><stop offset="1" stop-color="#d2ccc3"/>"##
    } else {
        r##"<stop offset="0" stop-color="#6b6e74"/><stop offset=".07" stop-color="#2c2e32"/><stop offset=".5" stop-color="#4a4d52"/><stop offset=".93" stop-color="#2a2c30"/><stop offset="1" stop-color="#666a70"/>"##
    };
    // Buttons, in points down the body from its top: (right side?, from, to).
    let buttons: &[(bool, f64, f64)] = if ios {
        &[
            (false, 118.0, 146.0),
            (false, 178.0, 238.0),
            (false, 252.0, 312.0),
            (true, 206.0, 304.0),
            (true, 520.0, 578.0),
        ]
    } else {
        &[(true, 170.0, 222.0), (true, 262.0, 360.0)]
    };
    let bw = 2.6 * unit;
    let mut keys = String::new();
    for &(right, from, to) in buttons {
        let (y0, y1) = (from * unit, to * unit);
        if y1 > h - radius {
            continue;
        }
        let x = if right {
            m + w - 0.8 * unit
        } else {
            m - bw + 0.8 * unit
        };
        keys.push_str(&format!(
            r#"<rect x="{x:.2}" y="{y0:.2}" width="{bw:.2}" height="{:.2}" rx="{:.2}" fill="url(#hwm)"/>"#,
            y1 - y0,
            bw / 2.0
        ));
    }
    let outer = rr(m, 0.0, w, h, radius);
    let inner_band = rr(
        m + band,
        band,
        w - 2.0 * band,
        h - 2.0 * band,
        radius - band,
    );
    let inner_glass = rr(
        m + bz,
        bz,
        w - 2.0 * bz,
        h - 2.0 * bz,
        (radius - bz).max(0.0),
    );
    format!(
        r##"<svg style="position:absolute;left:{}px;top:{}px;overflow:visible" width="{}" height="{}" viewBox="0 0 {} {}"><defs><linearGradient id="hwm" x1="0" y1="0" x2="1" y2="0">{metal}</linearGradient></defs>{keys}<path d="{outer}{inner_band}" fill="url(#hwm)" fill-rule="evenodd"/><path d="{inner_band}{inner_glass}" fill="#050506" fill-rule="evenodd"/><path d="{outer}" fill="none" stroke="#000" stroke-opacity=".35" stroke-width="{:.2}"/><path d="{inner_band}" fill="none" stroke="#fff" stroke-opacity=".18" stroke-width="{:.2}"/></svg>"##,
        b.x as f64 - m,
        b.y,
        w + 2.0 * m,
        h,
        w + 2.0 * m,
        h,
        0.6 * unit,
        0.5 * unit
    )
}

fn home_indicator(page: &PageInfo, ios: bool) -> String {
    let fg = fg_on(page.bottom_dark);
    let (w, h, bottom) = if ios {
        (134.0, 5.0, 8.0)
    } else {
        (108.0, 4.0, 9.0)
    };
    format!(
        r#"<div style="position:absolute;left:50%;bottom:{bottom}px;width:{w}px;height:{h}px;margin-left:-{}px;border-radius:3px;background:{fg};opacity:.85"></div>"#,
        w / 2.0
    )
}

fn ios_bottom(page: &PageInfo, browser: bool, dark: bool, url: &str) -> String {
    if !browser {
        return format!(
            r#"<div style="height:100%;background:{};position:relative">{}</div>"#,
            page.bottom_bg,
            home_indicator(page, true)
        );
    }
    let (bg, omni, fg, dim) = if dark {
        ("#1c1c1e", "#2c2c2e", "#ffffff", "#8e8e93")
    } else {
        ("#f6f6f7", "#e3e3e8", "#000000", "#8a8a8e")
    };
    let bar_page = PageInfo {
        bottom_dark: dark,
        ..page.clone()
    };
    format!(
        r#"<div style="height:100%;background:{bg};position:relative;font:17px {FONT};color:{fg};box-shadow:inset 0 1px 0 rgba(128,128,128,.25)">
<div style="position:absolute;left:12px;right:12px;top:8px;height:44px;border-radius:13px;background:{omni};display:flex;align-items:center;justify-content:center;gap:7px">
<span style="position:absolute;left:14px;font-size:15px;font-weight:600;color:{fg}">&#x1D00;A</span>{lock}<span style="max-width:62%;white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{host}</span>
<span style="position:absolute;right:14px">{reload}</span></div>{home}</div>"#,
        lock = svg(11.0, 14.0, "0 0 11 14", dim, r#"<rect x="1" y="6" width="9" height="7" rx="1.5" fill="currentColor"/><path d="M3 6V4a2.5 2.5 0 0 1 5 0v2"/>"#).replace("currentColor", dim),
        host = esc(&host_only(url)),
        reload = svg(17.0, 17.0, "0 0 24 24", fg, r#"<path d="M19 12a7 7 0 1 1-2.1-5M19 4.5V9h-4.5"/>"#),
        home = home_indicator(&bar_page, true),
    )
}

fn android_toolbar(dark: bool, url: &str) -> String {
    let (bg, omni, fg, dim) = if dark {
        ("#1f1f1f", "#2f3033", "#e3e3e3", "#aaaaaa")
    } else {
        ("#ffffff", "#eef0f3", "#1f1f1f", "#5f6368")
    };
    format!(
        r#"<div style="height:100%;background:{bg};display:flex;align-items:center;gap:12px;padding:0 12px 0 16px;font:16px Roboto,{FONT};color:{fg};box-shadow:inset 0 -1px 0 rgba(128,128,128,.2)">
{home}<div style="flex:1;min-width:0;height:44px;border-radius:22px;background:{omni};display:flex;align-items:center;gap:10px;padding:0 16px">{tune}<span style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{host}</span></div>
<span style="width:20px;height:20px;border:2px solid {fg};border-radius:4px;display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;flex:none;box-sizing:border-box">1</span>{kebab}</div>"#,
        home = svg(
            22.0,
            22.0,
            "0 0 24 24",
            fg,
            r#"<path d="M4 11 12 4l8 7v8.5h-5.5v-5h-5v5H4z"/>"#
        ),
        tune = svg(
            16.0,
            16.0,
            "0 0 24 24",
            dim,
            r#"<path d="M4 8h9m4 0h3M4 16h3m4 0h9"/><circle cx="15" cy="8" r="2"/><circle cx="9" cy="16" r="2"/>"#
        ),
        host = esc(&host_only(url)),
        kebab = svg(
            20.0,
            20.0,
            "0 0 24 24",
            fg,
            r#"<circle cx="12" cy="5.5" r=".9"/><circle cx="12" cy="12" r=".9"/><circle cx="12" cy="18.5" r=".9"/>"#
        ),
    )
}

fn emulator_toolbar(dark: bool) -> String {
    let (bg, fg) = if dark {
        ("#2b2d30", "#dfe1e5")
    } else {
        ("#f2f2f2", "#3c3c3c")
    };
    let ico = |d: &str| {
        format!(
            r#"<span style="height:44px;display:flex;align-items:center;justify-content:center">{}</span>"#,
            svg(22.0, 22.0, "0 0 24 24", fg, d)
        )
    };
    let small = |d: &str| svg(12.0, 12.0, "0 0 12 12", fg, d);
    format!(
        r#"<div style="height:100%;background:{bg};border-radius:10px;display:flex;flex-direction:column;box-shadow:inset 0 0 0 1px rgba(128,128,128,.25)">
<div style="height:32px;display:flex;align-items:center;justify-content:center;gap:10px">{close}{min}</div>
{power}{volup}{voldown}{rotl}{rotr}{cam}{zoom}{back}{home}{over}{more}</div>"#,
        close = small(r#"<path d="m3 3 6 6M9 3 3 9"/>"#),
        min = small(r#"<path d="M2.5 6h7"/>"#),
        power = ico(r#"<path d="M12 3.5v8"/><path d="M7 6.2a7 7 0 1 0 10 0"/>"#),
        volup = ico(
            r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>"#
        ),
        voldown =
            ico(r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6"/>"#),
        rotl = ico(
            r#"<rect x="9" y="8" width="11" height="12" rx="1.5"/><path d="M4 12a7 7 0 0 1 7-7M8.5 3 11 5l-2.5 2.2"/>"#
        ),
        rotr = ico(
            r#"<rect x="4" y="8" width="11" height="12" rx="1.5"/><path d="M20 12a7 7 0 0 0-7-7M15.5 3 13 5l2.5 2.2"/>"#
        ),
        cam = ico(
            r#"<path d="M4 8.5A1.5 1.5 0 0 1 5.5 7h2.5l1.5-2h5l1.5 2h2.5A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"/><circle cx="12" cy="12.8" r="3.4"/>"#
        ),
        zoom =
            ico(r#"<circle cx="10.5" cy="10.5" r="6"/><path d="m15 15 5 5M10.5 8v5M8 10.5h5"/>"#),
        back = ico(r#"<path d="M16 5.5v13L6 12z"/>"#),
        home = ico(r#"<circle cx="12" cy="12" r="6.5"/>"#),
        over = ico(r#"<rect x="6" y="6" width="12" height="12" rx="1.5"/>"#),
        more = ico(
            r#"<circle cx="6" cy="12" r=".9"/><circle cx="12" cy="12" r=".9"/><circle cx="18" cy="12" r=".9"/>"#
        ),
    )
}

fn simulator_title(dark: bool, name: &str, sub: &str) -> String {
    let (bg, fg, dim) = if dark {
        ("#2b2b2d", "#ececec", "#9a9a9e")
    } else {
        ("#ececec", "#262626", "#7a7a7e")
    };
    let ico = |d: &str| svg(18.0, 18.0, "0 0 24 24", fg, d);
    format!(
        r#"<div style="height:100%;background:{bg};border-radius:14px;display:flex;align-items:center;padding:0 14px;gap:12px;font:{FONT};box-shadow:inset 0 0 0 1px rgba(128,128,128,.25)">
<div style="display:flex;flex:none">{lights}</div>
<div style="display:flex;flex-direction:column;line-height:1.15;min-width:0;flex:1"><span style="font:600 13px {FONT};color:{fg};white-space:nowrap;overflow:hidden;text-overflow:ellipsis">{name}</span><span style="font:11px {FONT};color:{dim}">{sub}</span></div>
<div style="display:flex;gap:14px;flex:none">{home}{shot}{rot}</div></div>"#,
        lights = traffic_lights(12.0, 8.0),
        name = esc(name),
        sub = esc(sub),
        home = ico(r#"<path d="M4 11 12 4l8 7v8.5h-5.5v-5h-5v5H4z"/>"#),
        shot = ico(
            r#"<path d="M4 8.5A1.5 1.5 0 0 1 5.5 7h2.5l1.5-2h5l1.5 2h2.5A1.5 1.5 0 0 1 20 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"/><circle cx="12" cy="12.8" r="3.4"/>"#
        ),
        rot = ico(
            r#"<rect x="9" y="8" width="11" height="12" rx="1.5"/><path d="M4 12a7 7 0 0 1 7-7M8.5 3 11 5l-2.5 2.2"/>"#
        ),
    )
}

// ------------------------------------------------------------ desktop shells

const APPLE: &str = r#"<svg width="14" height="17" viewBox="0 0 14 17" style="display:block" fill="currentColor"><path d="M11.6 9c0-2 1.7-3 1.8-3-1-1.4-2.5-1.6-3-1.6-1.3-.1-2.5.8-3.2.8-.7 0-1.7-.7-2.8-.7C3 4.5 1.6 5.4.9 6.8c-1.5 2.6-.4 6.4 1 8.5.7 1 1.5 2.2 2.6 2.1 1-.1 1.4-.7 2.7-.7 1.2 0 1.6.7 2.7.6 1.1 0 1.8-1 2.5-2 .8-1.2 1.1-2.3 1.1-2.4 0 0-2-.8-2-3.9zM9.6 3c.6-.7 1-1.7.9-2.7-.9 0-1.9.6-2.5 1.3-.6.6-1 1.6-.9 2.6 1 .1 1.9-.5 2.5-1.2z"/></svg>"#;

fn default_dock(desktop: Os) -> Vec<&'static str> {
    match desktop {
        Os::Windows => vec!["files", "browser", "mail", "store", "settings"],
        Os::Linux => vec!["browser", "files", "terminal", "code", "settings"],
        _ => vec![
            "files", "browser", "mail", "chat", "music", "photos", "calendar", "notes", "settings",
        ],
    }
}

fn mac_menubar(app: &str, clock: &str, dark: bool) -> String {
    let (bg, fg) = if dark {
        ("rgba(24,24,26,.38)", "#ffffff")
    } else {
        ("rgba(255,255,255,.32)", "#000000")
    };
    let menus = ["File", "Edit", "View", "History", "Window", "Help"]
        .iter()
        .map(|m| format!(r#"<span>{m}</span>"#))
        .collect::<String>();
    format!(
        r#"<div style="height:100%;background:{bg};color:{fg};font:13px {FONT};display:flex;align-items:center;gap:20px;padding:0 16px 0 18px;box-sizing:border-box">
<span style="color:{fg}">{APPLE}</span><b style="font-weight:700">{app}</b>{menus}
<span style="margin-left:auto;display:flex;align-items:center;gap:16px">{bat}{wifi}{search}<span>{clock}</span></span></div>"#,
        app = esc(app),
        clock = esc(clock),
        bat = battery_ios(100, fg),
        wifi = wifi(fg, 16.0),
        search = svg(
            14.0,
            14.0,
            "0 0 24 24",
            fg,
            r#"<circle cx="10.5" cy="10.5" r="6.5" stroke-width="2.4"/><path d="m15.5 15.5 5 5" stroke-width="2.4"/>"#
        ),
    )
}

/// The macOS Trash: a frosted wire bin standing on the dock, no tile.
const TRASH: &str = r##"<svg viewBox="0 0 64 64" width="100%" height="100%" style="overflow:visible"><defs><linearGradient id="trg" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#fff" stop-opacity=".5"/><stop offset=".5" stop-color="#fff" stop-opacity=".22"/><stop offset="1" stop-color="#fff" stop-opacity=".45"/></linearGradient><filter id="trf" x="-20%" y="-20%" width="140%" height="150%"><feDropShadow dx="0" dy="1.1" stdDeviation="1.1" flood-color="#000" flood-opacity=".3"/></filter></defs><g filter="url(#trf)"><path d="M17.5 16.5h29l-3 37.5a3 3 0 0 1-3 2.8H23.5a3 3 0 0 1-3-2.8z" fill="url(#trg)" stroke="rgba(255,255,255,.85)" stroke-width="1"/><path d="M17.5 16.5h29l-3 37.5a3 3 0 0 1-3 2.8H23.5a3 3 0 0 1-3-2.8z" fill="none" stroke="rgba(0,0,0,.18)" stroke-width=".5"/><path d="M25 22l1 30M32 22v30M39 22l-1 30" stroke="rgba(255,255,255,.7)" stroke-width="1.1" stroke-linecap="round"/><ellipse cx="32" cy="16.5" rx="15.5" ry="3" fill="rgba(255,255,255,.55)" stroke="rgba(255,255,255,.9)" stroke-width=".8"/><ellipse cx="32" cy="16.5" rx="12" ry="1.6" fill="rgba(0,0,0,.12)"/></g></svg>"##;

/// The dock (macOS, GNOME) as a shelf of tiles, along the bottom or up one
/// side. `mac` gives it the macOS details: squircle tiles, a separator and
/// the Trash at the far end, and the running light in the shelf's own ink.
#[allow(clippy::too_many_arguments)]
fn dock(
    icons: &[DockItem],
    app: Option<&AppIcon>,
    active_builtin: Option<&str>,
    dark: bool,
    size: f64,
    pos: DockPos,
    look: Look,
) -> String {
    let d = dock_dims(size);
    let vertical = pos != DockPos::Bottom;
    // The running light sits between the tile and the screen edge.
    let off = (d.depth - size) / 2.0 * 0.3;
    let dot_at = match pos {
        DockPos::Bottom => format!("left:50%;bottom:{off}px;margin-left:-2px"),
        DockPos::Left => format!("top:50%;left:{off}px;margin-top:-2px"),
        DockPos::Right => format!("top:50%;right:{off}px;margin-top:-2px"),
    };
    let ink = if dark || !look.mac {
        "rgba(255,255,255,.8)"
    } else {
        "rgba(0,0,0,.62)"
    };
    let dot = format!(
        r#"<span style="position:absolute;{dot_at};width:4px;height:4px;border-radius:50%;background:{ink}"></span>"#
    );
    let cell = |inner: String, on: bool| {
        format!(
            r#"<span style="position:relative;display:flex;align-items:center;justify-content:center;flex:none;{}">{inner}{}</span>"#,
            if vertical {
                format!("width:{}px;height:{size}px", d.depth)
            } else {
                format!("width:{size}px;height:{}px", d.depth)
            },
            if on { dot.as_str() } else { "" }
        )
    };
    let mut tiles = String::new();
    for (i, item) in icons.iter().enumerate() {
        tiles.push_str(&cell(
            dock_tile(item, size, &format!("d{i}"), look),
            matches!(item, DockItem::Builtin(n) if active_builtin == Some(*n)),
        ));
    }
    if let Some(a) = app {
        if !matches!(a, AppIcon::None) {
            tiles.push_str(&cell(big_icon(a, size, "app", look), true));
        }
    }
    if look.mac {
        let line = if vertical {
            format!(
                "height:1px;width:{}px;margin:{}px 0",
                size * 0.8,
                (d.sep - 1.0) / 2.0
            )
        } else {
            format!(
                "width:1px;height:{}px;margin:0 {}px",
                size * 0.8,
                (d.sep - 1.0) / 2.0
            )
        };
        let sep_ink = if dark {
            "rgba(255,255,255,.28)"
        } else {
            "rgba(0,0,0,.2)"
        };
        tiles.push_str(&format!(
            r#"<span style="{line};background:{sep_ink};flex:none"></span>"#
        ));
        tiles.push_str(&cell(
            format!(r#"<span style="width:{size}px;height:{size}px;display:block">{TRASH}</span>"#),
            false,
        ));
    }
    let (bg, rim) = match (look.mac, dark) {
        (true, false) => ("rgba(255,255,255,.26)", "rgba(255,255,255,.42)"),
        (true, true) => ("rgba(28,28,30,.42)", "rgba(255,255,255,.16)"),
        (false, _) => ("rgba(30,30,32,.62)", "rgba(255,255,255,.12)"),
    };
    let dir = if vertical { "column" } else { "row" };
    let padding = if vertical {
        format!("{}px 0", d.pad)
    } else {
        format!("0 {}px", d.pad)
    };
    format!(
        r#"<div style="width:100%;height:100%;background:{bg};border-radius:{r}px;box-shadow:inset 0 0 0 .5px {rim},0 0 0 .5px rgba(0,0,0,.14),0 6px 18px rgba(0,0,0,.14);display:flex;flex-direction:{dir};align-items:center;padding:{padding};box-sizing:border-box">{tiles}</div>"#,
        r = d.radius
    )
}

fn windows_taskbar(
    icons: &[DockItem],
    app: &AppIcon,
    active_builtin: Option<&str>,
    clock: &str,
    dark: bool,
    look: Look,
) -> String {
    let (bg, fg) = if dark {
        ("rgba(32,32,32,.74)", "#ffffff")
    } else {
        ("rgba(243,245,249,.76)", "#111111")
    };
    let start = r##"<svg width="24" height="24" viewBox="0 0 24 24" style="display:block"><defs><linearGradient id="ws" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#2ec5ff"/><stop offset="1" stop-color="#0063d6"/></linearGradient></defs><rect x="1" y="1" width="10.5" height="10.5" rx="1" fill="url(#ws)"/><rect x="12.5" y="1" width="10.5" height="10.5" rx="1" fill="url(#ws)"/><rect x="1" y="12.5" width="10.5" height="10.5" rx="1" fill="url(#ws)"/><rect x="12.5" y="12.5" width="10.5" height="10.5" rx="1" fill="url(#ws)"/></svg>"##;
    let slot = |inner: String, on: bool| {
        format!(
            r#"<span style="width:44px;height:40px;border-radius:6px;display:flex;align-items:center;justify-content:center;position:relative;{}">{inner}{}</span>"#,
            if on {
                "background:rgba(128,128,128,.18)"
            } else {
                ""
            },
            if on {
                r#"<span style="position:absolute;bottom:2px;left:50%;width:16px;height:3px;margin-left:-8px;border-radius:2px;background:#0078d4"></span>"#
            } else {
                ""
            }
        )
    };
    let mut row = slot(start.to_string(), false);
    row.push_str(&slot(
        svg(
            20.0,
            20.0,
            "0 0 24 24",
            fg,
            r#"<circle cx="10.5" cy="10.5" r="6.5"/><path d="m15.5 15.5 5 5"/>"#,
        ),
        false,
    ));
    for (i, item) in icons.iter().enumerate() {
        row.push_str(&slot(
            dock_tile(item, 26.0, &format!("t{i}"), look),
            matches!(item, DockItem::Builtin(n) if active_builtin == Some(*n)),
        ));
    }
    if !matches!(app, AppIcon::None) {
        row.push_str(&slot(big_icon(app, 26.0, "tapp", look), true));
    }
    let mut lines = clock.splitn(2, '\n');
    let (t, d) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
    format!(
        r#"<div style="height:100%;background:{bg};box-shadow:inset 0 1px 0 rgba(128,128,128,.25);position:relative;font:12px {FONT};color:{fg}">
<div style="position:absolute;left:0;right:0;top:4px;height:40px;display:flex;justify-content:center;gap:4px">{row}</div>
<div style="position:absolute;right:12px;top:0;height:100%;display:flex;align-items:center;gap:14px">{wifi}{spk}{bat}<span style="display:flex;flex-direction:column;align-items:flex-end;line-height:1.35"><span>{t}</span><span>{d}</span></span></div></div>"#,
        wifi = wifi(fg, 16.0),
        spk = svg(
            16.0,
            16.0,
            "0 0 24 24",
            fg,
            r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>"#
        ),
        bat = battery_ios(100, fg),
        t = esc(t),
        d = esc(d),
    )
}

fn gnome_topbar(clock: &str) -> String {
    format!(
        r#"<div style="height:100%;background:#000;color:#fff;font:600 14px Cantarell,{FONT};position:relative">
<div style="position:absolute;left:12px;top:0;height:100%;display:flex;align-items:center;gap:6px"><span style="width:28px;height:8px;border-radius:4px;background:#fff"></span><span style="width:8px;height:8px;border-radius:4px;background:#888"></span><span style="width:8px;height:8px;border-radius:4px;background:#888"></span></div>
<div style="position:absolute;left:0;right:0;top:0;height:100%;display:flex;align-items:center;justify-content:center">{clock}</div>
<div style="position:absolute;right:14px;top:0;height:100%;display:flex;align-items:center;gap:12px">{wifi}{spk}{bat}</div></div>"#,
        clock = esc(clock),
        wifi = wifi("#fff", 16.0),
        spk = svg(
            16.0,
            16.0,
            "0 0 24 24",
            "#fff",
            r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6"/>"#
        ),
        bat = battery_ios(100, "#fff"),
    )
}

// ------------------------------------------------------------------ assembly

/// The chrome for one take, as a standalone HTML page `out` pixels square.
pub fn html(spec: &Spec, page: &PageInfo, g: &Geometry) -> String {
    let dark = match spec.theme {
        Theme::Light => false,
        Theme::Dark => true,
        Theme::Auto => page.dark,
    };
    let title = spec.title.clone().unwrap_or_else(|| page.title.clone());
    let url = spec.url.clone().unwrap_or_else(|| page.url.clone());
    let icon = app_icon(spec, page, &title);
    let pal = palette(spec.os, dark);
    let browser = spec.style == Style::Browser;
    let c = g.content;
    let b = g.body;
    let mut parts: Vec<String> = Vec::new();

    if spec.os.is_phone() {
        // Points to video pixels on the handset.
        let unit = g.k * (c.w as f64 / g.k) / ref_width(spec.os);
        let inner_top = b.y + g.bezel;
        let inner_bottom = b.bottom() - g.bezel;
        let clock = spec.clock.clone().unwrap_or_else(|| "9:41".into());
        let status_h_pt = if spec.os.is_ios() { 54.0 } else { 36.0 };
        // Exactly to the content's top edge unless a toolbar shares the space,
        // or a one pixel seam of nothing shows between the two.
        let status_h = if !spec.os.is_ios() && browser {
            ((status_h_pt * unit).round() as i64).min(c.y - inner_top)
        } else {
            c.y - inner_top
        };
        let status = Rect {
            x: c.x,
            y: inner_top,
            w: c.w,
            h: status_h,
        };
        // The screen's own corners: the bars are rects, the glass is not.
        let recording = spec.style == Style::Recording;
        let r_in = (g.body_radius - g.bezel as f64).max(0.0);
        parts.push(part(
            status,
            unit,
            &format!("border-radius:{r_in}px {r_in}px 0 0"),
            &if spec.os.is_ios() {
                ios_status(page, &clock, spec.battery, recording)
            } else {
                android_status(page, &clock, spec.battery, recording)
            },
        ));
        if !spec.os.is_ios() && browser {
            let bar = Rect {
                x: c.x,
                y: status.bottom(),
                w: c.w,
                h: c.y - status.bottom(),
            };
            parts.push(part(bar, unit, "", &android_toolbar(dark, &url)));
        }
        let bottom = Rect {
            x: c.x,
            y: c.bottom(),
            w: c.w,
            h: inner_bottom - c.bottom(),
        };
        parts.push(part(
            bottom,
            unit,
            &format!("border-radius:0 0 {r_in}px {r_in}px"),
            &if spec.os.is_ios() {
                ios_bottom(page, browser, dark, &url)
            } else {
                format!(
                    r#"<div style="height:100%;background:{};position:relative">{}</div>"#,
                    page.bottom_bg,
                    home_indicator(page, false)
                )
            },
        ));
        // The hardware last, so the bezel's inner curve rounds off the
        // screen's corners. A screen recording has none.
        if !recording {
            parts.push(hardware(spec.os, b, g.bezel, g.body_radius, unit));
        }
        if let Some((r, _)) = g.extra {
            let name_default = if spec.os.is_ios() {
                "iPhone 16 Pro"
            } else {
                "Pixel 9 Pro"
            };
            let name = spec
                .device_name
                .clone()
                .unwrap_or_else(|| name_default.into());
            parts.push(part(
                r,
                unit,
                "",
                &match spec.os {
                    Os::IosSimulator => simulator_title(dark, &name, "iOS 18.2"),
                    _ => emulator_toolbar(dark),
                },
            ));
        }
    } else {
        let unit = g.k;
        let bar = Rect {
            x: c.x,
            y: b.y,
            w: c.w,
            h: c.y - b.y,
        };
        let radius = g.body_radius;
        let inner = if browser {
            browser_bar(spec.os, &pal, dark, &icon, &title, &url)
        } else {
            app_bar(spec.os, &pal, dark, &icon, &title, bar.h as f64 / unit)
        };
        parts.push(part(
            bar,
            unit,
            &format!("border-radius:{radius}px {radius}px 0 0"),
            &inner,
        ));
        // A hairline round the whole window, as every desktop draws one.
        parts.push(format!(
            r#"<div style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;border-radius:{radius}px;box-shadow:inset 0 0 0 {}px rgba({},.{})"></div>"#,
            b.x,
            b.y,
            b.w,
            b.h,
            unit.max(1.0),
            if dark { "255,255,255" } else { "0,0,0" },
            if dark { "16" } else { "22" },
        ));
    }

    let look = Look {
        set: spec.icon_set,
        tint: &spec.icon_tint,
        mac: spec.dock_shell() == Some(Os::Macos),
    };
    let s = g.s;
    let (ow, oh) = (g.out.0 as i64, g.out.1 as i64);
    /*
     * The app in front: the browser, the page as an installed app, or,
     * round a handset, the emulator itself. A phone on a desktop only
     * makes sense as an emulator's window.
     */
    let emulator = AppIcon::Builtin("device");
    let (active, app_tile, app_name) = if spec.os.is_phone() {
        let name = if spec.os.is_ios() {
            "Simulator"
        } else {
            "Emulator"
        };
        (None, Some(&emulator), name.to_string())
    } else if browser {
        (Some("browser"), None, "Browser".to_string())
    } else {
        let name = if title.is_empty() {
            "App".into()
        } else {
            title.clone()
        };
        (None, Some(&icon), name)
    };

    match spec.desktop {
        Some(Os::Macos) => {
            let clock = spec
                .clock
                .clone()
                .unwrap_or_else(|| "Tue 9 Jun  9:41".into());
            parts.push(part(
                Rect {
                    x: 0,
                    y: 0,
                    w: ow,
                    h: g.top_bar,
                },
                s,
                "",
                &mac_menubar(&app_name, &clock, page.wall_dark.unwrap_or(dark)),
            ));
        }
        Some(Os::Linux) => {
            let clock = spec.clock.clone().unwrap_or_else(|| "Jun 9  09:41".into());
            parts.push(part(
                Rect {
                    x: 0,
                    y: 0,
                    w: ow,
                    h: g.top_bar,
                },
                s,
                "",
                &gnome_topbar(&clock),
            ));
        }
        _ => {}
    }

    if let Some(shell) = spec.dock_shell() {
        let icons = spec.dock.clone().unwrap_or_else(|| {
            default_dock(shell)
                .into_iter()
                .map(DockItem::Builtin)
                .collect()
        });
        if shell == Os::Windows {
            let clock = spec
                .clock
                .clone()
                .unwrap_or_else(|| "9:41 AM\n09/06/2026".into());
            parts.push(part(
                Rect {
                    x: 0,
                    y: oh - g.bottom_bar,
                    w: ow,
                    h: g.bottom_bar,
                },
                s,
                "",
                &windows_taskbar(
                    &icons,
                    app_tile.unwrap_or(&AppIcon::None),
                    active,
                    &clock,
                    dark,
                    look,
                ),
            ));
        } else {
            let html = dock(
                &icons,
                app_tile,
                active,
                shell == Os::Linux || dark,
                spec.dock_size,
                spec.dock_pos,
                look,
            );
            let (r, _) = g.dock_rect.expect("a dock shell always has a dock rect");
            parts.push(part(r, s, "overflow:visible", &html));
        }
    }

    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>html,body{{margin:0;background:transparent;width:{}px;height:{}px;overflow:hidden;-webkit-font-smoothing:antialiased}}*{{box-sizing:border-box}}.ico{{display:inline-block;flex:none;vertical-align:middle}}</style></head><body>{}</body></html>"#,
        g.out.0,
        g.out.1,
        parts.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frame_parses_and_lays_out_inside_the_video() {
        for (name, os, _) in FRAMES {
            assert_eq!(Os::parse(name), Some(*os));
            for style in [Style::Browser, Style::App] {
                for desktop in [None, Some(Os::Macos), Some(Os::Windows), Some(Os::Linux)] {
                    let mut spec = Spec::new(*os);
                    spec.style = style;
                    spec.desktop = desktop;
                    let ((cw, ch), _, (ow, oh)) =
                        os.default_shape()
                            .unwrap_or(((1470, 830), 2.0, (1470, 830)));
                    for (ow, oh) in [(ow, oh), (1080, 1920), (1920, 1080), (1080, 1080)] {
                        let g = geometry(&spec, ow, oh, cw, ch);
                        let c = g.content;
                        assert_eq!(
                            (c.x % 2, c.y % 2, c.w % 2, c.h % 2),
                            (0, 0, 0, 0),
                            "{name} not even"
                        );
                        for r in [g.body, c].into_iter().chain(g.extra.map(|e| e.0)) {
                            assert!(
                                r.x >= 0 && r.y >= 0,
                                "{name} {ow}x{oh}: {r:?} off the top left"
                            );
                            assert!(
                                r.right() <= ow as i64 && r.bottom() <= oh as i64,
                                "{name} {ow}x{oh}: {r:?} off the bottom right"
                            );
                        }
                        // The body holds the content.
                        assert!(g.body.x <= c.x && g.body.right() >= c.right());
                        assert!(g.body.y <= c.y && g.body.bottom() >= c.bottom());
                        // Aspect survives, to the pixel.
                        let a = cw as f64 / ch as f64;
                        assert!(((c.w as f64 / c.h as f64) - a).abs() / a < 0.01);
                        // Desktop bars do not overlap the window.
                        if desktop.is_some() {
                            assert!(g.body.y >= g.top_bar, "{name}: window under the menu bar");
                            let top = g.extra.map(|e| e.0.y).unwrap_or(g.body.y).min(g.body.y);
                            assert!(top >= g.top_bar);
                            assert!(g.body.bottom() <= oh as i64 - g.bottom_bar);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_dock_on_any_edge_keeps_clear_of_the_window() {
        for pos in [DockPos::Bottom, DockPos::Left, DockPos::Right] {
            for desktop in [None, Some(Os::Macos)] {
                let mut spec = Spec::new(Os::Macos);
                spec.show_dock = Some(true);
                spec.dock_pos = pos;
                spec.desktop = desktop;
                spec.dock_size = 72.0;
                let g = geometry(&spec, 1920, 1080, 1470, 830);
                let b = g.body;
                assert!(
                    b.x >= g.left_bar && b.right() <= 1920 - g.right_bar,
                    "{pos:?}"
                );
                assert!(
                    b.y >= g.top_bar && b.bottom() <= 1080 - g.bottom_bar,
                    "{pos:?}"
                );
                let depth = [g.left_bar, g.right_bar, g.bottom_bar];
                assert_eq!(depth.iter().filter(|d| **d > 0).count(), 1, "{pos:?}");
                assert_eq!(
                    g.top_bar > 0,
                    desktop.is_some(),
                    "menu bar only with --desktop"
                );
            }
        }
        let mut off = Spec::new(Os::Macos);
        off.desktop = Some(Os::Macos);
        off.show_dock = Some(false);
        assert_eq!(off.dock_shell(), None);
        let mut phone = Spec::new(Os::Ios);
        phone.show_dock = Some(true);
        assert_eq!(phone.dock_shell(), None, "a handset alone has no dock");
    }

    #[test]
    fn a_frame_op_reads_like_the_flags() {
        let v: Value = serde_json::from_str(
            r#"{"op":"frame","platform":"ios","style":"recording","battery":40,"clock":"10:08"}"#,
        )
        .unwrap();
        let spec = FrameOpts::from_op(&v).unwrap().build().unwrap().unwrap();
        assert_eq!(spec.os, Os::Ios);
        assert_eq!(spec.style, Style::Recording);
        assert_eq!(spec.battery, 40);
        assert_eq!(spec.default_shape().unwrap().2, (1080, 2344));
        // "frame" is accepted for "platform", and booleans read as on/off.
        let v: Value = serde_json::from_str(
            r#"{"op":"frame","frame":"macos","desktop":true,"dock":"dev","dock_size":64}"#,
        )
        .unwrap();
        let spec = FrameOpts::from_op(&v).unwrap().build().unwrap().unwrap();
        assert_eq!(spec.desktop, Some(Os::Macos));
        assert_eq!(spec.dock_size, 64.0);
        // A typo is refused, and so is a style the platform cannot have.
        let typo: Value = serde_json::from_str(r#"{"op":"frame","platfrom":"ios"}"#).unwrap();
        assert!(FrameOpts::from_op(&typo).is_err());
        for bad in [
            r#"{"op":"frame","platform":"macos","style":"recording"}"#,
            r#"{"op":"frame","platform":"ios-simulator","style":"recording"}"#,
            r#"{"op":"frame","platform":"ios","battery":140}"#,
            r#"{"op":"frame","desktop":"on"}"#,
        ] {
            let v: Value = serde_json::from_str(bad).unwrap();
            assert!(
                FrameOpts::from_op(&v).and_then(|o| o.build()).is_err(),
                "{bad} should be refused"
            );
        }
        let none: Value = serde_json::from_str(r#"{"op":"frame","platform":"none"}"#).unwrap();
        assert!(FrameOpts::from_op(&none)
            .unwrap()
            .build()
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_screen_recording_is_the_screen_edge_to_edge() {
        for os in [Os::Ios, Os::Android] {
            let mut spec = Spec::new(os);
            spec.style = Style::Recording;
            let ((cw, ch), _, (ow, oh)) = spec.default_shape().unwrap();
            let g = geometry(&spec, ow, oh, cw, ch);
            assert_eq!(g.bezel, 0);
            assert_eq!(g.body_radius, 0.0);
            // The page spans the full width, and the bars take the rest.
            assert!(
                (g.content.w - ow as i64).abs() <= 2,
                "{os:?}: {:?}",
                g.content
            );
            assert!(g.content.y > 0 && g.content.bottom() < oh as i64);
            assert!(
                g.body.y <= 2 && g.body.bottom() >= oh as i64 - 2,
                "{os:?}: {:?}",
                g.body
            );
        }
    }

    #[test]
    fn a_phone_frame_puts_bezel_and_bars_round_the_content() {
        let spec = Spec::new(Os::Ios);
        let g = geometry(&spec, 1080, 1920, 393, 852);
        assert!(g.bezel > 0);
        assert!(g.content.y - g.body.y > g.bezel, "no room for a status bar");
        assert!(
            g.body.bottom() - g.content.bottom() > g.bezel,
            "no room for the home bar"
        );
        assert_eq!(g.content.x - g.body.x, g.bezel);
        let l = g.layout();
        assert_eq!(l.outline.len(), 1);
        let emu = geometry(&Spec::new(Os::AndroidEmulator), 1920, 1080, 412, 915);
        assert_eq!(
            emu.layout().outline.len(),
            2,
            "the toolbar casts its own shadow"
        );
        let (tb, _) = emu.extra.unwrap();
        assert!(tb.x > emu.body.right(), "toolbar overlaps the handset");
    }

    #[test]
    fn the_chrome_escapes_what_the_page_says() {
        let spec = Spec::new(Os::Macos);
        let page = PageInfo {
            title: "<script>alert(1)</script>".into(),
            url: "https://example.com/a?b=\"c\"".into(),
            ..Default::default()
        };
        let g = geometry(&spec, 1470, 830, 1470, 830);
        let h = html(&spec, &page, &g);
        assert!(!h.contains("<script>"));
        assert!(h.contains("&lt;script&gt;"));
        assert!(h.contains("example.com"));
    }

    #[test]
    fn urls_display_as_an_address_bar_would() {
        assert_eq!(
            display_url("https://www.kaviri.dev/"),
            ("kaviri.dev".into(), "".into())
        );
        assert_eq!(
            display_url("https://kaviri.dev/play/?x=1"),
            ("kaviri.dev".into(), "/play/?x=1".into())
        );
        assert_eq!(
            display_url("file:///home/u/my%20demo.html").1.trim(),
            "my demo.html"
        );
        assert_eq!(host_only("http://127.0.0.1:8099/x"), "127.0.0.1:8099");
    }

    #[test]
    fn icons_and_docks_parse() {
        assert!(matches!(parse_icon("auto"), Ok(Icon::Auto)));
        assert!(matches!(
            parse_icon("terminal"),
            Ok(Icon::Builtin("terminal"))
        ));
        assert!(parse_icon("no-such-icon").is_err());
        assert_eq!(
            parse_dock("mail, chat").unwrap(),
            (
                true,
                Some(vec![DockItem::Builtin("mail"), DockItem::Builtin("chat")])
            )
        );
        assert!(parse_dock("mail,nope").is_err());
        assert_eq!(parse_dock("off").unwrap(), (false, None));
        assert_eq!(parse_dock("on").unwrap(), (true, None));
        // A group expands in place, and a repeat is kept once.
        let (_, dev) = parse_dock("dev,maps,code").unwrap();
        let dev = dev.unwrap();
        assert_eq!(dev.first(), Some(&DockItem::Builtin("files")));
        assert_eq!(dev.last(), Some(&DockItem::Builtin("maps")));
        assert_eq!(
            dev.iter()
                .filter(|i| **i == DockItem::Builtin("code"))
                .count(),
            1
        );
        // A file is a tile of its own.
        let f = std::env::temp_dir().join(format!("kaviri-dock-{}.png", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        let (_, with_file) = parse_dock(&format!("mail,{}", f.display())).unwrap();
        assert_eq!(with_file.unwrap()[1], DockItem::File(f.clone()));
        let _ = std::fs::remove_file(&f);
        // Every built-in icon has its illustration, in both shapes.
        for n in icon_names() {
            for mac in [false, true] {
                let t = art_tile(n, "u", mac).unwrap_or_else(|| panic!("{n} has no art"));
                assert!(
                    !t.contains("\"ID") && !t.contains("#ID"),
                    "{n}: unreplaced id"
                );
            }
        }
        for (n, g) in DOCK_GROUPS {
            for i in g.iter() {
                assert!(
                    builtin_icon(i).is_some(),
                    "group {n} names unknown icon {i}"
                );
            }
        }
        assert!(parse_tint("#abc").is_ok() && parse_tint("7c5cff").is_ok());
        assert!(parse_tint("red\"><script>").is_err());
        assert_eq!(
            parse_desktop("on", Some(Os::IosSimulator)).unwrap(),
            Some(Os::Macos)
        );
        assert_eq!(
            parse_desktop("windows", Some(Os::AndroidEmulator)).unwrap(),
            Some(Os::Windows)
        );
        assert!(parse_desktop("ios", None).is_err());
        for n in icon_names() {
            for (set_name, set, _) in ICON_SETS {
                for mac in [false, true] {
                    let t = tile_svg(n, "t", *set, "#7c5cff", mac);
                    assert!(
                        t.contains("<svg") && t.ends_with("</svg>"),
                        "{n} in {set_name}"
                    );
                }
                assert_eq!(parse_icon_set(set_name).unwrap(), *set);
            }
        }
    }
}
