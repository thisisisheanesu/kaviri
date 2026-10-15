//! Audio capture (`--audio`): what the page plays, muxed into the take.
//!
//! There is no sound card in a headless browser and no system mixer to record from on a CI
//! runner, so the sound is taken where it is made, inside the page. `AUDIO_JS` reroutes every
//! connection to an `AudioContext`'s destination through a tap, and hooks each `<audio>` and
//! `<video>` into the same tap when it starts playing. The tap hands PCM to kaviri through a
//! CDP binding, stamped with the wall-clock moment it plays, and `AudioTrack` mixes it onto
//! the recording clock. After the video is rendered, `mux` lays the mix under it as AAC.

use base64::Engine;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

/// The name the page calls with each chunk of PCM.
pub const BINDING: &str = "__kaviri_audio";

/// The mix is kept at one rate, whatever each context runs at.
const RATE: u32 = 48_000;

/// A chunk stamped further from the start of the take than this is a page lying about its
/// clock, not sound, and would otherwise allocate the gap.
const MAX_POS_SECS: f64 = 4.0 * 3600.0;

/// Installed before any page script runs, after `BINDING` exists, and before `SLOWMO_JS`
/// so the clock it keeps is the real one.
///
/// Silent buffers are not sent: a context runs its graph continuously whether or not anything
/// is playing, and a page that made one beep would otherwise stream silence over CDP for the
/// rest of the take. Each chunk carries its own position in its context's sample timeline, so
/// skipping silence leaves no drift behind.
///
/// A media element is hooked with `createMediaElementSource`, which the browser feeds silence
/// for a cross-origin file served without CORS. That is the platform's rule, not kaviri's.
pub const AUDIO_JS: &str = r##"
(() => {
  const send = window.__kaviri_audio;
  if (typeof send !== 'function' || window.__kaviri_audio_tap) return;
  window.__kaviri_audio_tap = true;
  const AC = window.AudioContext || window.webkitAudioContext;
  if (!AC) return;
  const FRAMES = 4096;
  // Taken before anything can warp it.
  const now = performance.now.bind(performance);
  const connect = AudioNode.prototype.connect;
  const disconnect = AudioNode.prototype.disconnect;
  const taps = new WeakMap();
  const b64 = (u8) => {
    let s = '';
    for (let i = 0; i < u8.length; i += 0x8000)
      s += String.fromCharCode.apply(null, u8.subarray(i, i + 0x8000));
    return btoa(s);
  };
  const tap = (ctx) => {
    let t = taps.get(ctx);
    if (t) return t;
    const id = Math.random().toString(36).slice(2) + Date.now().toString(36);
    const bus = ctx.createGain();
    const sp = ctx.createScriptProcessor(FRAMES, 2, 2);
    sp.channelCountMode = 'explicit';
    sp.channelInterpretation = 'speakers';
    connect.call(bus, ctx.destination);
    connect.call(bus, sp);
    // A processor that reaches nothing is never run. Its output buffer stays zeros.
    connect.call(sp, ctx.destination);
    sp.onaudioprocess = (e) => {
      const L = e.inputBuffer.getChannelData(0);
      const R = e.inputBuffer.numberOfChannels > 1 ? e.inputBuffer.getChannelData(1) : L;
      let loud = false;
      for (let i = 0; i < L.length; i++) {
        if (L[i] > 1e-4 || L[i] < -1e-4 || R[i] > 1e-4 || R[i] < -1e-4) { loud = true; break; }
      }
      if (!loud) return;
      const pcm = new Int16Array(L.length * 2);
      for (let i = 0; i < L.length; i++) {
        const l = Math.max(-1, Math.min(1, L[i])), r = Math.max(-1, Math.min(1, R[i]));
        pcm[2 * i] = l * 32767;
        pcm[2 * i + 1] = r * 32767;
      }
      /*
       * When this buffer was made, on the wall clock: it has just been filled, so it ends
       * now. Not when it is heard. getOutputTimestamp says that, and it includes whatever
       * latency the output device claims, which in a headless browser is a fake device's
       * and puts every sound a sixth of a second after what caused it. Only the first
       * chunk of a context uses this; the rest are placed by sample count.
       */
      const wall = performance.timeOrigin + now() - (L.length / ctx.sampleRate) * 1000;
      try {
        send(JSON.stringify({
          id, rate: ctx.sampleRate, at: Math.round(e.playbackTime * ctx.sampleRate),
          wall, data: b64(new Uint8Array(pcm.buffer)),
        }));
      } catch (_) {}
    };
    t = { bus, sp };
    taps.set(ctx, t);
    return t;
  };
  const isDest = (n) => typeof AudioDestinationNode !== 'undefined' && n instanceof AudioDestinationNode;
  AudioNode.prototype.connect = function (dest, ...rest) {
    if (isDest(dest)) {
      connect.call(this, tap(dest.context).bus, ...rest);
      return dest;
    }
    return connect.call(this, dest, ...rest);
  };
  AudioNode.prototype.disconnect = function (dest, ...rest) {
    if (isDest(dest)) return disconnect.call(this, tap(dest.context).bus, ...rest);
    return disconnect.call(this, dest, ...rest);
  };
  // Media elements: hooked once, the first time each one plays.
  const hooked = new WeakSet();
  const cmes = AC.prototype.createMediaElementSource;
  AC.prototype.createMediaElementSource = function (el) {
    hooked.add(el);
    return cmes.call(this, el);
  };
  let shared = null;
  document.addEventListener('play', (e) => {
    const el = e.target;
    if (!(el instanceof HTMLMediaElement) || hooked.has(el)) return;
    hooked.add(el);
    try {
      shared = shared || new AC();
      cmes.call(shared, el).connect(shared.destination);
      if (shared.state !== 'running') shared.resume();
    } catch (_) {}
  }, true);
})();
"##;

/// Where each context's sample timeline sits on the wall clock, fixed by its first chunk.
///
/// Placing every chunk by its own wall stamp would jitter by a few milliseconds from one to
/// the next, which is a click at every seam. Anchoring once and then counting samples keeps
/// each context gapless.
struct Anchor {
    /// Wall-clock seconds at sample zero of the context.
    wall0: f64,
}

/// The take's sound, mixed onto the recording clock at `RATE`, interleaved stereo.
#[derive(Default)]
pub struct AudioTrack {
    buf: Vec<f32>,
    anchors: HashMap<String, Anchor>,
    chunks: usize,
}

impl AudioTrack {
    pub fn new() -> AudioTrack {
        AudioTrack::default()
    }

    /// Start a new take.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.anchors.clear();
        self.chunks = 0;
    }

    pub fn is_silent(&self) -> bool {
        self.chunks == 0
    }

    /// One chunk from the page. `rec_wall` is the wall-clock second recording started.
    ///
    /// The payload comes from the page, so everything in it is checked: a chunk that does
    /// not parse, or claims a rate or a position no context could have, is dropped.
    pub fn push(&mut self, payload: &str, rec_wall: f64) {
        let Ok(v) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        let (Some(id), Some(rate), Some(at), Some(wall), Some(data)) = (
            v["id"].as_str(),
            v["rate"].as_f64(),
            v["at"].as_f64(),
            v["wall"].as_f64(),
            v["data"].as_str(),
        ) else {
            return;
        };
        if !(3000.0..=384_000.0).contains(&rate) || !at.is_finite() || !wall.is_finite() {
            return;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            return;
        };
        let pcm: Vec<f32> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        if pcm.len() < 2 {
            return;
        }
        let anchor = self.anchors.entry(id.to_string()).or_insert(Anchor {
            wall0: wall / 1000.0 - at / rate,
        });
        let pos = anchor.wall0 + at / rate - rec_wall;
        self.mix(&pcm, rate, pos);
    }

    /// Add interleaved stereo `pcm` at `rate`, starting `pos` seconds into the take.
    fn mix(&mut self, pcm: &[f32], rate: f64, pos: f64) {
        let frames_in = pcm.len() / 2;
        let ratio = RATE as f64 / rate;
        let frames_out = (frames_in as f64 * ratio).round() as usize;
        let start = (pos * RATE as f64).round() as i64;
        if pos > MAX_POS_SECS || start + (frames_out as i64) <= 0 {
            return;
        }
        let end = (start + frames_out as i64) as usize * 2;
        if self.buf.len() < end {
            self.buf.resize(end, 0.0);
        }
        for j in 0..frames_out {
            let at = start + j as i64;
            if at < 0 {
                continue;
            }
            // Linear interpolation is enough here: the usual case is no conversion at all.
            let src = j as f64 / ratio;
            let i = src.floor() as usize;
            let f = (src - i as f64) as f32;
            let i1 = (i + 1).min(frames_in - 1);
            let i = i.min(frames_in - 1);
            for c in 0..2 {
                let s = pcm[2 * i + c] * (1.0 - f) + pcm[2 * i1 + c] * f;
                self.buf[at as usize * 2 + c] += s;
            }
        }
        self.chunks += 1;
    }

    /// The mix as a 16-bit stereo WAV exactly `secs` long, padded or cut to match the video.
    fn wav(&self, secs: f64) -> Vec<u8> {
        let frames = (secs.max(0.0) * RATE as f64).round() as usize;
        let data_len = frames * 4;
        let mut out = Vec::with_capacity(44 + data_len);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&2u16.to_le_bytes()); // stereo
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        for k in 0..frames * 2 {
            let s = self.buf.get(k).copied().unwrap_or(0.0).clamp(-1.0, 1.0);
            out.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
        }
        out
    }
}

/// Lay the take's sound under the rendered video at `out_path`, which is `secs` long.
///
/// The video stream is copied, not re-encoded, so this costs about as long as writing the file.
pub fn mux(out_path: &str, track: &AudioTrack, secs: f64, keep_temp: bool) -> Result<(), String> {
    let ffmpeg = crate::zoom::find_ffmpeg()?;
    let dir = crate::cdp::session_dir()?;
    let wav = dir.join(format!("audio-{}.wav", std::process::id()));
    std::fs::write(&wav, track.wav(secs)).map_err(|e| format!("write audio: {e}"))?;
    let out = Path::new(out_path);
    let staged = out.with_file_name(format!(
        ".{}.audio.mp4",
        out.file_name().and_then(|n| n.to_str()).unwrap_or("kaviri")
    ));
    let staged_s = staged.display().to_string();
    let done = Command::new(&ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(crate::zoom::arg_path(out_path))
        .arg("-i")
        .arg(crate::zoom::arg_path(&wav.display().to_string()))
        .args([
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
        ])
        .arg(crate::zoom::arg_path(&staged_s))
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;
    if !keep_temp {
        let _ = std::fs::remove_file(&wav);
    } else {
        eprintln!("kaviri: kept the take's audio at {}", wav.display());
    }
    if !done.status.success() {
        let _ = std::fs::remove_file(&staged);
        return Err(format!(
            "ffmpeg audio mux failed, exit {}:\n{}",
            done.status,
            crate::zoom::stderr_tail(&done.stderr)
        ));
    }
    std::fs::rename(&staged, out).map_err(|e| format!("move muxed take into place: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &str, rate: f64, at: f64, wall_ms: f64, frames: usize, v: i16) -> String {
        let mut bytes = Vec::new();
        for _ in 0..frames * 2 {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        serde_json::json!({
            "id": id, "rate": rate, "at": at, "wall": wall_ms,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        })
        .to_string()
    }

    #[test]
    fn a_chunk_lands_where_it_was_heard_on_the_recording_clock() {
        let mut t = AudioTrack::new();
        // Recording started at wall 1000s; the chunk played 2s later.
        t.push(&chunk("a", 48000.0, 0.0, 1_002_000.0, 480, 16384), 1000.0);
        assert!(!t.is_silent());
        let first = t.buf.iter().position(|s| *s != 0.0).unwrap() / 2;
        assert_eq!(first, 96_000);
    }

    #[test]
    fn later_chunks_follow_the_sample_count_not_their_own_stamp() {
        let mut t = AudioTrack::new();
        t.push(&chunk("a", 48000.0, 4800.0, 1_001_000.0, 100, 1000), 1000.0);
        // Stamped 7ms late, which is jitter; its sample position says exactly 1s later.
        t.push(
            &chunk("a", 48000.0, 52800.0, 1_002_007.0, 100, 1000),
            1000.0,
        );
        let loud: Vec<usize> = (0..t.buf.len() / 2)
            .filter(|i| t.buf[i * 2] != 0.0)
            .collect();
        assert_eq!(loud[0], 48_000);
        assert_eq!(loud[100], 96_000);
    }

    #[test]
    fn a_chunk_from_before_the_take_is_dropped_and_nonsense_is_ignored() {
        let mut t = AudioTrack::new();
        t.push(&chunk("a", 48000.0, 0.0, 990_000.0, 480, 1000), 1000.0);
        t.push("not json", 1000.0);
        t.push(&chunk("b", 1.0, 0.0, 1_001_000.0, 480, 1000), 1000.0);
        assert!(t.is_silent());
    }

    #[test]
    fn the_wav_is_exactly_as_long_as_the_video() {
        let mut t = AudioTrack::new();
        t.push(&chunk("a", 44100.0, 0.0, 1_000_500.0, 4410, 1000), 1000.0);
        let w = t.wav(2.0);
        assert_eq!(w.len(), 44 + 2 * RATE as usize * 4);
        assert_eq!(&w[0..4], b"RIFF");
    }
}
