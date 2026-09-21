//! Telemetry -> zoom timeline -> final MP4.
//!
//! Port of the proven Python zoomcut approach:
//! - cubic smoothstep s = p*p*(3-2p) for every ease
//! - zoom events {t, end, cx, cy, z} with optional path waypoints for
//!   follow-pans between consecutive nearby interactions
//! - EASE = 0.7s, z in 1.5..1.9, crop clamped to the frame
//! - VFR frames are normalized to CFR 30fps BEFORE any time-based math
//!   (pass 1), then a zoompan expression does the zooms (pass 2).

use crate::backdrop::{self, Choice, Plate};
use crate::cdp::FrameSpool;
use crate::ops::Mark;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const EASE: f64 = 0.7;
pub const FPS: u32 = 30;
/// How far left of an interaction to sit the crop, as a fraction of the cropped width.
///
/// Centring on the thing being interacted with is the obvious choice and the wrong one. A
/// control sits to the RIGHT of whatever it acts on: the send button after the message, the
/// caret after the words already typed. Centre on the control and the frame fills with empty
/// space on its right while the thing you wanted to read falls off the left. Sitting the crop
/// a little left of the target keeps both. Typing leans further because a line of text grows
/// away to the right as it is written.
const LEFT_BIAS_TYPE: f64 = 0.18;
const LEFT_BIAS_CLICK: f64 = 0.12;
/// Never lean so far that the target itself leaves the frame.
const KEEP_IN_FRAME: f64 = 0.06;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ZoomEvent {
    pub t: f64,
    pub end: f64,
    pub cx: f64,
    pub cy: f64,
    pub z: f64,
    /// Waypoints (absolute t, cx, cy) the view pans through while zoomed.
    pub path: Vec<(f64, f64, f64)>,
}

/// Build zoom events from interaction marks.
/// `scale` converts CSS px -> source video px. Coordinates out are source px.
pub fn events_from_marks(
    marks: &[Mark],
    scale: f64,
    frame_w: f64,
    frame_h: f64,
    duration: f64,
) -> Vec<ZoomEvent> {
    // Interaction targets: marks that carry a bounding box.
    struct Target {
        t: f64,
        cx: f64,
        cy: f64,
        z: f64,
    }
    let targets: Vec<Target> = marks
        .iter()
        .filter(|m| matches!(m.kind.as_str(), "click" | "type"))
        .filter_map(|m| {
            let (x, y, w, h) = m.bbox?;
            let cy = (y + h / 2.0) * scale;
            // Bigger targets get gentler zoom.
            let z = if h * scale > frame_h * 0.45 {
                1.5
            } else if h * scale > frame_h * 0.25 {
                1.7
            } else {
                1.85
            };
            let crop_w = frame_w / z;
            let want = if m.kind == "type" { LEFT_BIAS_TYPE } else { LEFT_BIAS_CLICK };
            // Cap the lean so the target's own right edge stays comfortably inside the crop.
            let room = 0.5 - (w * scale / 2.0) / crop_w - KEEP_IN_FRAME;
            let bias = want.min(room.max(0.0));
            let cx = (x + w / 2.0) * scale - crop_w * bias;
            Some(Target {
                t: m.t,
                cx: cx.clamp(0.0, frame_w),
                cy: cy.clamp(0.0, frame_h),
                z,
            })
        })
        .collect();

    let mut events: Vec<ZoomEvent> = Vec::new();
    let hold_after = 2.1; // seconds to stay zoomed after an interaction
    let lead_in = 0.45; // start easing before the interaction moment
    let merge_gap = 1.3; // if the next interaction starts within this of our end, keep following

    let mut i = 0;
    while i < targets.len() {
        let first = &targets[i];
        let mut ev = ZoomEvent {
            t: (first.t - lead_in).max(0.0),
            end: first.t + hold_after,
            cx: first.cx,
            cy: first.cy,
            z: first.z,
            path: Vec::new(),
        };
        let mut j = i + 1;
        while j < targets.len() && targets[j].t < ev.end + merge_gap {
            let next = &targets[j];
            let dist =
                ((next.cx - ev.cx).powi(2) + (next.cy - ev.cy).powi(2)).sqrt();
            let last_wp_t = ev.path.last().map(|p| p.0).unwrap_or(ev.t + EASE);
            let wp_t = next.t.max(last_wp_t + 0.15);
            if dist > 30.0 {
                ev.path.push((wp_t, next.cx, next.cy));
            }
            ev.end = wp_t + hold_after;
            ev.z = ev.z.min(next.z); // never tighter than the loosest merged target
            j += 1;
        }
        // Clamp into the recording and keep waypoints inside the eased window.
        ev.end = ev.end.min(duration - 0.05);
        ev.path
            .retain(|p| p.0 > ev.t + EASE && p.0 < ev.end - EASE);
        if ev.end - ev.t >= 2.0 * EASE + 0.15 {
            events.push(ev);
        }
        i = j;
    }
    events
}

fn smooth(a: &str, b: &str, t0: f64, t1: f64) -> String {
    let p = format!("clip((it-{t0:.3})/({:.3}),0,1)", t1 - t0);
    let s = format!("({p}*{p}*(3-2*{p}))");
    format!("({a}+({b}-{a})*{s})")
}

/// Piecewise ffmpeg expression for z, cx or cy over time (direct port of
/// the proven Python generator).
pub fn build_expr(events: &[ZoomEvent], which: &str) -> String {
    let base = match which {
        "z" => "1",
        "cx" => "iw/2",
        _ => "ih/2",
    };
    let mut sorted: Vec<&ZoomEvent> = events.iter().collect();
    sorted.sort_by(|a, b| b.t.partial_cmp(&a.t).unwrap());
    let mut expr = base.to_string();
    for ev in sorted {
        let (t0, t1) = (ev.t, ev.end);
        let inner = if which == "z" || ev.path.is_empty() {
            let v = match which {
                "z" => format!("{:.4}", ev.z),
                "cx" => format!("{:.2}", ev.cx),
                _ => format!("{:.2}", ev.cy),
            };
            let ease_in = smooth(base, &v, t0, t0 + EASE);
            let ease_out = smooth(&v, base, t1 - EASE, t1);
            format!(
                "if(lt(it,{:.3}),{ease_in},if(lt(it,{:.3}),{v},{ease_out}))",
                t0 + EASE,
                t1 - EASE
            )
        } else {
            // Pan through waypoints while zoomed.
            let mut pts: Vec<(f64, f64)> = vec![(
                t0,
                if which == "cx" { ev.cx } else { ev.cy },
            )];
            for p in &ev.path {
                pts.push((p.0, if which == "cx" { p.1 } else { p.2 }));
            }
            let v_first = format!("{:.2}", pts[0].1);
            let v_last = format!("{:.2}", pts[pts.len() - 1].1);
            let ease_in = smooth(base, &v_first, t0, t0 + EASE);
            let ease_out = smooth(&v_last, base, t1 - EASE, t1);
            let mut mid = v_last.clone();
            for k in (0..pts.len() - 1).rev() {
                let (ta, va) = pts[k];
                let (tb, vb) = pts[k + 1];
                let seg = smooth(&format!("{va:.2}"), &format!("{vb:.2}"), ta, tb);
                mid = format!("if(lt(it,{tb:.3}),{seg},{mid})");
            }
            format!(
                "if(lt(it,{:.3}),{ease_in},if(lt(it,{:.3}),{mid},{ease_out}))",
                t0 + EASE,
                t1 - EASE
            )
        };
        expr = format!("if(between(it,{t0:.3},{t1:.3}),{inner},{expr})");
    }
    expr
}

pub fn find_ffmpeg() -> Result<String, String> {
    if let Ok(p) = std::env::var("LENSA_FFMPEG") {
        return Ok(p);
    }
    for cand in ["ffmpeg"] {
        if Command::new(cand)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(cand.to_string());
        }
    }
    // Common no-sudo static install location.
    if let Some(home) = std::env::var_os("HOME") {
        let p = std::path::Path::new(&home).join(".local/bin/ffmpeg");
        if p.exists() {
            return Ok(p.display().to_string());
        }
    }
    Err("ffmpeg not found (install a static build or set LENSA_FFMPEG)".into())
}

/// Pass 1: VFR screencast frames -> CFR 30fps H.264 intermediate.
fn render_cfr(spool: &FrameSpool, raw_path: &str, ffmpeg: &str) -> Result<f64, String> {
    let frames = spool.frames();
    if frames.is_empty() {
        return Err("no frames captured".into());
    }
    let t_last = frames.last().unwrap().t + 0.4;
    let mut child = Command::new(ffmpeg)
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "image2pipe",
            "-framerate",
            &FPS.to_string(),
            "-i",
            "-",
            "-vf",
            "crop=iw-mod(iw\\,2):ih-mod(ih\\,2)",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            raw_path,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;
    {
        let stdin = child.stdin.as_mut().unwrap();
        let n_ticks = (t_last * FPS as f64).ceil() as usize;
        let mut idx = 0usize;
        /*
         * One frame is held at a time. A static stretch emits the same JPEG for many
         * ticks, so it is read off the spool once and reused, which keeps this pass at
         * O(one frame) of memory however long the take is.
         */
        let mut held = spool.read(0)?;
        let mut held_idx = 0usize;
        for n in 0..n_ticks {
            let t = n as f64 / FPS as f64;
            while idx + 1 < frames.len() && frames[idx + 1].t <= t {
                idx += 1;
            }
            if idx != held_idx {
                held = spool.read(idx)?;
                held_idx = idx;
            }
            stdin.write_all(&held).map_err(|e| format!("write frame: {e}"))?;
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("ffmpeg pass 1 (CFR normalize) failed".into());
    }
    Ok(t_last)
}

/// Pass 2: zoompan with generated expressions, composited onto the backdrop
/// plate if there is one -> final MP4.
fn render_zoom(
    raw_path: &str,
    out_path: &str,
    events: &[ZoomEvent],
    out_w: u32,
    out_h: u32,
    plate: Option<&Plate>,
    ffmpeg: &str,
) -> Result<(), String> {
    /*
     * With a backdrop the content is no longer the frame: it is scaled to the
     * plate's window instead, padded out to the frame, and the plate is laid
     * over it. The plate is opaque everywhere except that window, so one
     * overlay draws background, drop shadow and rounded corners together.
     */
    let (cw, ch) = plate.map(|p| p.content).unwrap_or((out_w, out_h));
    let core = if events.is_empty() {
        format!("fps={FPS},scale={cw}:{ch}")
    } else {
        let z = build_expr(events, "z");
        let cx = build_expr(events, "cx");
        let cy = build_expr(events, "cy");
        format!(
            "fps={FPS},zoompan=z='({z})':\
             x='clip(({cx})-iw/(2*({z})),0,iw-iw/({z}))':\
             y='clip(({cy})-ih/(2*({z})),0,ih-ih/({z}))':\
             d=1:fps={FPS}:s={cw}x{ch}"
        )
    };

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        raw_path.into(),
    ];
    match plate {
        Some(p) => {
            let (x, y) = p.origin;
            args.push("-i".into());
            args.push(p.path.display().to_string());
            args.push("-filter_complex".into());
            args.push(format!(
                "[0:v]{core},format=rgba,pad={out_w}:{out_h}:{x}:{y}[c];\
                 [c][1:v]overlay=0:0:format=auto,format=yuv420p[v]"
            ));
            args.push("-map".into());
            args.push("[v]".into());
        }
        None => {
            args.push("-filter:v".into());
            args.push(format!("{core},format=yuv420p"));
        }
    }
    args.extend(
        [
            "-c:v", "libx264", "-preset", "medium", "-crf", "19", "-movflags", "+faststart",
            out_path,
        ]
        .iter()
        .map(|s| s.to_string()),
    );

    let status = Command::new(ffmpeg)
        .args(&args)
        .status()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;
    if !status.success() {
        return Err("ffmpeg pass 2 (zoompan) failed".into());
    }
    Ok(())
}

/// Resolve `--background` into a rendered plate.
///
/// A backdrop is decoration, so nothing here is fatal: a failed probe falls
/// back to the deterministic default, and a failed render falls back to the
/// full-frame take that lensa produced before backdrops existed.
fn plate_for(
    choice: Choice,
    tmp_dir: &Path,
    raw_path: &str,
    ffmpeg: &str,
    out_w: u32,
    out_h: u32,
    aspect: f64,
) -> Option<Plate> {
    let (bg, how) = match choice {
        Choice::Off => return None,
        Choice::Named(bg) => (bg, String::from("--background")),
        Choice::Auto => {
            let probe = backdrop::probe(raw_path, ffmpeg);
            let how = match probe.as_ref() {
                Some(p) => match p.hue {
                    Some(h) => format!("auto, content hue {h:.0}\u{b0} / lightness {:.2}", p.lum),
                    None => format!("auto, colourless content / lightness {:.2}", p.lum),
                },
                None => "auto, probe failed - default".to_string(),
            };
            (backdrop::choose(probe.as_ref()), how)
        }
    };
    match backdrop::build(tmp_dir, bg, out_w, out_h, aspect) {
        Ok(p) => {
            eprintln!(
                "lensa: backdrop {} ({how}), content {}x{} at {},{}",
                p.name, p.content.0, p.content.1, p.origin.0, p.origin.1
            );
            Some(p)
        }
        Err(e) => {
            eprintln!("lensa: backdrop unavailable ({e}); rendering full-frame");
            None
        }
    }
}

/// Full pipeline: frames + marks -> zoomed MP4. Returns (duration, n_events).
// Every one of these is a genuinely independent knob on the take, and the three
// sizes are deliberately not collapsed into one; a struct here would only move
// the list somewhere else.
#[allow(clippy::too_many_arguments)]
pub fn render(
    spool: &FrameSpool,
    marks: &[Mark],
    out_path: &str,
    css_w: u32,
    css_h: u32,
    /*
     * The video's own size, which is not the viewport's. A phone-shaped take wants a
     * 432px viewport so the site lays out like a phone, and a 1080px video so it is not
     * a postage stamp on the platform it is going to. Capture happens between the two:
     * css * scale pixels, cropped by the zoom, then resampled to this.
     */
    out_size: (u32, u32),
    /* The backdrop the content is composited onto, if any. */
    background: Choice,
    keep_temp: bool,
) -> Result<(f64, usize), String> {
    let ffmpeg = find_ffmpeg()?;
    if spool.is_empty() {
        return Err("no frames captured".into());
    }
    let (fw, fh) = crate::cdp::jpeg_dims(&spool.read(0)?)
        .ok_or("could not parse first frame's JPEG header")?;
    let fw = fw - fw % 2;
    let fh = fh - fh % 2;
    let scale = fw as f64 / css_w as f64;
    let out_w = out_size.0 - out_size.0 % 2;
    let out_h = out_size.1 - out_size.1 % 2;

    let tmp_dir = std::path::Path::new(out_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ".".into())
        .join(".lensa-tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let raw_path = tmp_dir.join("raw.mp4").display().to_string();

    eprintln!(
        "lensa: viewport {css_w}x{css_h}, capture {fw}x{fh} ({scale:.2}x), output {out_w}x{out_h}"
    );

    let duration = render_cfr(spool, &raw_path, &ffmpeg)?;
    let events = events_from_marks(marks, scale, fw as f64, fh as f64, duration);
    /*
     * After pass 1, not before: the auto picker measures the take itself, and
     * the CFR intermediate is the only place the captured pixels exist in a
     * form ffmpeg can read cheaply.
     */
    let plate = plate_for(
        background,
        &tmp_dir,
        &raw_path,
        &ffmpeg,
        out_w,
        out_h,
        fw as f64 / fh as f64,
    );

    // Telemetry sidecar for debugging / re-rendering.
    let sidecar = format!("{out_path}.telemetry.json");
    let telemetry = serde_json::json!({
        "duration": duration,
        "frame_size": [fw, fh],
        "css_size": [css_w, css_h],
        "scale": scale,
        "background": plate.as_ref().map(|p| p.name),
        "content_box": plate.as_ref().map(|p| vec![p.origin.0, p.origin.1, p.content.0, p.content.1]),
        "marks": marks.iter().map(|m| serde_json::json!({
            "t": m.t, "kind": m.kind, "label": m.label,
            "box": m.bbox.map(|(x,y,w,h)| vec![x,y,w,h]),
        })).collect::<Vec<_>>(),
        "zoom_events": events,
    });
    let _ = std::fs::write(&sidecar, serde_json::to_string_pretty(&telemetry).unwrap());

    render_zoom(
        &raw_path,
        out_path,
        &events,
        out_w,
        out_h,
        plate.as_ref(),
        &ffmpeg,
    )?;
    if !keep_temp {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    Ok((duration, events.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(kind: &str, t: f64, bbox: (f64, f64, f64, f64)) -> Mark {
        Mark {
            kind: kind.into(),
            label: String::new(),
            t,
            bbox: Some(bbox),
        }
    }

    /// A button is a point: centre on it. A text field is read from its left edge, so the
    /// crop is anchored there instead, or the beginning of the line ends up off screen.
    #[test]
    fn the_crop_leans_left_of_an_interaction() {
        let (fw, fh, dur) = (1000.0, 800.0, 20.0);
        // Same box either way, far enough right that centring would cut its left edge off.
        let bbox = (600.0, 300.0, 300.0, 60.0);

        let typed = events_from_marks(&[mark("type", 3.0, bbox)], 1.0, fw, fh, dur);
        let clicked = events_from_marks(&[mark("click", 3.0, bbox)], 1.0, fw, fh, dur);
        assert_eq!(typed.len(), 1);
        assert_eq!(clicked.len(), 1);

        let t = &typed[0];
        let c = &clicked[0];
        let centre = bbox.0 + bbox.2 / 2.0;

        assert!(t.cx < c.cx, "typing leans further left than a click: {} vs {}", t.cx, c.cx);
        assert!(c.cx < centre, "even a click sits left of dead centre: {} vs {centre}", c.cx);

        // Whatever the lean, the target has to stay in the crop.
        for (name, ev) in [("type", t), ("click", c)] {
            let crop_w = fw / ev.z;
            let (l, r) = (ev.cx - crop_w / 2.0, ev.cx + crop_w / 2.0);
            assert!(
                bbox.0 >= l && bbox.0 + bbox.2 <= r,
                "{name}: target [{}, {}] escaped crop [{l}, {r}]",
                bbox.0,
                bbox.0 + bbox.2
            );
        }
    }

    /// Every zoom returns to the wide shot: the expression falls back to its base outside the
    /// event window, so nothing is left cropped once an interaction is over.
    #[test]
    fn the_view_returns_to_wide_after_an_interaction() {
        let evs = events_from_marks(
            &[mark("click", 4.0, (100.0, 100.0, 80.0, 40.0))],
            1.0,
            1000.0,
            800.0,
            20.0,
        );
        assert_eq!(evs.len(), 1);
        let z = build_expr(&evs, "z");
        assert!(z.starts_with("if(between(it,"), "guarded by the event window");
        assert!(z.ends_with(",1)"), "falls back to z=1 outside it, got tail {}", &z[z.len() - 12..]);
        assert!(evs[0].end < 20.0, "the event ends inside the recording");
    }

    fn run(ffmpeg: &str, args: &[&str]) -> bool {
        Command::new(ffmpeg)
            .args(args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Decode one frame of a video as raw RGB.
    fn first_frame(ffmpeg: &str, path: &str) -> Vec<u8> {
        let out = Command::new(ffmpeg)
            .args([
                "-v", "error", "-nostdin", "-i", path, "-frames:v", "1", "-pix_fmt", "rgb24",
                "-f", "rawvideo", "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .expect("decode");
        out.stdout
    }

    /// The generated filter graph, and the hand-rolled PNG the plate is written
    /// as, are the two things only ffmpeg can actually validate. Skipped rather
    /// than failed where ffmpeg is not installed.
    #[test]
    fn composites_the_take_onto_its_plate() {
        let ffmpeg = match find_ffmpeg() {
            Ok(f) => f,
            Err(_) => return,
        };
        let dir = std::env::temp_dir().join("lensa-composite-test");
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("raw.mp4");
        let raw_s = raw.display().to_string();
        assert!(run(
            &ffmpeg,
            &[
                "-y", "-loglevel", "error", "-f", "lavfi", "-i",
                "testsrc=size=320x180:rate=30:duration=1", "-c:v", "libx264", "-pix_fmt",
                "yuv420p", &raw_s,
            ],
        ));

        let (ow, oh) = (640u32, 360u32);
        let bg = crate::backdrop::background("tide").unwrap();
        let plate = crate::backdrop::build(&dir, bg, ow, oh, 320.0 / 180.0).unwrap();
        let out = dir.join("out.mp4").display().to_string();
        let events = vec![ZoomEvent {
            t: 0.2,
            end: 0.9,
            cx: 160.0,
            cy: 90.0,
            z: 1.6,
            path: Vec::new(),
        }];
        render_zoom(&raw_s, &out, &events, ow, oh, Some(&plate), &ffmpeg).unwrap();

        let frame = first_frame(&ffmpeg, &out);
        assert_eq!(
            frame.len(),
            (ow * oh * 3) as usize,
            "output is not {ow}x{oh}"
        );
        // Top-left corner is backdrop: tide is much bluer than it is red. If the
        // PNG were unreadable or the overlay a no-op this would be pad black.
        let (r, b) = (frame[0] as i32, frame[2] as i32);
        assert!(b > r + 30, "corner is not the backdrop: {r},{},{b}", frame[1]);
        // The middle of the frame is the content, which testsrc makes bright.
        let mid = ((oh / 2) * ow + ow / 2) as usize * 3;
        let centre = &frame[mid..mid + 3];
        assert!(
            (centre[0] as i32 - r).abs() + (centre[2] as i32 - b).abs() > 20,
            "centre looks like the backdrop, the content did not land: {centre:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Without a backdrop the take is still the old full-frame render.
    #[test]
    fn renders_full_frame_without_a_plate() {
        let ffmpeg = match find_ffmpeg() {
            Ok(f) => f,
            Err(_) => return,
        };
        let dir = std::env::temp_dir().join("lensa-fullframe-test");
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("raw.mp4").display().to_string();
        assert!(run(
            &ffmpeg,
            &[
                "-y", "-loglevel", "error", "-f", "lavfi", "-i",
                "testsrc=size=320x180:rate=30:duration=1", "-c:v", "libx264", "-pix_fmt",
                "yuv420p", &raw,
            ],
        ));
        let out = dir.join("out.mp4").display().to_string();
        render_zoom(&raw, &out, &[], 640, 360, None, &ffmpeg).unwrap();
        assert_eq!(first_frame(&ffmpeg, &out).len(), 640 * 360 * 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
