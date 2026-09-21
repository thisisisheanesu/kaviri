//! The plate a take is composited onto: a wallpaper behind the content, the
//! content inset with rounded corners and a soft drop shadow.
//!
//! The whole backdrop is one RGBA PNG rendered here and handed to ffmpeg as a
//! second input. It is a *frame*, not a background: opaque everywhere except
//! the rounded window the content shows through, so a single `overlay` on top
//! of the padded video gives background, shadow and rounded corners at once.
//! Compositing the other way round, background under the content, would need an
//! alpha channel on the video and a second filter pass to build it.
//!
//! No image crate: the encoder below writes stored-deflate PNGs, which cost a
//! few megabytes of temp file and no dependency at all.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/* How the content sits in the frame. Fractions of the frame's short side, so a
 * 9:16 phone take and a 16:9 desktop take get visually the same inset. */
const PAD_FRAC: f64 = 0.055;
const RADIUS_FRAC: f64 = 0.035;
const RADIUS_MIN: f64 = 10.0;
const RADIUS_MAX: f64 = 56.0;
const SHADOW_BLUR_FRAC: f64 = 0.030;
/// Shadow offset and spread, as fractions of the blur.
const SHADOW_DY: f64 = 0.55;
const SHADOW_SPREAD: f64 = 0.10;
const SHADOW_ALPHA: f64 = 0.42;
/// Dither amplitude in 8-bit levels. Smooth gradients band badly once h264 has
/// had its way with them; a deterministic ±1 level of noise costs nothing and
/// hides the steps.
const DITHER: f64 = 1.3;

// ---------------------------------------------------------------- backgrounds

/// How a background paints. Coordinates are the unit square, so a fill is
/// independent of the output size.
enum Fill {
    /// Multi-stop wash along `angle` degrees (0 = left to right, 90 = top to
    /// bottom). Stops are eased with the same cubic smoothstep the zoom uses,
    /// which is what makes it read as a wash rather than a ramp.
    Linear {
        angle: f64,
        stops: &'static [(f64, [u8; 3])],
    },
    Solid([u8; 3]),
    /// Soft radial blobs over a base colour: (cx, cy, radius, colour).
    Mesh {
        base: [u8; 3],
        blobs: &'static [(f64, f64, f64, [u8; 3])],
    },
}

pub struct Background {
    pub name: &'static str,
    pub about: &'static str,
    fill: Fill,
}

/// The built-in set: five washes, two muted solids, two mesh gradients.
pub const BACKGROUNDS: &[Background] = &[
    Background {
        name: "dusk",
        about: "indigo to violet to magenta wash (the fallback if auto cannot probe)",
        fill: Fill::Linear {
            angle: 115.0,
            stops: &[
                (0.00, [26, 22, 58]),
                (0.45, [72, 45, 120]),
                (0.78, [142, 68, 145]),
                (1.00, [196, 104, 120]),
            ],
        },
    },
    Background {
        name: "dawn",
        about: "peach to rose to lilac, light and warm",
        fill: Fill::Linear {
            angle: 120.0,
            stops: &[
                (0.00, [255, 214, 190]),
                (0.40, [249, 178, 176]),
                (0.75, [214, 160, 208]),
                (1.00, [176, 157, 224]),
            ],
        },
    },
    Background {
        name: "tide",
        about: "deep teal to blue to cyan",
        fill: Fill::Linear {
            angle: 110.0,
            stops: &[
                (0.00, [6, 48, 74]),
                (0.42, [10, 92, 120]),
                (0.75, [24, 120, 150]),
                (1.00, [66, 166, 178]),
            ],
        },
    },
    Background {
        name: "moss",
        about: "forest to olive to sand",
        fill: Fill::Linear {
            angle: 115.0,
            stops: &[
                (0.00, [24, 48, 40]),
                (0.45, [54, 94, 64]),
                (0.80, [104, 140, 84]),
                (1.00, [168, 180, 120]),
            ],
        },
    },
    Background {
        name: "ember",
        about: "oxblood to orange to amber",
        fill: Fill::Linear {
            angle: 115.0,
            stops: &[
                (0.00, [64, 18, 26]),
                (0.40, [150, 48, 38]),
                (0.72, [214, 102, 44]),
                (1.00, [242, 170, 96]),
            ],
        },
    },
    Background {
        name: "slate",
        about: "solid muted blue-grey",
        fill: Fill::Solid([58, 66, 80]),
    },
    Background {
        name: "linen",
        about: "solid warm off-white",
        fill: Fill::Solid([232, 226, 214]),
    },
    Background {
        name: "mesh-cool",
        about: "dark mesh gradient, blue and violet blobs",
        fill: Fill::Mesh {
            base: [18, 24, 52],
            blobs: &[
                (0.18, 0.15, 0.55, [52, 96, 220]),
                (0.85, 0.28, 0.50, [132, 72, 214]),
                (0.55, 0.90, 0.60, [26, 146, 178]),
                (0.08, 0.85, 0.45, [64, 40, 160]),
            ],
        },
    },
    Background {
        name: "mesh-warm",
        about: "light mesh gradient, rose and amber blobs",
        fill: Fill::Mesh {
            base: [244, 232, 224],
            blobs: &[
                (0.15, 0.20, 0.55, [255, 190, 170]),
                (0.88, 0.18, 0.50, [248, 170, 206]),
                (0.60, 0.92, 0.60, [214, 196, 255]),
                (0.05, 0.90, 0.50, [255, 214, 160]),
            ],
        },
    },
];

pub fn background(name: &str) -> Option<&'static Background> {
    BACKGROUNDS.iter().find(|b| b.name == name)
}

pub fn help() -> String {
    let mut s = BACKGROUNDS
        .iter()
        .map(|b| format!("  {:<10} {}", b.name, b.about))
        .collect::<Vec<_>>()
        .join("\n");
    s.push_str("\n  auto       pick one from the recording's dominant colour (default)");
    s.push_str("\n  none       no backdrop: the content fills the frame, as before");
    s
}

/// What the caller asked for on the command line.
#[derive(Clone, Copy)]
pub enum Choice {
    Off,
    Auto,
    Named(&'static Background),
}

pub fn parse_choice(s: &str) -> Result<Choice, String> {
    match s {
        "none" | "off" => Ok(Choice::Off),
        "auto" => Ok(Choice::Auto),
        name => background(name)
            .map(Choice::Named)
            .ok_or_else(|| format!("unknown background: {name}\n\nBackgrounds:\n{}", help())),
    }
}

impl Fill {
    /// Colour at a point of the unit square, 0..1 per channel. `ar` is the
    /// frame's aspect ratio, used to keep mesh blobs round.
    fn sample(&self, u: f64, v: f64, ar: f64) -> [f64; 3] {
        match self {
            Fill::Solid(c) => [c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0],
            Fill::Linear { angle, stops } => {
                let (dx, dy) = (angle.to_radians().cos(), angle.to_radians().sin());
                // Normalize the projection over the unit square's own extent so
                // the first and last stop always land on opposite corners.
                let lo = dx.min(0.0) + dy.min(0.0);
                let hi = dx.max(0.0) + dy.max(0.0);
                let t = if (hi - lo).abs() < 1e-9 {
                    0.0
                } else {
                    ((u * dx + v * dy) - lo) / (hi - lo)
                };
                sample_stops(stops, t.clamp(0.0, 1.0))
            }
            Fill::Mesh { base, blobs } => {
                let mut c = [
                    base[0] as f64 / 255.0,
                    base[1] as f64 / 255.0,
                    base[2] as f64 / 255.0,
                ];
                let (sx, sy) = if ar >= 1.0 { (ar, 1.0) } else { (1.0, 1.0 / ar) };
                for (bx, by, r, col) in *blobs {
                    let dx = (u - bx) * sx;
                    let dy = (v - by) * sy;
                    let d = (dx * dx + dy * dy).sqrt() / r;
                    if d >= 1.0 {
                        continue;
                    }
                    let w = (1.0 - d * d).powi(2);
                    for k in 0..3 {
                        c[k] += (col[k] as f64 / 255.0 - c[k]) * w;
                    }
                }
                c
            }
        }
    }
}

fn sample_stops(stops: &[(f64, [u8; 3])], t: f64) -> [f64; 3] {
    let last = stops.len() - 1;
    if t <= stops[0].0 {
        return rgb01(stops[0].1);
    }
    for i in 0..last {
        let (t0, c0) = stops[i];
        let (t1, c1) = stops[i + 1];
        if t <= t1 {
            let p = if (t1 - t0).abs() < 1e-9 {
                0.0
            } else {
                (t - t0) / (t1 - t0)
            };
            let s = p * p * (3.0 - 2.0 * p); // the same ease the zoom uses
            let (a, b) = (rgb01(c0), rgb01(c1));
            return [
                a[0] + (b[0] - a[0]) * s,
                a[1] + (b[1] - a[1]) * s,
                a[2] + (b[2] - a[2]) * s,
            ];
        }
    }
    rgb01(stops[last].1)
}

fn rgb01(c: [u8; 3]) -> [f64; 3] {
    [
        c[0] as f64 / 255.0,
        c[1] as f64 / 255.0,
        c[2] as f64 / 255.0,
    ]
}

impl Background {
    /// Saturation-weighted mean hue and mean lightness, measured rather than
    /// declared: edit the stops and the auto-picker follows.
    fn key(&self) -> (f64, f64) {
        let (mut sx, mut sy, mut lum, mut n) = (0.0, 0.0, 0.0, 0.0);
        for i in 0..9 {
            for j in 0..9 {
                let (u, v) = ((i as f64 + 0.5) / 9.0, (j as f64 + 0.5) / 9.0);
                let c = self.fill.sample(u, v, 1.0);
                let (h, ch, l) = hue_chroma_lum(c[0], c[1], c[2]);
                let w = ch * ch;
                sx += w * h.to_radians().cos();
                sy += w * h.to_radians().sin();
                lum += l;
                n += 1.0;
            }
        }
        let hue = (sy.atan2(sx).to_degrees() + 360.0) % 360.0;
        (hue, lum / n)
    }

    fn is_solid(&self) -> bool {
        matches!(self.fill, Fill::Solid(_))
    }
}

fn hue_chroma_lum(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let c = max - min;
    let l = (max + min) / 2.0;
    if c <= 1e-9 {
        return (0.0, 0.0, l);
    }
    let h = if max == r {
        60.0 * (((g - b) / c).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / c + 2.0)
    } else {
        60.0 * ((r - g) / c + 4.0)
    };
    ((h + 360.0) % 360.0, c, l)
}

fn hue_dist(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

// ------------------------------------------------------------------- auto pick

/// What the recording looks like, in the two terms the picker cares about.
pub struct Probe {
    /// Dominant hue, absent when the content is essentially colourless.
    pub hue: Option<f64>,
    pub lum: f64,
}

/// Measure the rendered take: average colour and dominant hue.
///
/// ffmpeg is already a hard dependency and it decodes the intermediate far
/// faster than anything this crate could, so the probe is two frames a second
/// scaled to 12x12 and read off a pipe: a few kilobytes for any length of take.
pub fn probe(raw_path: &str, ffmpeg: &str) -> Option<Probe> {
    let out = Command::new(ffmpeg)
        .args([
            "-v", "error", "-nostdin", "-i", raw_path, "-vf",
            "fps=2,scale=12:12:flags=area", "-pix_fmt", "rgb24", "-f", "rawvideo", "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.len() < 3 {
        return None;
    }
    Some(measure(&out.stdout))
}

/// Aggregate raw rgb24 samples into a probe.
///
/// Hue is accumulated as chroma-weighted unit vectors: a page that is mostly
/// white with one brand colour still resolves to the brand colour, and a page
/// with no colour at all resolves to nothing rather than to noise.
fn measure(rgb: &[u8]) -> Probe {
    let (mut sx, mut sy, mut wsum, mut lum, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for px in rgb.as_chunks::<3>().0 {
        let (h, c, l) = hue_chroma_lum(
            px[0] as f64 / 255.0,
            px[1] as f64 / 255.0,
            px[2] as f64 / 255.0,
        );
        let w = c * c;
        sx += w * h.to_radians().cos();
        sy += w * h.to_radians().sin();
        wsum += w;
        lum += l;
        n += 1.0;
    }
    let rms_chroma = (wsum / n).sqrt();
    Probe {
        hue: if rms_chroma < 0.06 {
            None
        } else {
            Some((sy.atan2(sx).to_degrees() + 360.0) % 360.0)
        },
        lum: lum / n,
    }
}

/// Pick a background for a recording.
///
/// The rule, in one line: aim a third of the way round the colour wheel from
/// the content's dominant hue, and prefer a backdrop whose lightness is far
/// from the content's.
///
/// - **Hue.** The target is `content_hue + 150°`, split-complementary rather
///   than the flat 180° opposite, which is the pairing that reads as deliberate
///   instead of as a clash. A backdrop is scored on how close its measured key
///   hue is to that target.
/// - **Lightness.** A backdrop within 0.35 of the content's mean lightness is
///   penalised in proportion, so a white app gets a deep wash and a dark app
///   gets a light one. The content has to pop off the plate.
/// - **Colourless content.** A page with no real colour (rms chroma < 0.06)
///   drops the hue term entirely and is picked on lightness contrast alone.
/// - **Ties.** Solids carry a small constant penalty so a wash wins an
///   otherwise equal contest, and the final tie-break is declaration order.
///   Nothing is random: the same recording always picks the same backdrop, and
///   if the probe fails at all the fallback is the first entry (`dusk`).
pub fn choose(probe: Option<&Probe>) -> &'static Background {
    let probe = match probe {
        Some(p) => p,
        None => return &BACKGROUNDS[0],
    };
    let mut best = &BACKGROUNDS[0];
    let mut best_score = f64::MAX;
    for bg in BACKGROUNDS {
        let (hue, lum) = bg.key();
        let hue_term = match probe.hue {
            Some(h) => hue_dist(hue, (h + 150.0) % 360.0) / 180.0,
            None => 0.0,
        };
        let lum_term = ((0.35 - (lum - probe.lum).abs()).max(0.0)) / 0.35;
        let solid_term = if bg.is_solid() { 0.05 } else { 0.0 };
        let score = 0.6 * hue_term + 0.4 * lum_term + solid_term;
        if score < best_score - 1e-9 {
            best_score = score;
            best = bg;
        }
    }
    best
}

// --------------------------------------------------------------- the plate

pub struct Plate {
    pub path: PathBuf,
    /// Size the content is scaled to, always even.
    pub content: (u32, u32),
    /// Where the content sits in the frame, always even.
    pub origin: (u32, u32),
    pub name: &'static str,
}

fn even(v: f64) -> u32 {
    let n = v.round().max(2.0) as u32;
    n - n % 2
}

/// Fit `aspect` inside the frame with an even inset, centred.
///
/// Even on every axis because the h264 pass downstream is yuv420p, where an odd
/// pad offset is a hard error rather than a rounding difference.
pub fn content_box(out_w: u32, out_h: u32, aspect: f64) -> ((u32, u32), (u32, u32)) {
    let pad = (out_w.min(out_h) as f64 * PAD_FRAC).max(12.0);
    let avail_w = (out_w as f64 - 2.0 * pad).max(16.0);
    let avail_h = (out_h as f64 - 2.0 * pad).max(16.0);
    let aspect = if aspect.is_finite() && aspect > 0.01 {
        aspect
    } else {
        avail_w / avail_h
    };
    let (cw, ch) = if avail_w / avail_h > aspect {
        (avail_h * aspect, avail_h)
    } else {
        (avail_w, avail_w / aspect)
    };
    let cw = even(cw).min(even(out_w as f64));
    let ch = even(ch).min(even(out_h as f64));
    let x = even((out_w - cw) as f64 / 2.0).min(out_w - cw);
    let y = even((out_h - ch) as f64 / 2.0).min(out_h - ch);
    ((cw, ch), (x, y))
}

/// Signed distance to a rounded rect: negative inside, in pixels.
fn sd_round_rect(px: f64, py: f64, cx: f64, cy: f64, hw: f64, hh: f64, r: f64) -> f64 {
    let r = r.min(hw).min(hh).max(0.0);
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let ax = qx.max(0.0);
    let ay = qy.max(0.0);
    (ax * ax + ay * ay).sqrt() + qx.max(qy).min(0.0) - r
}

fn blur_h(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let win = (2 * r + 1) as f32;
    for y in 0..h {
        let base = y * w;
        let mut sum = 0f32;
        for i in 0..2 * r + 1 {
            let x = (i as isize - r as isize).clamp(0, w as isize - 1) as usize;
            sum += src[base + x];
        }
        dst[base] = sum / win;
        for x in 1..w {
            let add = (x + r).min(w - 1);
            let sub = x.saturating_sub(r + 1);
            sum += src[base + add] - src[base + sub];
            dst[base + x] = sum / win;
        }
    }
}

fn blur_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    let win = (2 * r + 1) as f32;
    for x in 0..w {
        let mut sum = 0f32;
        for i in 0..2 * r + 1 {
            let y = (i as isize - r as isize).clamp(0, h as isize - 1) as usize;
            sum += src[y * w + x];
        }
        dst[x] = sum / win;
        for y in 1..h {
            let add = (y + r).min(h - 1);
            let sub = y.saturating_sub(r + 1);
            sum += src[add * w + x] - src[sub * w + x];
            dst[y * w + x] = sum / win;
        }
    }
}

/// Three box blurs is a close enough Gaussian and stays O(pixels).
fn blur(buf: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 || w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![0f32; w * h];
    for _ in 0..3 {
        blur_h(buf, &mut tmp, w, h, r);
        blur_v(&tmp, buf, w, h, r);
    }
}

fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Render the plate for one take and write it next to the intermediate.
pub fn build(
    dir: &Path,
    bg: &'static Background,
    out_w: u32,
    out_h: u32,
    aspect: f64,
) -> Result<Plate, String> {
    let (content, origin) = content_box(out_w, out_h, aspect);
    let (w, h) = (out_w as usize, out_h as usize);
    let (cw, ch) = (content.0 as f64, content.1 as f64);
    let (ox, oy) = (origin.0 as f64, origin.1 as f64);
    let short = out_w.min(out_h) as f64;
    let radius = (cw.min(ch) * RADIUS_FRAC).clamp(RADIUS_MIN, RADIUS_MAX);
    let blur_px = (short * SHADOW_BLUR_FRAC).max(6.0);
    let dy = blur_px * SHADOW_DY;
    let spread = blur_px * SHADOW_SPREAD;

    // The shadow: the same rounded rect, nudged down, spread a little, blurred.
    let mut shadow = vec![0f32; w * h];
    let (scx, scy) = (ox + cw / 2.0, oy + ch / 2.0 + dy);
    for y in 0..h {
        for x in 0..w {
            let d = sd_round_rect(
                x as f64 + 0.5,
                y as f64 + 0.5,
                scx,
                scy,
                cw / 2.0 + spread,
                ch / 2.0 + spread,
                radius + spread,
            );
            shadow[y * w + x] = (0.5 - d).clamp(0.0, 1.0) as f32;
        }
    }
    blur(&mut shadow, w, h, (blur_px / 3.0).round().max(1.0) as usize);

    let ar = out_w as f64 / out_h as f64;
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let c = bg
                .fill
                .sample((x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64, ar);
            let s = shadow[i] as f64 * SHADOW_ALPHA;
            // Alpha is the inverse of the content window's coverage, so the
            // rounded corners are anti-aliased against the real content edge.
            let d = sd_round_rect(
                x as f64 + 0.5,
                y as f64 + 0.5,
                ox + cw / 2.0,
                oy + ch / 2.0,
                cw / 2.0,
                ch / 2.0,
                radius,
            );
            let alpha = 1.0 - (0.5 - d).clamp(0.0, 1.0);
            let n = (hash32(i as u32) as f64 / u32::MAX as f64 - 0.5) * 2.0 * DITHER;
            for k in 0..3 {
                // Shadow darkens towards a blue-black rather than pure black:
                // pure black over a saturated wash goes muddy.
                let lit = c[k] * (1.0 - s) + [0.02, 0.02, 0.04][k] * s;
                px[i * 4 + k] = (lit * 255.0 + n).round().clamp(0.0, 255.0) as u8;
            }
            px[i * 4 + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    let path = dir.join(format!("backdrop-{}.png", bg.name));
    write_png_rgba(&path, out_w, out_h, &px)?;
    Ok(Plate {
        path,
        content,
        origin,
        name: bg.name,
    })
}

// ------------------------------------------------------------------- PNG out

fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in bytes.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Write an 8-bit RGBA PNG. The zlib stream is all stored blocks: bigger on
/// disk than a real deflate, and it keeps this file dependency-free.
pub fn write_png_rgba(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    if rgba.len() != (w as usize) * (h as usize) * 4 {
        return Err("png: pixel buffer is the wrong size".into());
    }
    let mut raw = Vec::with_capacity((h as usize) * (1 + (w as usize) * 4));
    for y in 0..h as usize {
        raw.push(0u8); // filter: none
        let row = y * w as usize * 4;
        raw.extend_from_slice(&rgba[row..row + w as usize * 4]);
    }

    let mut z = vec![0x78u8, 0x01];
    let mut i = 0usize;
    loop {
        let n = (raw.len() - i).min(0xFFFF);
        let last = i + n >= raw.len();
        z.push(if last { 1 } else { 0 });
        z.extend_from_slice(&(n as u16).to_le_bytes());
        z.extend_from_slice(&(!(n as u16)).to_le_bytes());
        z.extend_from_slice(&raw[i..i + n]);
        i += n;
        if last {
            break;
        }
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, truecolour + alpha
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);

    let mut f = std::fs::File::create(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    f.write_all(&png)
        .map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve() {
        assert!(background("dusk").is_some());
        assert!(background("mesh-warm").is_some());
        assert!(background("nope").is_none());
        assert!(matches!(parse_choice("none"), Ok(Choice::Off)));
        assert!(matches!(parse_choice("auto"), Ok(Choice::Auto)));
        assert!(matches!(parse_choice("tide"), Ok(Choice::Named(_))));
        assert!(parse_choice("chartreuse").is_err());
        // Every name in the help text is a name you can pass.
        for b in BACKGROUNDS {
            assert!(help().contains(b.name), "{} missing from help", b.name);
        }
    }

    #[test]
    fn gradient_runs_corner_to_corner() {
        // dusk is a 115 degree wash: down and a little to the left, so it
        // starts in the top-right corner and ends in the bottom-left one.
        let bg = background("dusk").unwrap();
        let a = bg.fill.sample(1.0, 0.0, 1.0);
        let b = bg.fill.sample(0.0, 1.0, 1.0);
        // The extreme corners land exactly on the first and last stop, which is
        // what normalising the projection over the unit square buys.
        assert!((a[0] - 26.0 / 255.0).abs() < 1e-9, "start is not the first stop: {a:?}");
        assert!((b[0] - 196.0 / 255.0).abs() < 1e-9, "end is not the last stop: {b:?}");
        assert!(a[0] < 0.2 && a[2] < 0.3, "start too light: {a:?}");
        assert!(b[0] > 0.6, "end too dark: {b:?}");
        // and it climbs through the middle rather than jumping at a stop
        let mid = bg.fill.sample(0.5, 0.5, 1.0);
        assert!(mid[0] > a[0] && mid[0] < b[0], "mid off the ramp: {mid:?}");
    }

    #[test]
    fn measured_keys_match_the_eye() {
        let (_, lum) = background("linen").unwrap().key();
        assert!(lum > 0.8, "linen should be light, got {lum}");
        let (_, lum) = background("slate").unwrap().key();
        assert!(lum < 0.4, "slate should be dark, got {lum}");
        let (hue, _) = background("tide").unwrap().key();
        assert!((150.0..=230.0).contains(&hue), "tide should be cyan-ish: {hue}");
        let (hue, _) = background("ember").unwrap().key();
        assert!(hue < 60.0 || hue > 330.0, "ember should be warm: {hue}");
    }

    #[test]
    fn auto_pick_is_complementary_and_deterministic() {
        // A light page with a green brand colour.
        let green = Probe { hue: Some(140.0), lum: 0.85 };
        let a = choose(Some(&green));
        let b = choose(Some(&green));
        assert_eq!(a.name, b.name, "auto pick must be deterministic");
        let (hue, lum) = a.key();
        assert!(lum < 0.6, "a light page wants a deep backdrop, got {lum}");
        assert!(
            hue_dist(hue, 290.0) < 70.0,
            "{} is not complementary to green (hue {hue})",
            a.name
        );
        // A colourless page still gets a backdrop, chosen on lightness alone.
        let plain = Probe { hue: None, lum: 0.93 };
        let p = choose(Some(&plain));
        assert!(p.key().1 < 0.5, "{} is too light for a white page", p.name);
        // No probe at all: the documented fallback.
        assert_eq!(choose(None).name, BACKGROUNDS[0].name);
    }

    #[test]
    fn probe_reads_raw_rgb() {
        // Mostly white with a few strong blue pixels: hue should find the blue.
        let mut buf = vec![250u8; 300];
        for i in 0..20 {
            buf[i * 3..i * 3 + 3].copy_from_slice(&[20, 60, 220]);
        }
        let p = measure(&buf);
        assert!(p.lum > 0.5);
        let h = p.hue.expect("blue should register");
        assert!((200.0..=260.0).contains(&h), "expected blue, got {h}");
        // Pure grey has no hue at all.
        assert!(measure(&[128u8; 300]).hue.is_none());
    }

    #[test]
    fn content_box_is_inset_even_and_keeps_aspect() {
        for (w, h) in [(1080u32, 1920u32), (1470, 830), (1080, 1080), (1920, 1080)] {
            let aspect = w as f64 / h as f64;
            let ((cw, ch), (x, y)) = content_box(w, h, aspect);
            assert_eq!((cw % 2, ch % 2, x % 2, y % 2), (0, 0, 0, 0), "{w}x{h} not even");
            assert!(cw < w && ch < h, "{w}x{h}: content not inset");
            assert!(x + cw <= w && y + ch <= h, "{w}x{h}: content off-frame");
            // centred within a pixel of rounding
            assert!((x as i64 - (w - cw - x) as i64).abs() <= 2);
            assert!((y as i64 - (h - ch - y) as i64).abs() <= 2);
            let got = cw as f64 / ch as f64;
            assert!((got - aspect).abs() / aspect < 0.01, "{w}x{h}: aspect {got}");
        }
        // A capture that is a different shape from the frame is letterboxed,
        // not stretched.
        let ((cw, ch), _) = content_box(1080, 1080, 16.0 / 9.0);
        assert!((cw as f64 / ch as f64 - 16.0 / 9.0).abs() < 0.02);
        assert!(ch < cw);
    }

    #[test]
    fn png_is_well_formed() {
        let dir = std::env::temp_dir().join("lensa-png-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.png");
        let (w, h) = (7u32, 5u32);
        write_png_rgba(&path, w, h, &vec![9u8; (w * h * 4) as usize]).unwrap();
        let b = std::fs::read(&path).unwrap();
        assert_eq!(&b[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // Walk the chunks and check every length and CRC.
        let mut i = 8usize;
        let mut kinds: Vec<String> = Vec::new();
        while i + 12 <= b.len() {
            let len = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            let kind = String::from_utf8_lossy(&b[i + 4..i + 8]).to_string();
            let crc = u32::from_be_bytes(b[i + 8 + len..i + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&b[i + 4..i + 8 + len]), "bad CRC on {kind}");
            kinds.push(kind);
            i += 12 + len;
        }
        assert_eq!(i, b.len(), "trailing bytes after the last chunk");
        assert_eq!(kinds, vec!["IHDR", "IDAT", "IEND"]);
        assert_eq!(u32::from_be_bytes(b[16..20].try_into().unwrap()), w);
        assert_eq!(u32::from_be_bytes(b[20..24].try_into().unwrap()), h);
        assert_eq!(&b[24..29], &[8, 6, 0, 0, 0]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plate_covers_the_frame_and_opens_a_window() {
        let dir = std::env::temp_dir().join("lensa-plate-test");
        std::fs::create_dir_all(&dir).unwrap();
        let (w, h) = (320u32, 200u32);
        let bg = background("tide").unwrap();
        let plate = build(&dir, bg, w, h, w as f64 / h as f64).unwrap();
        assert!(plate.path.exists());
        assert!(plate.content.0 < w && plate.origin.0 > 0);
        // Re-render the alpha the same way build does and check the two states.
        let ((cw, ch), (ox, oy)) = content_box(w, h, w as f64 / h as f64);
        let mid = sd_round_rect(
            (ox + cw / 2) as f64,
            (oy + ch / 2) as f64,
            ox as f64 + cw as f64 / 2.0,
            oy as f64 + ch as f64 / 2.0,
            cw as f64 / 2.0,
            ch as f64 / 2.0,
            12.0,
        );
        assert!(mid < 0.0, "centre of the content window must be inside");
        let corner = sd_round_rect(
            1.0, 1.0,
            ox as f64 + cw as f64 / 2.0,
            oy as f64 + ch as f64 / 2.0,
            cw as f64 / 2.0,
            ch as f64 / 2.0,
            12.0,
        );
        assert!(corner > 0.0, "frame corner must be outside the window");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
