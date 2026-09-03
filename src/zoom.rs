//! Telemetry -> zoom timeline -> final MP4.
//!
//! Port of the proven Python zoomcut approach:
//! - cubic smoothstep s = p*p*(3-2p) for every ease
//! - zoom events {t, end, cx, cy, z} with optional path waypoints for
//!   follow-pans between consecutive nearby interactions
//! - EASE = 0.7s, z in 1.5..1.9, crop clamped to the frame
//! - VFR frames are normalized to CFR 30fps BEFORE any time-based math
//!   (pass 1), then a zoompan expression does the zooms (pass 2).

use crate::cdp::Frame;
use crate::ops::Mark;
use std::io::Write;
use std::process::{Command, Stdio};

pub const EASE: f64 = 0.7;
pub const FPS: u32 = 30;

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
            let cx = (x + w / 2.0) * scale;
            let cy = (y + h / 2.0) * scale;
            // Bigger targets get gentler zoom.
            let z = if h * scale > frame_h * 0.45 {
                1.5
            } else if h * scale > frame_h * 0.25 {
                1.7
            } else {
                1.85
            };
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
fn render_cfr(frames: &[Frame], raw_path: &str, ffmpeg: &str) -> Result<f64, String> {
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
        for n in 0..n_ticks {
            let t = n as f64 / FPS as f64;
            while idx + 1 < frames.len() && frames[idx + 1].t <= t {
                idx += 1;
            }
            stdin
                .write_all(&frames[idx].jpeg)
                .map_err(|e| format!("write frame: {e}"))?;
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("ffmpeg pass 1 (CFR normalize) failed".into());
    }
    Ok(t_last)
}

/// Pass 2: zoompan with generated expressions -> final MP4.
fn render_zoom(
    raw_path: &str,
    out_path: &str,
    events: &[ZoomEvent],
    out_w: u32,
    out_h: u32,
    ffmpeg: &str,
) -> Result<(), String> {
    let vf = if events.is_empty() {
        format!("fps={FPS},scale={out_w}:{out_h},format=yuv420p")
    } else {
        let z = build_expr(events, "z");
        let cx = build_expr(events, "cx");
        let cy = build_expr(events, "cy");
        format!(
            "fps={FPS},zoompan=z='({z})':\
             x='clip(({cx})-iw/(2*({z})),0,iw-iw/({z}))':\
             y='clip(({cy})-ih/(2*({z})),0,ih-ih/({z}))':\
             d=1:fps={FPS}:s={out_w}x{out_h},format=yuv420p"
        )
    };
    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-loglevel",
            "error",
            "-i",
            raw_path,
            "-filter:v",
            &vf,
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "19",
            "-movflags",
            "+faststart",
            out_path,
        ])
        .status()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;
    if !status.success() {
        return Err("ffmpeg pass 2 (zoompan) failed".into());
    }
    Ok(())
}

/// Full pipeline: frames + marks -> zoomed MP4. Returns (duration, n_events).
pub fn render(
    frames: &[Frame],
    marks: &[Mark],
    out_path: &str,
    css_w: u32,
    css_h: u32,
    keep_temp: bool,
) -> Result<(f64, usize), String> {
    let ffmpeg = find_ffmpeg()?;
    let (fw, fh) = crate::cdp::jpeg_dims(&frames.first().ok_or("no frames captured")?.jpeg)
        .ok_or("could not parse first frame's JPEG header")?;
    let fw = fw - fw % 2;
    let fh = fh - fh % 2;
    let scale = fw as f64 / css_w as f64;
    let out_w = css_w - css_w % 2;
    let out_h = css_h - css_h % 2;

    let tmp_dir = std::path::Path::new(out_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| ".".into())
        .join(".lensa-tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let raw_path = tmp_dir.join("raw.mp4").display().to_string();

    let duration = render_cfr(frames, &raw_path, &ffmpeg)?;
    let events = events_from_marks(marks, scale, fw as f64, fh as f64, duration);

    // Telemetry sidecar for debugging / re-rendering.
    let sidecar = format!("{out_path}.telemetry.json");
    let telemetry = serde_json::json!({
        "duration": duration,
        "frame_size": [fw, fh],
        "css_size": [css_w, css_h],
        "scale": scale,
        "marks": marks.iter().map(|m| serde_json::json!({
            "t": m.t, "kind": m.kind, "label": m.label,
            "box": m.bbox.map(|(x,y,w,h)| vec![x,y,w,h]),
        })).collect::<Vec<_>>(),
        "zoom_events": events,
    });
    let _ = std::fs::write(&sidecar, serde_json::to_string_pretty(&telemetry).unwrap());

    render_zoom(&raw_path, out_path, &events, out_w, out_h, &ffmpeg)?;
    if !keep_temp {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    Ok((duration, events.len()))
}
