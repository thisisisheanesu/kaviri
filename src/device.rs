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
    pub dock: Option<Vec<&'static str>>,
    /// The title the emulators show for the handset.
    pub device_name: Option<String>,
    pub clock: Option<String>,
    pub battery: u8,
}

impl Spec {
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
        o => Err(format!("--frame-style must be browser or app, got {o}")),
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

pub fn parse_dock(s: &str) -> Result<Vec<&'static str>, String> {
    if s == "none" || s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|n| {
            builtin_icon(n.trim()).map(|b| b.0).ok_or_else(|| {
                format!(
                    "--dock: unknown icon {n}\n\nIcons: {}",
                    icon_names().join(", ")
                )
            })
        })
        .collect()
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

pub fn help() -> String {
    let mut s = FRAMES
        .iter()
        .map(|f| format!("  {:<17} {}", f.0, f.2))
        .collect::<Vec<_>>()
        .join("\n");
    s.push_str("\n  none              no frame (default)");
    s.push_str("\n\nIcons (--frame-icon, --dock):\n  ");
    s.push_str(&icon_names().join(", "));
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
    title: document.title || '', url: location.href, icon: data || icon || '',
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
}

impl Geometry {
    pub fn layout(&self) -> Layout {
        let mut outline = vec![rf(self.body, self.body_radius)];
        if let Some((r, rad)) = self.extra {
            outline.push(rf(r, rad));
        }
        Layout {
            content: (self.content.w as u32, self.content.h as u32),
            origin: (self.content.x as u32, self.content.y as u32),
            outline,
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

/// Desktop bars, in points: (top, bottom).
fn desktop_bars(d: Os) -> (f64, f64) {
    match d {
        Os::Windows => (0.0, 48.0),
        Os::Linux => (32.0, 76.0),
        _ => (25.0, 76.0),
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
    let (top_bar, bottom_bar) = match spec.desktop {
        Some(d) => {
            let (t, b) = desktop_bars(d);
            ((t * s).round() as i64, (b * s).round() as i64)
        }
        None => (0, 0),
    };
    let short = ow.min(oh);
    let pad = (short * if spec.desktop.is_some() { 0.05 } else { 0.055 }).max(12.0);
    let avail_x = pad;
    let avail_y = top_bar as f64 + pad;
    let avail_w = (ow - 2.0 * pad).max(16.0);
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
    let cx = even_i(bx + il as f64).max(0);
    let cy = even_i(by + ext as f64 + it as f64).max(0);
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

/// A built-in icon as a rounded, gradient app tile.
fn tile_svg(name: &str, uid: &str) -> String {
    let (n, a, b, glyph) = builtin_icon(name).copied().unwrap_or(ICONS[0]);
    format!(
        r##"<svg viewBox="0 0 64 64" width="100%" height="100%"><defs><linearGradient id="g{uid}{n}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{a}"/><stop offset="1" stop-color="{b}"/></linearGradient></defs><rect x="2" y="2" width="60" height="60" rx="14" fill="url(#g{uid}{n})"/><rect x="2.5" y="2.5" width="59" height="59" rx="13.5" fill="none" stroke="rgba(255,255,255,.25)"/><g transform="translate(12 12) scale(1.6667)" fill="none" stroke="#fff" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{glyph}</g></svg>"##
    )
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
            r#"<img class="ico" src="{}" style="width:{px}px;height:{px}px;object-fit:contain">"#,
            esc(src)
        ),
        AppIcon::Letter(c) => format!(
            r#"<span class="ico" style="width:{px}px;height:{px}px;border-radius:50%;background:{accent};color:#fff;font-size:{}px;font-weight:700;display:inline-flex;align-items:center;justify-content:center">{}</span>"#,
            px * 0.62,
            esc(&c.to_string())
        ),
    }
}

/// Large icon for a dock or taskbar, `px` points square.
fn big_icon(icon: &AppIcon, px: f64, uid: &str) -> String {
    match icon {
        AppIcon::None => String::new(),
        AppIcon::Builtin(n) => format!(
            r#"<span style="width:{px}px;height:{px}px;display:block">{}</span>"#,
            tile_svg(n, uid)
        ),
        AppIcon::Image(src) => format!(
            r#"<span style="width:{px}px;height:{px}px;display:flex;align-items:center;justify-content:center;background:#fff;border-radius:{}px;box-shadow:inset 0 0 0 1px rgba(0,0,0,.08)"><img src="{}" style="width:70%;height:70%;object-fit:contain"></span>"#,
            px * 0.225,
            esc(src)
        ),
        AppIcon::Letter(c) => format!(
            r#"<span style="width:{px}px;height:{px}px;display:flex;align-items:center;justify-content:center;background:linear-gradient(#5b6cff,#3a3fd0);color:#fff;border-radius:{}px;font-weight:700;font-size:{}px">{}</span>"#,
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

fn ios_status(page: &PageInfo, clock: &str, battery: u8) -> String {
    let fg = fg_on(page.top_dark);
    format!(
        r#"<div style="height:100%;background:{bg};position:relative;font:600 17px {FONT};color:{fg};letter-spacing:-.2px">
<div style="position:absolute;left:0;top:0;width:36%;height:100%;display:flex;align-items:center;justify-content:center;padding:4px 0 0 12px;box-sizing:border-box">{clock}</div>
<div style="position:absolute;left:50%;top:11px;width:124px;height:36px;margin-left:-62px;border-radius:18px;background:#000"></div>
<div style="position:absolute;right:0;top:0;width:36%;height:100%;display:flex;align-items:center;justify-content:center;gap:6px;padding:4px 14px 0 0;box-sizing:border-box">{sig}{wifi}{bat}</div></div>"#,
        bg = page.top_bg,
        clock = esc(clock),
        sig = signal_bars(fg),
        wifi = wifi(fg, 17.0),
        bat = battery_ios(battery, fg),
    )
}

fn android_status(page: &PageInfo, clock: &str, battery: u8) -> String {
    let fg = fg_on(page.top_dark);
    let level = 13.0 * battery.min(100) as f64 / 100.0;
    format!(
        r#"<div style="height:100%;background:{bg};position:relative;font:500 14px Roboto,{FONT};color:{fg}">
<div style="position:absolute;left:22px;top:0;height:100%;display:flex;align-items:center">{clock}</div>
<div style="position:absolute;left:50%;top:9px;width:20px;height:20px;margin-left:-10px;border-radius:50%;background:#050505;box-shadow:0 0 0 1.5px rgba(128,128,128,.25)"></div>
<div style="position:absolute;right:20px;top:0;height:100%;display:flex;align-items:center;gap:6px">{wifi}<svg width="14" height="14" viewBox="0 0 14 14" style="display:block" fill="{fg}"><path d="M14 0v14H0z"/></svg><svg width="9" height="15" viewBox="0 0 9 15" style="display:block"><rect x="2.8" y="0" width="3.4" height="2" rx=".5" fill="{fg}"/><rect x=".75" y="1.75" width="7.5" height="12.5" rx="1.4" fill="none" stroke="{fg}" stroke-width="1.3"/><rect x="2" y="{ly}" width="5" height="{level}" rx=".5" fill="{fg}"/></svg></div></div>"#,
        bg = page.top_bg,
        clock = esc(clock),
        wifi = wifi(fg, 15.0),
        ly = 14.0 - level - 0.3,
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
        ("rgba(30,30,32,.55)", "#ffffff")
    } else {
        ("rgba(246,246,248,.62)", "#111111")
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

/// The dock (macOS, GNOME) as a centred shelf of tiles.
fn dock(
    icons: &[&'static str],
    app: Option<&AppIcon>,
    active_builtin: Option<&str>,
    dark: bool,
) -> (String, f64) {
    let size = 52.0;
    let gap = 8.0;
    let mut tiles = String::new();
    let mut n = 0.0;
    let dot = r#"<span style="position:absolute;left:50%;bottom:-7px;width:4px;height:4px;margin-left:-2px;border-radius:50%;background:rgba(255,255,255,.9)"></span>"#;
    for (i, name) in icons.iter().enumerate() {
        let on = active_builtin == Some(*name);
        tiles.push_str(&format!(
            r#"<span style="position:relative;display:block;flex:none">{}{}</span>"#,
            big_icon(&AppIcon::Builtin(name), size, &format!("d{i}")),
            if on { dot } else { "" }
        ));
        n += 1.0;
    }
    if let Some(a) = app {
        if !matches!(a, AppIcon::None) {
            tiles.push_str(r#"<span style="width:1px;height:44px;background:rgba(255,255,255,.35);flex:none;margin:0 2px"></span>"#);
            tiles.push_str(&format!(
                r#"<span style="position:relative;display:block;flex:none">{}{dot}</span>"#,
                big_icon(a, size, "app")
            ));
            n += 1.0;
        }
    }
    let bg = if dark {
        "rgba(40,40,44,.55)"
    } else {
        "rgba(255,255,255,.28)"
    };
    let width = n * size + (n - 1.0).max(0.0) * gap + 24.0 + if app.is_some() { 13.0 } else { 0.0 };
    (
        format!(
            r#"<div style="height:68px;background:{bg};border-radius:20px;box-shadow:inset 0 0 0 1px rgba(255,255,255,.35),0 8px 24px rgba(0,0,0,.18);display:flex;align-items:center;gap:{gap}px;padding:0 12px;box-sizing:border-box;width:100%">{tiles}</div>"#
        ),
        width,
    )
}

fn windows_taskbar(
    icons: &[&'static str],
    app: &AppIcon,
    active_builtin: Option<&str>,
    clock: &str,
    dark: bool,
) -> String {
    let (bg, fg) = if dark {
        ("rgba(32,32,32,.86)", "#ffffff")
    } else {
        ("rgba(238,241,246,.86)", "#111111")
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
    for (i, n) in icons.iter().enumerate() {
        row.push_str(&slot(
            big_icon(&AppIcon::Builtin(n), 26.0, &format!("t{i}")),
            active_builtin == Some(*n),
        ));
    }
    if !matches!(app, AppIcon::None) {
        row.push_str(&slot(big_icon(app, 26.0, "tapp"), true));
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
        let r_in = (g.body_radius - g.bezel as f64).max(0.0);
        parts.push(part(
            status,
            unit,
            &format!("border-radius:{r_in}px {r_in}px 0 0"),
            &if spec.os.is_ios() {
                ios_status(page, &clock, spec.battery)
            } else {
                android_status(page, &clock, spec.battery)
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
        // The bezel last, so its inner curve rounds off the screen's corners.
        let ring = (2.0 * unit).max(1.0);
        parts.push(format!(
            r#"<div style="position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;box-sizing:border-box;border:{}px solid #0b0b0c;border-radius:{}px;box-shadow:0 0 0 {ring}px #3a3a3e, inset 0 0 0 {}px #1d1d20"></div>"#,
            b.x,
            b.y,
            b.w,
            b.h,
            g.bezel,
            g.body_radius,
            (1.0 * unit).max(1.0),
        ));
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

    if let Some(d) = spec.desktop {
        let s = g.s;
        let (ow, oh) = (g.out.0 as i64, g.out.1 as i64);
        let icons = spec.dock.clone().unwrap_or_else(|| default_dock(d));
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
        match d {
            Os::Windows => {
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
                    ),
                ));
            }
            _ => {
                if d == Os::Macos {
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
                        &mac_menubar(&app_name, &clock, dark),
                    ));
                } else {
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
                let (html, width_pt) = dock(&icons, app_tile, active, d == Os::Linux || dark);
                let w = (width_pt * s).round() as i64;
                let h = (68.0 * s).round() as i64;
                parts.push(part(
                    Rect {
                        x: (ow - w) / 2,
                        y: oh - h - (4.0 * s).round() as i64,
                        w,
                        h,
                    },
                    s,
                    "overflow:visible",
                    &html,
                ));
            }
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
        assert_eq!(parse_dock("mail, chat").unwrap(), vec!["mail", "chat"]);
        assert!(parse_dock("mail,nope").is_err());
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
            assert!(tile_svg(n, "t").contains("<svg"));
        }
    }
}
