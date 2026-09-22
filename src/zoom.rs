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
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
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
/// Leave a little margin around the target rather than filling the crop with it exactly.
const FIT_MARGIN: f64 = 1.15;

/// Seconds to stay zoomed after an interaction.
const HOLD_AFTER: f64 = 2.1;
/// Seconds to start easing before the interaction moment.
const LEAD_IN: f64 = 0.45;
/// If the next interaction starts within this of our end, keep following it.
const MERGE_GAP: f64 = 1.3;
/// How far before the last frame an event has to finish.
const TAIL_MARGIN: f64 = 0.05;
/// The most the tail of a take will be stretched to give the last interaction its zoom.
const MAX_TAIL_PAD: f64 = HOLD_AFTER + EASE;

/// Minimum spacing between pan waypoints on the time axis.
///
/// ops.rs samples the caret every 0.12s while typing. A floor at or above that interval pushes
/// every single sample later than it happened, and the error accumulates: a 4.3s typing pan
/// measured 5.4s of waypoints and finished a second and a half after the typing did. The floor
/// only has to keep a segment from having zero length, which is far below the sampling rate.
const WAYPOINT_MIN_GAP: f64 = 0.04;

/// Most waypoints kept for one event, and across the whole take.
///
/// Every retained waypoint costs roughly ninety bytes in each of the three generated
/// expressions, and the shipped 29-second README demo alone produced 78 of them. Left
/// unbounded a five-minute take builds a filter graph larger than the kernel will accept as a
/// single argument. The shape of a pan survives decimation; its byte count is what has to stop
/// growing.
const PATH_BUDGET: usize = 16;
const PATH_BUDGET_TOTAL: usize = 192;

/// A filter graph longer than this is handed to ffmpeg as a file instead of an argument.
///
/// Linux caps one argv element at 128 KiB. Staying with an argument for ordinary takes keeps
/// lensa off `-filter_complex_script`, which recent ffmpeg deprecates, while still having a
/// path that works when a very long take generates a graph an argument cannot hold.
const GRAPH_ARG_LIMIT: usize = 32 * 1024;

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
            let ladder: f64 = if h * scale > frame_h * 0.45 {
                1.5
            } else if h * scale > frame_h * 0.25 {
                1.7
            } else {
                1.85
            };
            /*
             * The ladder reads the target's height only, so a wide, short element - a nav bar,
             * a table row, a full-width button - used to take the tightest zoom and hang off
             * both sides of the crop. Bound the zoom by whichever axis runs out first. A target
             * wider than the frame then gets z = 1 and no zoom, which is the honest answer.
             */
            let fit_w = frame_w / (w * scale * FIT_MARGIN);
            let fit_h = frame_h / (h * scale * FIT_MARGIN);
            let z = ladder.min(fit_w).min(fit_h).max(1.0);
            let crop_w = frame_w / z;
            let want = if m.kind == "type" {
                LEFT_BIAS_TYPE
            } else {
                LEFT_BIAS_CLICK
            };
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

    let mut i = 0;
    while i < targets.len() {
        let first = &targets[i];
        let mut ev = ZoomEvent {
            t: (first.t - LEAD_IN).max(0.0),
            end: first.t + HOLD_AFTER,
            cx: first.cx,
            cy: first.cy,
            z: first.z,
            path: Vec::new(),
        };
        let mut j = i + 1;
        while j < targets.len() && targets[j].t < ev.end + MERGE_GAP {
            let next = &targets[j];
            let dist = ((next.cx - ev.cx).powi(2) + (next.cy - ev.cy).powi(2)).sqrt();
            let last_wp_t = ev.path.last().map(|p| p.0).unwrap_or(ev.t + EASE);
            let wp_t = next.t.max(last_wp_t + WAYPOINT_MIN_GAP);
            if dist > 6.0 {
                ev.path.push((wp_t, next.cx, next.cy));
            }
            ev.end = wp_t + HOLD_AFTER;
            ev.z = ev.z.min(next.z); // never tighter than the loosest merged target
            j += 1;
        }
        // Clamp into the recording and keep waypoints inside the eased window.
        ev.end = ev.end.min(duration - TAIL_MARGIN);
        let before = ev.path.len();
        ev.path.retain(|p| p.0 > ev.t + EASE && p.0 < ev.end - EASE);
        if ev.path.len() < before {
            eprintln!(
                "lensa: {} pan waypoint(s) of the interaction at {:.1}s fall outside the zoom \
                 window and were dropped",
                before - ev.path.len(),
                first.t
            );
        }
        if ev.end - ev.t >= 2.0 * EASE + 0.15 {
            events.push(ev);
        } else {
            /*
             * Silence here is what made this expensive to diagnose: the only trace of a lost
             * zoom was a lower event count in a sidecar nobody reads.
             */
            eprintln!(
                "lensa: the interaction at {:.1}s is too close to the end of the {duration:.1}s \
                 take to be zoomed; add a trailing wait before stop_recording",
                first.t
            );
        }
        i = j;
    }
    thin_paths(&mut events);
    events
}

/// Reduce each event's waypoints to a budget, keeping the ones that carry the shape.
fn thin_paths(events: &mut [ZoomEvent]) {
    let wanted: usize = events.iter().map(|e| e.path.len().min(PATH_BUDGET)).sum();
    // Share the total budget out when many events each want their full allowance.
    let squeeze = if wanted > PATH_BUDGET_TOTAL {
        PATH_BUDGET_TOTAL as f64 / wanted as f64
    } else {
        1.0
    };
    let budget = ((PATH_BUDGET as f64 * squeeze).floor() as usize).max(2);
    for ev in events.iter_mut() {
        if ev.path.len() > budget {
            ev.path = thin_path(&ev.path, budget);
        }
    }
}

/// Ramer-Douglas-Peucker with a point budget: repeatedly keep whichever remaining waypoint is
/// furthest from the straight line the pan would otherwise take through its neighbours.
/// Dropping points uniformly instead would flatten exactly the corners a viewer notices.
fn thin_path(path: &[(f64, f64, f64)], budget: usize) -> Vec<(f64, f64, f64)> {
    if path.len() <= budget {
        return path.to_vec();
    }
    let mut keep = vec![0usize, path.len() - 1];
    while keep.len() < budget {
        let mut best: Option<(f64, usize, usize)> = None;
        for w in 0..keep.len() - 1 {
            let (a, b) = (keep[w], keep[w + 1]);
            if b <= a + 1 {
                continue;
            }
            let span = path[b].0 - path[a].0;
            let (mut err, mut at) = (0.0f64, a);
            for (i, p) in path.iter().enumerate().take(b).skip(a + 1) {
                let f = if span > 1e-9 {
                    (p.0 - path[a].0) / span
                } else {
                    0.0
                };
                let ex = path[a].1 + (path[b].1 - path[a].1) * f - p.1;
                let ey = path[a].2 + (path[b].2 - path[a].2) * f - p.2;
                let d = ex.hypot(ey);
                if d > err {
                    err = d;
                    at = i;
                }
            }
            if at > a && best.map_or(true, |(e, _, _)| err > e) {
                best = Some((err, w + 1, at));
            }
        }
        match best {
            // Below a pixel of error the waypoint is not describing a move anyone can see.
            Some((err, pos, idx)) if err > 1.0 => keep.insert(pos, idx),
            _ => break,
        }
    }
    keep.iter().map(|&i| path[i]).collect()
}

/// Constant-rate move from `va` to `vb`. Used between pan waypoints, where easing every
/// segment would make the camera stop at each one. The delta is folded in here rather than
/// emitted as `(b-a)`, because this text is repeated once per waypoint per axis.
fn linear(va: f64, vb: f64, t0: f64, t1: f64) -> String {
    format!(
        "({va:.2}+{:.2}*clip((it-{t0:.3})/({:.3}),0,1))",
        vb - va,
        (t1 - t0).max(0.001)
    )
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
            /*
             * Pan through waypoints while zoomed. The first point is clocked at the END of the
             * ease-in, not at t0: the ease occupies t0..t0+EASE and delivers the first value at
             * its end, and the pan chain takes over at exactly that instant. Clocking the first
             * point at t0 made the pan's first segment already partly travelled by the time it
             * became visible, so the camera teleported at the handover - a measured 433px in
             * one frame on the shipped demo.
             */
            let mut pts: Vec<(f64, f64)> =
                vec![(t0 + EASE, if which == "cx" { ev.cx } else { ev.cy })];
            for p in &ev.path {
                pts.push((p.0, if which == "cx" { p.1 } else { p.2 }));
            }
            let v_first = format!("{:.2}", pts[0].1);
            let v_last = format!("{:.2}", pts[pts.len() - 1].1);
            let ease_in = smooth(base, &v_first, t0, t0 + EASE);
            let ease_out = smooth(&v_last, base, t1 - EASE, t1);
            // Straight lines between waypoints, not a smoothstep per segment. Easing each
            // segment separately drives the velocity to zero at every waypoint, so a run of
            // close samples reads as stepping rather than gliding. The ease belongs at the two
            // ends of the whole move, which is where it already is.
            let mut mid = v_last.clone();
            for k in (0..pts.len() - 1).rev() {
                let (ta, va) = pts[k];
                let (tb, vb) = pts[k + 1];
                let seg = linear(va, vb, ta, tb);
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

/// Find an ffmpeg to shell out to.
///
/// Deliberately not bundled: shipping ffmpeg means shipping its licence, and the builds that
/// can write H.264 are the GPL ones. Using whichever the machine already has keeps lensa's own
/// terms its own business.
pub fn find_ffmpeg() -> Result<String, String> {
    if let Ok(p) = std::env::var("LENSA_FFMPEG") {
        /*
         * A bare command name means "this one, off PATH", which is how the sibling
         * LENSA_CHROMIUM already behaves. Treating it as a filesystem path made
         * LENSA_FFMPEG=ffmpeg7 fail with a message asserting that a binary which exists
         * does not.
         */
        if runs(&p) {
            return Ok(p);
        }
        let named = if p.contains(std::path::MAIN_SEPARATOR) {
            "which does not run"
        } else {
            "which is not a runnable ffmpeg on PATH"
        };
        return Err(format!("LENSA_FFMPEG points at {p}, {named}"));
    }
    if runs("ffmpeg") {
        return Ok("ffmpeg".into());
    }
    let mut tried: Vec<String> = vec!["ffmpeg (on PATH)".into()];
    let mut cands: Vec<std::path::PathBuf> = vec![
        "/usr/bin/ffmpeg".into(),
        "/usr/local/bin/ffmpeg".into(),
        "/opt/homebrew/bin/ffmpeg".into(),
        "/snap/bin/ffmpeg".into(),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let h = Path::new(&home);
        cands.push(h.join(".local/bin/ffmpeg"));
        // Static tarballs unpack into a versioned directory and people leave them there.
        if let Ok(rd) = std::fs::read_dir(h.join(".local/opt")) {
            for e in rd.flatten() {
                let p = e.path().join("ffmpeg");
                if p.is_file() {
                    cands.push(p);
                }
            }
        }
    }
    for p in cands {
        let s = p.display().to_string();
        // A candidate that exists but cannot run - a stub, a dangling wrapper, the wrong
        // architecture - is worse than no candidate, because it is selected and then fails
        // after the whole take has been captured.
        if p.is_file() && runs(&s) {
            return Ok(s);
        }
        tried.push(s);
    }
    Err(format!(
        "ffmpeg not found. Looked in:\n  {}\nInstall one (apt install ffmpeg, brew install \
         ffmpeg, or a static build) or point LENSA_FFMPEG at it.",
        tried.join("\n  ")
    ))
}

fn runs(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// ffmpeg has no `--` end-of-options marker, so any path beginning with a dash is read as an
/// option and the rest of the command line is reinterpreted around it. Making such a path
/// explicitly relative is the portable way to pass it as data.
fn arg_path(p: &str) -> String {
    if p.starts_with('-') {
        format!("./{p}")
    } else {
        p.to_string()
    }
}

/// The last few lines of ffmpeg's own diagnostics, for an error message.
fn stderr_tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let keep = lines.len().saturating_sub(8);
    lines[keep..].join("\n")
}

/// Where the render's intermediates live for the length of one render.
///
/// Two takes rendering into the same output directory used to share one `.lensa-tmp` next to
/// the output, so each overwrote the other's raw.mp4 and whichever finished first deleted the
/// plate the other was still reading. The directory is now unique per run and under TMPDIR,
/// which is also what the action's TMPDIR redirection has always claimed to cover, and the
/// guard removes it on the error paths rather than only on success.
struct TempDir {
    path: PathBuf,
    keep: bool,
}

impl TempDir {
    fn new(out_path: &str, keep: bool) -> Result<TempDir, String> {
        let stem: String = Path::new(out_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(24)
            .collect();
        let stem = if stem.is_empty() {
            "take".to_string()
        } else {
            stem
        };
        let base = std::env::temp_dir();
        for _ in 0..8 {
            let salt = std::collections::hash_map::RandomState::new()
                .build_hasher()
                .finish();
            let path = base.join(format!(
                "lensa-render-{stem}-{}-{salt:016x}",
                std::process::id()
            ));
            let mut b = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                b.mode(0o700);
            }
            match b.create(&path) {
                Ok(()) => return Ok(TempDir { path, keep }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("create {}: {e}", path.display())),
            }
        }
        Err(format!(
            "could not create a render temp directory under {}",
            base.display()
        ))
    }

    /// Keep the intermediates after all. Called when a render fails, because the filter graph
    /// and the pass-1 output are the only two things that can explain why.
    fn retain(&mut self) {
        self.keep = true;
    }

    fn join(&self, name: &str) -> String {
        self.path.join(name).display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.keep {
            eprintln!(
                "lensa: render intermediates kept in {}",
                self.path.display()
            );
            return;
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Pass 1: VFR screencast frames -> CFR 30fps H.264 intermediate.
///
/// `tail_pad` holds the final frame for longer than the take itself ran, so that an
/// interaction in the last second still has room for its hold and its ease-out.
fn render_cfr(
    spool: &FrameSpool,
    raw_path: &str,
    ffmpeg: &str,
    tail_pad: f64,
) -> Result<f64, String> {
    let frames = spool.frames();
    if frames.is_empty() {
        return Err("no frames captured".into());
    }
    let t_last = frames.last().unwrap().t + 0.4 + tail_pad;
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
            &arg_path(raw_path),
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;

    /*
     * The feeding loop cannot use `?`: an early return here would drop the Child without
     * killing or reaping it, leaving ffmpeg as a zombie for the life of the process - and this
     * path is taken precisely when ffmpeg has already died and the pipe gives EPIPE, which is
     * the case whose real cause is sitting unread in ffmpeg's stderr.
     */
    let feed = (|| -> Result<(), String> {
        let stdin = child.stdin.as_mut().ok_or("ffmpeg stdin was not piped")?;
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
            stdin
                .write_all(&held)
                .map_err(|e| format!("write frame: {e}"))?;
        }
        Ok(())
    })();

    // Closing the pipe is what tells ffmpeg the stream ended, so it has to happen before the
    // wait even on the success path.
    drop(child.stdin.take());
    let done = child
        .wait_with_output()
        .map_err(|e| format!("wait for ffmpeg: {e}"))?;
    if let Err(e) = feed {
        let tail = stderr_tail(&done.stderr);
        if tail.is_empty() {
            return Err(format!("{e} (ffmpeg exited {})", done.status));
        }
        return Err(format!("{e} (ffmpeg exited {}):\n{tail}", done.status));
    }
    if !done.status.success() {
        let tail = stderr_tail(&done.stderr);
        return Err(format!(
            "ffmpeg pass 1 (CFR normalize) failed, exit {}:\n{tail}",
            done.status
        ));
    }
    Ok(t_last)
}

/// The largest centred box of the given aspect ratio that fits in out_w x out_h, both sides
/// even because H.264 chroma is subsampled.
fn fit_box(out_w: u32, out_h: u32, aspect: f64) -> (u32, u32) {
    let (ow, oh) = (out_w.max(2) as f64, out_h.max(2) as f64);
    let a = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        ow / oh
    };
    let (cw, ch) = if ow / oh > a {
        (oh * a, oh)
    } else {
        (ow, ow / a)
    };
    let even = |v: f64, cap: u32| -> u32 {
        let n = (v.round().max(2.0) as u32).min(cap.max(2));
        n - n % 2
    };
    (even(cw, out_w), even(ch, out_h))
}

/// Pass 2: zoompan with generated expressions, composited onto the backdrop
/// plate if there is one -> final MP4.
#[allow(clippy::too_many_arguments)]
fn render_zoom(
    raw_path: &str,
    out_path: &str,
    events: &[ZoomEvent],
    out_w: u32,
    out_h: u32,
    plate: Option<&Plate>,
    ffmpeg: &str,
    aspect: f64,
    tmp: &TempDir,
) -> Result<(), String> {
    /*
     * With a backdrop the content is no longer the frame: it is scaled to the
     * plate's window instead, padded out to the frame, and the plate is laid
     * over it. The plate is opaque everywhere except that window, so one
     * overlay draws background, drop shadow and rounded corners together.
     */
    let (cw, ch) = match plate {
        Some(p) => p.content,
        /*
         * Without a plate the content used to be resized straight to the output size, which
         * anamorphically distorts every take whose capture aspect differs from its output
         * aspect - `--preset tiktok --width 500 --background none` among them. Fit and pad
         * instead, exactly as the plate path already does.
         */
        None => fit_box(out_w, out_h, aspect),
    };
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

    let (complex, graph) = match plate {
        Some(p) => {
            let (x, y) = p.origin;
            (
                true,
                format!(
                    "[0:v]{core},format=rgba,pad={out_w}:{out_h}:{x}:{y}[c];\
                     [c][1:v]overlay=0:0:format=auto,format=yuv420p[v]"
                ),
            )
        }
        None => {
            let (x, y) = (
                (out_w.saturating_sub(cw) / 2) & !1,
                (out_h.saturating_sub(ch) / 2) & !1,
            );
            (
                false,
                format!("{core},pad={out_w}:{out_h}:{x}:{y},format=yuv420p"),
            )
        }
    };

    /*
     * The graph goes to disk whatever happens: machine-generated expressions are the thing
     * most likely to be rejected, and until now the only report was "pass 2 failed" with the
     * expression that failed existing nowhere at all.
     */
    let graph_path = tmp.join("filter.txt");
    std::fs::write(&graph_path, &graph)
        .map_err(|e| format!("write filter graph to {graph_path}: {e}"))?;

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-nostdin".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        arg_path(raw_path),
    ];
    if let Some(p) = plate {
        args.push("-i".into());
        args.push(arg_path(&p.path.display().to_string()));
    }
    // An argv element is capped at 128 KiB by the kernel, so a graph that has outgrown one is
    // passed by filename instead. Ordinary takes stay on the argument, which every ffmpeg
    // accepts without deprecation warnings.
    let by_file = graph.len() > GRAPH_ARG_LIMIT;
    match (complex, by_file) {
        (true, false) => {
            args.push("-filter_complex".into());
            args.push(graph);
        }
        (true, true) => {
            args.push("-filter_complex_script".into());
            args.push(arg_path(&graph_path));
        }
        (false, false) => {
            args.push("-filter:v".into());
            args.push(graph);
        }
        (false, true) => {
            args.push("-filter_script:v".into());
            args.push(arg_path(&graph_path));
        }
    }
    if complex {
        args.push("-map".into());
        args.push("[v]".into());
    }
    args.extend(
        [
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "19",
            "-movflags",
            "+faststart",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    args.push(arg_path(out_path));

    let done = Command::new(ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;
    if !done.status.success() {
        let tail = stderr_tail(&done.stderr);
        return Err(format!(
            "ffmpeg pass 2 (zoompan) failed, exit {}. The filter graph is in {graph_path}\n{tail}",
            done.status
        ));
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

/// How much to stretch the tail of the take so the last interaction still gets a zoom.
///
/// An event ends `HOLD_AFTER` past its interaction and is dropped entirely if that lands
/// beyond the last frame, so every script ending in a click and a stop_recording lost the zoom
/// on the very thing it was demonstrating. Holding the last frame is honest: nothing happened
/// after it either way.
fn tail_pad_for(marks: &[Mark], raw_end: f64) -> f64 {
    let last = marks
        .iter()
        .filter(|m| m.bbox.is_some() && matches!(m.kind.as_str(), "click" | "type"))
        .map(|m| m.t)
        .fold(f64::NEG_INFINITY, f64::max);
    if !last.is_finite() {
        return 0.0;
    }
    (last + HOLD_AFTER + TAIL_MARGIN - raw_end).clamp(0.0, MAX_TAIL_PAD)
}

/// Decide where, if anywhere, the telemetry sidecar goes.
///
/// It used to be written next to every video unconditionally and silently. Its `marks` array
/// carries each navigate label, which is a fully resolved URL including any query string or
/// token, so delivering it alongside the deliverable is a decision the user should make.
fn telemetry_path(out_path: &str, keep_temp: bool) -> Option<String> {
    match std::env::var("LENSA_TELEMETRY") {
        Ok(v) if v == "0" || v == "off" => None,
        Ok(v) if !v.is_empty() => Some(v),
        _ if keep_temp => Some(format!("{out_path}.telemetry.json")),
        _ => None,
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

    let mut tmp = TempDir::new(out_path, keep_temp)?;
    let raw_path = tmp.join("raw.mp4");

    eprintln!(
        "lensa: viewport {css_w}x{css_h}, capture {fw}x{fh} ({scale:.2}x), output {out_w}x{out_h}"
    );

    let raw_end = spool.frames().last().map(|f| f.t).unwrap_or(0.0) + 0.4;
    let duration = render_cfr(spool, &raw_path, &ffmpeg, tail_pad_for(marks, raw_end))?;
    let events = events_from_marks(marks, scale, fw as f64, fh as f64, duration);
    /*
     * After pass 1, not before: the auto picker measures the take itself, and
     * the CFR intermediate is the only place the captured pixels exist in a
     * form ffmpeg can read cheaply.
     */
    let plate = plate_for(
        background,
        &tmp.path,
        &raw_path,
        &ffmpeg,
        out_w,
        out_h,
        fw as f64 / fh as f64,
    );

    if let Some(sidecar) = telemetry_path(out_path, keep_temp) {
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
        let body = serde_json::to_string_pretty(&telemetry).unwrap_or_else(|_| "{}".into());
        match std::fs::write(&sidecar, body) {
            // Said out loud because the labels include every URL the script visited.
            Ok(()) => eprintln!("lensa: telemetry (including visited URLs) written to {sidecar}"),
            Err(e) => eprintln!("lensa: could not write telemetry to {sidecar}: {e}"),
        }
    }

    let zoomed = render_zoom(
        &raw_path,
        out_path,
        &events,
        out_w,
        out_h,
        plate.as_ref(),
        &ffmpeg,
        fw as f64 / fh as f64,
        &tmp,
    );
    if zoomed.is_err() {
        tmp.retain();
    }
    zoomed?;
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

    /// A tiny evaluator for the subset of ffmpeg expression syntax `build_expr` emits, so the
    /// camera path can be sampled in a test instead of being eyeballed in a rendered video.
    struct Eval<'a> {
        s: &'a [u8],
        i: usize,
        it: f64,
        iw: f64,
        ih: f64,
    }

    impl<'a> Eval<'a> {
        fn peek(&self) -> Option<u8> {
            self.s.get(self.i).copied()
        }
        fn expr(&mut self) -> f64 {
            let mut v = self.term();
            while let Some(c) = self.peek() {
                match c {
                    b'+' => {
                        self.i += 1;
                        v += self.term();
                    }
                    b'-' => {
                        self.i += 1;
                        v -= self.term();
                    }
                    _ => break,
                }
            }
            v
        }
        fn term(&mut self) -> f64 {
            let mut v = self.factor();
            while let Some(c) = self.peek() {
                match c {
                    b'*' => {
                        self.i += 1;
                        v *= self.factor();
                    }
                    b'/' => {
                        self.i += 1;
                        v /= self.factor();
                    }
                    _ => break,
                }
            }
            v
        }
        fn args(&mut self, n: usize) -> Vec<f64> {
            let mut out = Vec::with_capacity(n);
            self.i += 1; // the '('
            for k in 0..n {
                out.push(self.expr());
                let c = self.peek().expect("truncated argument list");
                assert_eq!(c, if k + 1 == n { b')' } else { b',' }, "bad argument list");
                self.i += 1;
            }
            out
        }
        fn factor(&mut self) -> f64 {
            match self.peek().expect("truncated expression") {
                b'(' => {
                    self.i += 1;
                    let v = self.expr();
                    assert_eq!(self.peek(), Some(b')'), "unbalanced parenthesis");
                    self.i += 1;
                    v
                }
                b'-' => {
                    self.i += 1;
                    -self.factor()
                }
                c if c.is_ascii_digit() || c == b'.' => {
                    let start = self.i;
                    while matches!(self.peek(), Some(d) if d.is_ascii_digit() || d == b'.') {
                        self.i += 1;
                    }
                    std::str::from_utf8(&self.s[start..self.i])
                        .unwrap()
                        .parse()
                        .unwrap()
                }
                _ => {
                    let start = self.i;
                    while matches!(self.peek(), Some(d) if d.is_ascii_alphabetic()) {
                        self.i += 1;
                    }
                    let name = std::str::from_utf8(&self.s[start..self.i])
                        .unwrap()
                        .to_string();
                    match name.as_str() {
                        "it" => self.it,
                        "iw" => self.iw,
                        "ih" => self.ih,
                        "lt" => {
                            let a = self.args(2);
                            (a[0] < a[1]) as i32 as f64
                        }
                        "gt" => {
                            let a = self.args(2);
                            (a[0] > a[1]) as i32 as f64
                        }
                        "between" => {
                            let a = self.args(3);
                            (a[0] >= a[1] && a[0] <= a[2]) as i32 as f64
                        }
                        "clip" => {
                            let a = self.args(3);
                            a[0].clamp(a[1], a[2])
                        }
                        "if" => {
                            let a = self.args(3);
                            if a[0] != 0.0 {
                                a[1]
                            } else {
                                a[2]
                            }
                        }
                        other => panic!("unsupported function in generated expression: {other}"),
                    }
                }
            }
        }
    }

    fn eval(expr: &str, it: f64, iw: f64, ih: f64) -> f64 {
        let mut e = Eval {
            s: expr.as_bytes(),
            i: 0,
            it,
            iw,
            ih,
        };
        let v = e.expr();
        assert_eq!(e.i, expr.len(), "trailing text in expression");
        v
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

        assert!(
            t.cx < c.cx,
            "typing leans further left than a click: {} vs {}",
            t.cx,
            c.cx
        );
        assert!(
            c.cx < centre,
            "even a click sits left of dead centre: {} vs {centre}",
            c.cx
        );

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

    /// Waypoints are joined by straight lines. Easing each segment separately brings the
    /// camera to a stop at every sample, which over a typing pan reads as stepping.
    #[test]
    fn a_pan_moves_at_a_constant_rate_between_waypoints() {
        let marks: Vec<Mark> = (0..6)
            .map(|k| {
                let t = 3.0 + k as f64 * 0.12;
                mark("type", t, (200.0 + k as f64 * 40.0, 300.0, 2.0, 40.0))
            })
            .collect();
        let evs = events_from_marks(&marks, 1.0, 1000.0, 800.0, 20.0);
        assert_eq!(evs.len(), 1, "close samples merge into one move");
        assert!(
            evs[0].path.len() >= 3,
            "and keep their waypoints: {:?}",
            evs[0].path.len()
        );

        // Exactly two eases in the horizontal move: into the zoom and back out of it. One per
        // segment as well would be the stepping, and would grow with the number of samples.
        let cx = build_expr(&evs, "cx");
        let eases = cx.matches("*(3-2*").count();
        assert_eq!(
            eases,
            2,
            "expected an ease at each end of the move and straight lines between, got {eases} \
             across {} waypoints",
            evs[0].path.len()
        );
        let z = build_expr(&evs, "z");
        assert_eq!(
            z.matches("*(3-2*").count(),
            2,
            "the zoom itself still eases in and out"
        );
    }

    /// The ease-in hands the camera over to the pan at t0 + EASE. Clocking the pan's first
    /// waypoint at t0 instead meant the first segment was already part-travelled at the
    /// handover, and the crop centre jumped 433px in a single frame on the shipped demo.
    #[test]
    fn the_pan_starts_exactly_where_the_ease_in_lands() {
        let (fw, fh) = (2200.0, 1240.0);
        let marks: Vec<Mark> = (0..8)
            .map(|k| {
                let t = 4.4 + k as f64 * 0.35;
                mark("type", t, (1400.0 - k as f64 * 60.0, 300.0, 4.0, 40.0))
            })
            .collect();
        let evs = events_from_marks(&marks, 1.0, fw, fh, 30.0);
        assert_eq!(evs.len(), 1);
        assert!(
            !evs[0].path.is_empty(),
            "the move has waypoints to pan through"
        );
        let (ev, boundary) = (&evs[0], evs[0].t + EASE);

        for which in ["cx", "cy"] {
            let expr = build_expr(&evs, which);
            let before = eval(&expr, boundary - 1e-4, fw, fh);
            let after = eval(&expr, boundary + 1e-4, fw, fh);
            assert!(
                (before - after).abs() < 1.0,
                "{which} teleports {:.1}px at the ease-in/pan boundary: {before:.1} -> {after:.1}",
                (before - after).abs()
            );
        }

        // And nothing anywhere else in the event moves by a frame's worth of a whip either.
        let expr = build_expr(&evs, "cx");
        let (mut prev, mut worst) = (eval(&expr, ev.t, fw, fh), 0.0f64);
        let mut t = ev.t;
        while t < ev.end {
            t += 1.0 / FPS as f64;
            let v = eval(&expr, t, fw, fh);
            worst = worst.max((v - prev).abs());
            prev = v;
        }
        assert!(
            worst < 80.0,
            "the camera jumps {worst:.1}px in one frame somewhere in the move"
        );
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
        assert!(
            z.starts_with("if(between(it,"),
            "guarded by the event window"
        );
        assert!(
            z.ends_with(",1)"),
            "falls back to z=1 outside it, got tail {}",
            &z[z.len() - 12..]
        );
        assert!(evs[0].end < 20.0, "the event ends inside the recording");
    }

    /// Choosing the zoom from the target's height alone hung a wide element off both sides of
    /// the crop; nothing recomputed the zoom to fit it.
    #[test]
    fn a_wide_target_is_not_cropped_off_at_the_sides() {
        let (fw, fh) = (2200.0, 1240.0);
        // A full-width nav bar: 2000px across, 40px tall, which the height ladder would have
        // given the tightest zoom of all.
        let bbox = (100.0, 80.0, 2000.0, 40.0);
        let evs = events_from_marks(&[mark("click", 4.0, bbox)], 1.0, fw, fh, 20.0);
        assert_eq!(evs.len(), 1);
        let ev = &evs[0];
        assert_eq!(
            ev.z, 1.0,
            "a target as wide as the frame gets no zoom at all"
        );

        let crop_w = fw / ev.z;
        let (l, r) = (ev.cx - crop_w / 2.0, ev.cx + crop_w / 2.0);
        assert!(
            bbox.0 >= l - 0.001 && bbox.0 + bbox.2 <= r + 0.001,
            "target [{}, {}] escaped crop [{l}, {r}]",
            bbox.0,
            bbox.0 + bbox.2
        );

        // A narrow target of the same height still zooms, so the ladder is not simply disabled.
        let narrow = events_from_marks(
            &[mark("click", 4.0, (100.0, 80.0, 120.0, 40.0))],
            1.0,
            fw,
            fh,
            20.0,
        );
        assert!(
            narrow[0].z > 1.5,
            "a small target still zooms: {}",
            narrow[0].z
        );
    }

    /// Waypoints used to be spaced no closer than 0.15s while the caret is sampled every
    /// 0.12s, so every sample was pushed later than it happened and the pan finished well
    /// after the typing did.
    #[test]
    fn a_typing_pan_keeps_up_with_the_typing() {
        let n = 30;
        let marks: Vec<Mark> = (0..n)
            .map(|k| {
                let t = 5.0 + k as f64 * 0.12;
                mark("type", t, (300.0 + k as f64 * 12.0, 400.0, 2.0, 40.0))
            })
            .collect();
        let last_typed = 5.0 + (n - 1) as f64 * 0.12;
        let evs = events_from_marks(&marks, 1.0, 1600.0, 900.0, 40.0);
        assert_eq!(evs.len(), 1);
        let last_wp = evs[0].path.last().unwrap().0;
        assert!(
            last_wp <= last_typed + 0.2,
            "the pan finishes {:.2}s after the typing did",
            last_wp - last_typed
        );
    }

    /// The filter graph is one argv element, which the kernel caps at 128 KiB. A long take
    /// used to grow it without any bound at all.
    #[test]
    fn a_long_take_cannot_grow_the_filter_graph_without_bound() {
        let marks: Vec<Mark> = (0..1200)
            .map(|k| {
                let t = 1.0 + k as f64 * 0.12;
                mark(
                    "type",
                    t,
                    (200.0 + (k % 40) as f64 * 25.0, 300.0, 2.0, 40.0),
                )
            })
            .collect();
        let evs = events_from_marks(&marks, 1.0, 1600.0, 900.0, 200.0);
        let waypoints: usize = evs.iter().map(|e| e.path.len()).sum();
        assert!(
            waypoints <= PATH_BUDGET_TOTAL,
            "{waypoints} waypoints survived thinning"
        );
        for which in ["z", "cx", "cy"] {
            let n = build_expr(&evs, which).len();
            assert!(n < 120_000, "the {which} expression alone is {n} bytes");
        }
    }

    /// Thinning keeps the corners of a pan rather than every n-th sample of it.
    #[test]
    fn thinning_keeps_the_shape_of_a_pan() {
        // An L: straight along x, then straight along y. Only the corner matters.
        let mut path: Vec<(f64, f64, f64)> = Vec::new();
        for k in 0..20 {
            path.push((k as f64 * 0.1, k as f64 * 10.0, 0.0));
        }
        for k in 1..20 {
            path.push((2.0 + k as f64 * 0.1, 190.0, k as f64 * 10.0));
        }
        let thin = thin_path(&path, 4);
        assert!(thin.len() <= 4);
        assert_eq!(thin[0], path[0], "the first waypoint is always kept");
        assert_eq!(
            thin[thin.len() - 1],
            path[path.len() - 1],
            "and so is the last"
        );
        assert!(
            thin.iter().any(|p| (p.1 - 190.0).abs() < 1.0 && p.2 < 20.0),
            "the corner survived: {thin:?}"
        );
    }

    /// Without a backdrop the content is fitted into the output, not stretched to it.
    #[test]
    fn a_plateless_render_keeps_the_capture_aspect() {
        // A 1250x1920 capture (0.651) into a 1080x1920 video (0.5625).
        assert_eq!(fit_box(1080, 1920, 1250.0 / 1920.0), (1080, 1658));
        // A wider capture than the output is limited by height instead.
        assert_eq!(fit_box(1920, 1080, 16.0 / 9.0), (1920, 1080));
        assert_eq!(fit_box(1080, 1080, 2.0), (1080, 540));
        // A nonsense aspect falls back to filling the frame rather than dividing by zero.
        assert_eq!(fit_box(640, 480, f64::NAN), (640, 480));
    }

    fn run(ffmpeg: &str, args: &[&str]) -> bool {
        Command::new(ffmpeg)
            .args(args)
            .stdin(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Decode one frame of a video as raw RGB.
    fn first_frame(ffmpeg: &str, path: &str) -> Vec<u8> {
        let out = Command::new(ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-i",
                path,
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .expect("decode");
        out.stdout
    }

    /// The generated filter graph, and the hand-rolled PNG the plate is written as, are the
    /// two things only ffmpeg can actually validate. Skipping them silently when ffmpeg is
    /// missing meant CI reported a pass for tests that never ran, so the skip is now explicit
    /// and has to be asked for.
    fn ffmpeg_for_test() -> Option<String> {
        match find_ffmpeg() {
            Ok(f) => Some(f),
            Err(e) => {
                if std::env::var("LENSA_SKIP_FFMPEG_TESTS").as_deref() == Ok("1") {
                    eprintln!("lensa: skipping an ffmpeg test (LENSA_SKIP_FFMPEG_TESTS=1): {e}");
                    return None;
                }
                panic!("{e}\n\nSet LENSA_SKIP_FFMPEG_TESTS=1 to skip the tests that need ffmpeg.");
            }
        }
    }

    #[test]
    fn composites_the_take_onto_its_plate() {
        let Some(ffmpeg) = ffmpeg_for_test() else {
            return;
        };
        let tmp = TempDir::new("composite-test", false).unwrap();
        let dir = tmp.path.clone();
        let raw_s = tmp.join("raw.mp4");
        assert!(run(
            &ffmpeg,
            &[
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=30:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                &raw_s,
            ],
        ));

        let (ow, oh) = (640u32, 360u32);
        let bg = crate::backdrop::background("tide").unwrap();
        let plate = crate::backdrop::build(&dir, bg, ow, oh, 320.0 / 180.0).unwrap();
        let out = tmp.join("out.mp4");
        let events = vec![ZoomEvent {
            t: 0.2,
            end: 0.9,
            cx: 160.0,
            cy: 90.0,
            z: 1.6,
            path: Vec::new(),
        }];
        render_zoom(
            &raw_s,
            &out,
            &events,
            ow,
            oh,
            Some(&plate),
            &ffmpeg,
            320.0 / 180.0,
            &tmp,
        )
        .unwrap();

        let frame = first_frame(&ffmpeg, &out);
        assert_eq!(
            frame.len(),
            (ow * oh * 3) as usize,
            "output is not {ow}x{oh}"
        );
        // Top-left corner is backdrop: tide is much bluer than it is red. If the
        // PNG were unreadable or the overlay a no-op this would be pad black.
        let (r, b) = (frame[0] as i32, frame[2] as i32);
        assert!(
            b > r + 30,
            "corner is not the backdrop: {r},{},{b}",
            frame[1]
        );
        // The middle of the frame is the content, which testsrc makes bright.
        let mid = ((oh / 2) * ow + ow / 2) as usize * 3;
        let centre = &frame[mid..mid + 3];
        assert!(
            (centre[0] as i32 - r).abs() + (centre[2] as i32 - b).abs() > 20,
            "centre looks like the backdrop, the content did not land: {centre:?}"
        );
    }

    /// Without a backdrop the take is still the old full-frame render.
    #[test]
    fn renders_full_frame_without_a_plate() {
        let Some(ffmpeg) = ffmpeg_for_test() else {
            return;
        };
        let tmp = TempDir::new("fullframe-test", false).unwrap();
        let raw = tmp.join("raw.mp4");
        assert!(run(
            &ffmpeg,
            &[
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=30:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                &raw,
            ],
        ));
        let out = tmp.join("out.mp4");
        render_zoom(
            &raw,
            &out,
            &[],
            640,
            360,
            None,
            &ffmpeg,
            320.0 / 180.0,
            &tmp,
        )
        .unwrap();
        assert_eq!(first_frame(&ffmpeg, &out).len(), 640 * 360 * 3);
    }

    /// A capture whose aspect does not match the output is letterboxed, not stretched.
    #[test]
    fn a_plateless_render_letterboxes_a_mismatched_capture() {
        let Some(ffmpeg) = ffmpeg_for_test() else {
            return;
        };
        let tmp = TempDir::new("letterbox-test", false).unwrap();
        let raw = tmp.join("raw.mp4");
        assert!(run(
            &ffmpeg,
            &[
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=30:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                &raw,
            ],
        ));
        let out = tmp.join("out.mp4");
        // 16:9 content into a 1:1 frame: the top and bottom bands have to be pad, not content.
        render_zoom(
            &raw,
            &out,
            &[],
            360,
            360,
            None,
            &ffmpeg,
            320.0 / 180.0,
            &tmp,
        )
        .unwrap();
        let frame = first_frame(&ffmpeg, &out);
        assert_eq!(frame.len(), 360 * 360 * 3);
        let top: i32 = frame[..3].iter().map(|&v| v as i32).sum();
        assert!(
            top < 60,
            "the top band is content, so the take was stretched: {:?}",
            &frame[..3]
        );
    }
}
