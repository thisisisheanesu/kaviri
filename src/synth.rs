//! The soundtrack for `kaviri motion`: a score and its sound effects, made from
//! nothing but arithmetic.
//!
//! A motion script says where its cuts are in bars and beats. The music is
//! written from the same numbers, so the kick lands on every beat the picture
//! pulses on, a build's riser ends exactly where the next scene starts, and the
//! drop's impact is the frame the logo hits. Nothing is sampled and nothing is
//! downloaded; the same script makes the same file, byte for byte.
//!
//! The score is sections (`intro`, `build`, `drop`, `break`, `outro`) over a
//! chord progression in one key. Each section turns instruments on and off:
//! kick, clap, hats, a snare roll, a sidechained bass, a pad, a plucked
//! arpeggio with a ping-pong delay, supersaw stabs, a riser and an impact.
//! Three styles re-voice the same arrangement: `pulse` (electronic, the
//! default), `cinematic` (low drums, drones and braams) and `minimal` (soft
//! keys and a gentle beat).

use crate::motion::Grid;
use serde_json::{json, Value};
use std::f64::consts::PI;
use std::path::Path;

pub const RATE: u32 = 48_000;
const SR: f64 = RATE as f64;

/// Sound effects a script may place with `{"op":"sfx"}`.
pub const SFX: &[&str] = &[
    "whoosh", "impact", "riser", "pop", "click", "tick", "swell", "glitch", "shimmer", "boom",
    "type", "chime",
];

pub const STYLES: &[&str] = &["pulse", "cinematic", "minimal"];
pub const PARTS: &[&str] = &["intro", "build", "drop", "break", "outro", "silence"];

/// A stereo buffer.
pub struct Stereo {
    pub l: Vec<f32>,
    pub r: Vec<f32>,
}

impl Stereo {
    pub fn new(secs: f64) -> Stereo {
        let n = (secs.max(0.0) * SR).ceil() as usize + 1;
        Stereo {
            l: vec![0.0; n],
            r: vec![0.0; n],
        }
    }
    fn len(&self) -> usize {
        self.l.len()
    }
    /// Add a mono sample at index `i` with an equal-power pan in -1..1.
    #[inline]
    fn add(&mut self, i: usize, v: f64, pan: f64) {
        if i < self.l.len() {
            let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
            self.l[i] += (v * a.cos()) as f32;
            self.r[i] += (v * a.sin()) as f32;
        }
    }
    #[inline]
    fn add2(&mut self, i: usize, l: f64, r: f64) {
        if i < self.l.len() {
            self.l[i] += l as f32;
            self.r[i] += r as f32;
        }
    }
    fn mix_from(&mut self, o: &Stereo, gain: f64) {
        let g = gain as f32;
        for i in 0..self.len().min(o.len()) {
            self.l[i] += o.l[i] * g;
            self.r[i] += o.r[i] * g;
        }
    }
}

/// A sound placed on the timeline.
#[derive(Clone, Debug)]
pub struct Cue {
    pub kind: String,
    pub at: f64,
    pub gain: f64,
    pub pan: f64,
    pub pitch: f64,
    pub dur: Option<f64>,
}

/// Deterministic noise. The seed is the whole of the randomness in a take.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    #[inline]
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    #[inline]
    fn bi(&mut self) -> f64 {
        self.next() * 2.0 - 1.0
    }
}

/// A topology-preserving state variable filter: lowpass, bandpass and highpass
/// at once, stable under a cutoff that moves every sample.
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f64,
    ic2: f64,
}

impl Svf {
    /// Returns (low, band, high).
    #[inline]
    fn run(&mut self, x: f64, cutoff: f64, q: f64) -> (f64, f64, f64) {
        let fc = cutoff.clamp(20.0, SR * 0.45);
        let g = (PI * fc / SR).tan();
        let k = 1.0 / q.max(0.05);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let low = v2;
        let band = v1;
        let high = x - k * v1 - v2;
        (low, band, high)
    }
}

/// Band-limited sawtooth step, so high notes do not alias into a whistle.
#[inline]
fn poly_blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Default)]
struct Saw {
    phase: f64,
}

impl Saw {
    #[inline]
    fn next(&mut self, hz: f64) -> f64 {
        let dt = (hz / SR).min(0.5);
        let v = 2.0 * self.phase - 1.0 - poly_blep(self.phase, dt);
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        v
    }
}

fn midi_hz(n: f64) -> f64 {
    440.0 * 2f64.powf((n - 69.0) / 12.0)
}

/// A key: a root and a scale.
#[derive(Clone, Copy, Debug)]
struct Key {
    root: i32,
    minor: bool,
}

fn parse_key(s: &str) -> Result<Key, String> {
    let t = s.trim();
    let mut chars = t.chars();
    let letter = chars.next().ok_or("the key is empty")?.to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => {
            return Err(format!(
                "\"{s}\" is not a key (like \"F#m\", \"Eb\", \"A minor\")"
            ))
        }
    };
    let rest: String = chars.collect();
    let (acc, rest) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest.as_str())
    };
    let r = rest.trim().to_ascii_lowercase();
    let minor = match r.as_str() {
        "m" | "min" | "minor" | "-" => true,
        "" | "maj" | "major" | "M" => false,
        other => return Err(format!("\"{s}\": \"{other}\" is not major or minor")),
    };
    Ok(Key {
        root: (base + acc + 12) % 12,
        minor,
    })
}

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];

/// A chord as scale degrees, from a roman numeral. The quality comes from the
/// scale, so "v" and "V" are the same chord; "7" adds the seventh.
fn parse_numeral(s: &str) -> Result<(usize, bool), String> {
    let t = s.trim();
    let (body, seventh) = match t.strip_suffix('7') {
        Some(b) => (b, true),
        None => (t, false),
    };
    let deg = match body.to_ascii_lowercase().as_str() {
        "i" => 0,
        "ii" => 1,
        "iii" => 2,
        "iv" => 3,
        "v" => 4,
        "vi" => 5,
        "vii" => 6,
        _ => {
            return Err(format!(
                "\"{s}\" is not a chord; write roman numerals like \"i\", \"VI\", \"iv7\""
            ))
        }
    };
    Ok((deg, seventh))
}

impl Key {
    fn scale(&self) -> &'static [i32; 7] {
        if self.minor {
            &MINOR
        } else {
            &MAJOR
        }
    }
    /// Semitones above the key's root for a scale degree, across octaves.
    fn degree(&self, d: i32) -> i32 {
        let s = self.scale();
        let o = d.div_euclid(7);
        s[d.rem_euclid(7) as usize] + 12 * o
    }
    /// The chord's notes as semitone offsets from the root of the key.
    fn chord(&self, deg: usize, seventh: bool) -> Vec<i32> {
        let d = deg as i32;
        let mut v = vec![self.degree(d), self.degree(d + 2), self.degree(d + 4)];
        if seventh {
            v.push(self.degree(d + 6));
        }
        v
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub bars: f64,
    pub part: String,
}

#[derive(Clone, Debug)]
pub struct Music {
    grid: Grid,
    key: Key,
    chords: Vec<(usize, bool)>,
    chord_bars: f64,
    sections: Vec<Section>,
    style: String,
    gain: f64,
    seed: u64,
    pub fade_out: f64,
}

fn energy_of(part: &str) -> f64 {
    match part {
        "intro" => 0.35,
        "build" => 0.65,
        "drop" => 1.0,
        "break" => 0.4,
        "outro" => 0.5,
        _ => 0.0,
    }
}

impl Music {
    pub fn from_op(op: &Value, grid: Grid) -> Result<Music, String> {
        let key = parse_key(op.get("key").and_then(Value::as_str).unwrap_or("Am"))?;
        let default_prog: &[&str] = if key.minor {
            &["i", "VI", "III", "VII"]
        } else {
            &["I", "V", "vi", "IV"]
        };
        let chords = match op.get("progression") {
            Some(Value::Array(a)) => {
                let mut v = Vec::new();
                for c in a {
                    let s = c
                        .as_str()
                        .ok_or("progression is a list of roman numerals")?;
                    v.push(parse_numeral(s)?);
                }
                if v.is_empty() {
                    return Err("progression is empty".into());
                }
                v
            }
            Some(_) => return Err("progression is a list of roman numerals".into()),
            None => default_prog
                .iter()
                .map(|s| parse_numeral(s))
                .collect::<Result<_, _>>()?,
        };
        let style = op
            .get("style")
            .and_then(Value::as_str)
            .unwrap_or("pulse")
            .to_string();
        if !STYLES.contains(&style.as_str()) {
            return Err(format!(
                "unknown music style \"{style}\" (one of: {})",
                STYLES.join(", ")
            ));
        }
        let sections = match op.get("sections") {
            Some(Value::Array(a)) => {
                let mut v = Vec::new();
                for (i, s) in a.iter().enumerate() {
                    let bars = s["bars"]
                        .as_f64()
                        .filter(|b| *b > 0.0 && *b <= 256.0)
                        .ok_or_else(|| format!("sections[{i}] needs \"bars\" between 0 and 256"))?;
                    let part = s["part"].as_str().unwrap_or("drop").to_string();
                    if !PARTS.contains(&part.as_str()) {
                        return Err(format!(
                            "sections[{i}]: unknown part \"{part}\" (one of: {})",
                            PARTS.join(", ")
                        ));
                    }
                    v.push(Section { bars, part });
                }
                if v.is_empty() {
                    return Err("sections is empty".into());
                }
                v
            }
            Some(_) => return Err("sections is a list of {\"bars\":…, \"part\":…}".into()),
            None => {
                let bars = op.get("bars").and_then(Value::as_f64).unwrap_or(16.0);
                vec![
                    Section {
                        bars: 2.0,
                        part: "intro".into(),
                    },
                    Section {
                        bars: 2.0,
                        part: "build".into(),
                    },
                    Section {
                        bars: (bars - 6.0).max(1.0),
                        part: "drop".into(),
                    },
                    Section {
                        bars: 2.0,
                        part: "outro".into(),
                    },
                ]
            }
        };
        let fade_out = op.get("fade_out").and_then(Value::as_f64).unwrap_or(1.5);
        Ok(Music {
            grid,
            key,
            chords,
            chord_bars: op
                .get("chord_bars")
                .and_then(Value::as_f64)
                .unwrap_or(1.0)
                .max(0.25),
            sections,
            style,
            gain: op.get("gain").and_then(Value::as_f64).unwrap_or(1.0),
            seed: op.get("seed").and_then(Value::as_u64).unwrap_or(7),
            fade_out,
        })
    }

    /// Seconds of score, not counting the tail an outro rings out into.
    pub fn length(&self) -> f64 {
        self.grid.offset + self.sections.iter().map(|s| s.bars).sum::<f64>() * self.grid.bar()
    }

    /// Where each section sits, for the picture to pulse harder when the music does.
    pub fn energy_map(&self) -> Vec<Value> {
        let mut t = self.grid.offset;
        self.sections
            .iter()
            .map(|s| {
                let d = s.bars * self.grid.bar();
                let v =
                    json!({"start": t, "end": t + d, "part": s.part, "energy": energy_of(&s.part)});
                t += d;
                v
            })
            .collect()
    }

    /// The chord sounding at bar `b` (0-based, fractional).
    fn chord_at(&self, bar: f64) -> Vec<i32> {
        let idx = (bar / self.chord_bars).floor().max(0.0) as usize % self.chords.len();
        let (d, s) = self.chords[idx];
        self.key.chord(d, s)
    }

    pub fn render(&self, len: f64) -> Stereo {
        let total = len.max(self.length()) + 3.0;
        let mut drums = Stereo::new(total);
        let mut bass = Stereo::new(total);
        let mut music = Stereo::new(total);
        let mut send = Stereo::new(total);
        let mut delay_send = Stereo::new(total);
        let mut rng = Rng::new(self.seed);
        let beat = self.grid.beat();
        let bar = self.grid.bar();
        let bpb = self.grid.beats_per_bar.round().max(1.0) as usize;
        let mut kicks: Vec<(f64, f64)> = Vec::new();
        let cinematic = self.style == "cinematic";
        let minimal = self.style == "minimal";
        let root = self.key.root;

        let mut t = self.grid.offset;
        let mut bar_index = 0.0f64;
        let n_sections = self.sections.len();
        for (si, sec) in self.sections.iter().enumerate() {
            let s0 = t;
            let sdur = sec.bars * bar;
            let part = sec.part.as_str();
            let next_part = self.sections.get(si + 1).map(|s| s.part.as_str());
            let whole_bars = sec.bars.ceil() as usize;

            // An impact opens every drop and the outro, on the downbeat.
            if (part == "drop" && (si == 0 || self.sections[si - 1].part != "drop"))
                || (part == "outro" && si > 0)
            {
                impact(&mut music, &mut send, s0, 1.0, &mut rng, cinematic);
                if !minimal {
                    crash(&mut drums, &mut send, s0, 0.5, &mut rng);
                }
            }

            for b in 0..whole_bars {
                let bt = s0 + b as f64 * bar;
                if bt >= s0 + sdur - 1e-9 {
                    break;
                }
                let bar_abs = bar_index + b as f64;
                let chord = self.chord_at(bar_abs);
                let chord_dur = bar.min(s0 + sdur - bt);
                let last_bar = b + 1 == whole_bars;
                let into_drop = next_part == Some("drop");
                let progress = (b as f64 + 0.5) / sec.bars;

                // Pad: every part but silence.
                if part != "silence" {
                    let cutoff = match part {
                        "intro" => 500.0 + 500.0 * progress,
                        "build" => 700.0 + 1600.0 * progress,
                        "drop" => 2200.0,
                        "break" => 900.0,
                        _ => 1200.0,
                    };
                    let g = match part {
                        "drop" => 0.075,
                        "outro" => 0.1,
                        _ => 0.09,
                    };
                    let voicing: Vec<f64> = chord
                        .iter()
                        .map(|n| (48 + root + n) as f64)
                        .map(|n| if n > 63.0 { n - 12.0 } else { n })
                        .collect();
                    let hold = if part == "outro" && last_bar && si + 1 == n_sections {
                        chord_dur + 2.5
                    } else {
                        chord_dur
                    };
                    if minimal {
                        keys_chord(&mut music, &mut send, bt, hold, &voicing, g * 1.6);
                    } else {
                        pad(
                            &mut music, &mut send, bt, hold, &voicing, cutoff, g, cinematic,
                        );
                    }
                }

                // Bass.
                let bass_note = (36 + root + chord[0]) as f64;
                let bass_note = if bass_note > 45.0 {
                    bass_note - 12.0
                } else {
                    bass_note
                };
                match part {
                    "drop" if cinematic => {
                        drone(&mut bass, bt, chord_dur, bass_note, 0.28);
                    }
                    "drop" if minimal => {
                        for k in 0..bpb {
                            if k % 2 == 0 {
                                bass_note_pluck(
                                    &mut bass,
                                    bt + k as f64 * beat,
                                    beat * 1.6,
                                    bass_note,
                                    0.32,
                                    500.0,
                                );
                            }
                        }
                    }
                    "drop" => {
                        // Rolling sixteenths that leave the downbeat to the kick.
                        for k in 0..(bpb * 4) {
                            if k % 4 == 0 {
                                continue;
                            }
                            let nt = bt + k as f64 * beat / 4.0;
                            let oct = if k % 8 == 6 { 12.0 } else { 0.0 };
                            bass_note_pluck(
                                &mut bass,
                                nt,
                                beat / 4.0 * 0.9,
                                bass_note + oct,
                                0.30,
                                900.0,
                            );
                        }
                    }
                    "break" | "outro" => {
                        drone(&mut bass, bt, chord_dur, bass_note, 0.16);
                    }
                    "build" if !cinematic => {
                        for k in 0..(bpb * 2) {
                            if k % 2 == 1 {
                                bass_note_pluck(
                                    &mut bass,
                                    bt + k as f64 * beat / 2.0,
                                    beat / 2.0 * 0.8,
                                    bass_note,
                                    0.16,
                                    400.0 + 800.0 * progress,
                                );
                            }
                        }
                    }
                    _ => {}
                }

                // Arpeggio.
                let arp_on = matches!(part, "intro" | "build" | "drop" | "break");
                if arp_on {
                    let open = match part {
                        "intro" => 0.15 + 0.2 * progress,
                        "build" => 0.3 + 0.6 * progress,
                        "drop" => 1.0,
                        _ => 0.35,
                    };
                    let g = match part {
                        "drop" => 0.07,
                        _ => 0.06,
                    };
                    let notes: Vec<f64> = {
                        let mut v: Vec<f64> =
                            chord.iter().map(|n| (60 + root + n) as f64).collect();
                        v.push(v[0] + 12.0);
                        v.push(v[1] + 12.0);
                        v
                    };
                    let pattern = [0usize, 2, 1, 3, 2, 4, 3, 1];
                    let step = if cinematic || minimal {
                        beat / 2.0
                    } else {
                        beat / 4.0
                    };
                    let steps = (bar / step).round() as usize;
                    for k in 0..steps {
                        let nt = bt + k as f64 * step;
                        if nt >= s0 + sdur - 1e-9 {
                            break;
                        }
                        let n = notes[pattern[k % pattern.len()] % notes.len()];
                        let acc = if k % 4 == 0 { 1.0 } else { 0.72 };
                        pluck(
                            &mut music,
                            &mut delay_send,
                            nt,
                            step * 0.95,
                            n,
                            g * acc,
                            open,
                            if k % 2 == 0 { -0.25 } else { 0.25 },
                        );
                    }
                }

                // Stabs on the offbeats of a drop.
                if part == "drop" && !minimal {
                    let pos: &[f64] = if cinematic { &[0.0] } else { &[0.0, 1.5, 3.0] };
                    for p in pos {
                        let st = bt + p * beat;
                        if st < s0 + sdur - 1e-9 {
                            let v: Vec<f64> =
                                chord.iter().map(|n| (60 + root + n) as f64).collect();
                            if cinematic {
                                braam(&mut music, &mut send, st, bar * 0.9, &v, 0.07);
                            } else {
                                stab(&mut music, &mut send, st, beat * 0.45, &v, 0.05);
                            }
                        }
                    }
                }

                // Drums.
                for k in 0..bpb {
                    let bt_k = bt + k as f64 * beat;
                    if bt_k >= s0 + sdur - 1e-9 {
                        break;
                    }
                    // The half beat before a drop is left empty, so the drop lands.
                    let gap = into_drop && last_bar && k + 1 == bpb;
                    match part {
                        "drop" => {
                            if cinematic {
                                if k % 2 == 0 {
                                    tom(&mut drums, &mut send, bt_k, 1.0, 55.0);
                                    kicks.push((bt_k, 0.6));
                                }
                                if k == 3 {
                                    tom(&mut drums, &mut send, bt_k + beat * 0.5, 0.6, 80.0);
                                }
                            } else if minimal {
                                if k % 2 == 0 {
                                    kick(&mut drums, bt_k, 0.7, 0.8);
                                    kicks.push((bt_k, 0.4));
                                } else {
                                    rim(&mut drums, &mut send, bt_k, 0.35, &mut rng);
                                }
                                hat(&mut drums, bt_k + beat * 0.5, 0.10, false, &mut rng);
                            } else {
                                kick(&mut drums, bt_k, 1.0, 1.0);
                                kicks.push((bt_k, 1.0));
                                if k % 2 == 1 {
                                    clap(&mut drums, &mut send, bt_k, 0.55, &mut rng);
                                }
                                hat(&mut drums, bt_k + beat * 0.5, 0.2, k % 2 == 1, &mut rng);
                                hat(&mut drums, bt_k + beat * 0.25, 0.06, false, &mut rng);
                                hat(&mut drums, bt_k + beat * 0.75, 0.06, false, &mut rng);
                            }
                        }
                        "build" => {
                            if !gap {
                                if cinematic {
                                    if k == 0 {
                                        tom(&mut drums, &mut send, bt_k, 0.7, 60.0);
                                    }
                                } else {
                                    kick(&mut drums, bt_k, 0.55 + 0.35 * progress, 0.9);
                                    kicks.push((bt_k, 0.5));
                                }
                            }
                            // A roll that doubles on the last bar.
                            let div = if last_bar {
                                4
                            } else if progress > 0.5 {
                                2
                            } else {
                                1
                            };
                            for j in 0..div {
                                let rt = bt_k + j as f64 * beat / div as f64;
                                if gap && j * 2 >= div {
                                    break;
                                }
                                let vel =
                                    0.12 + 0.3 * progress * (0.6 + 0.4 * (j as f64 / div as f64));
                                snare(&mut drums, &mut send, rt, vel, &mut rng);
                            }
                        }
                        "intro" => {
                            if k == 0 && progress > 0.4 && !cinematic {
                                kick(&mut drums, bt_k, 0.4, 0.8);
                            }
                            if !cinematic {
                                hat(&mut drums, bt_k + beat * 0.5, 0.05, false, &mut rng);
                            }
                        }
                        "break" => {
                            if k == 0 && !minimal {
                                kick(&mut drums, bt_k, 0.35, 0.7);
                            }
                            hat(&mut drums, bt_k + beat * 0.5, 0.04, false, &mut rng);
                        }
                        _ => {}
                    }
                }

                // A riser across a build, ending on the next downbeat.
                if part == "build" && b == 0 {
                    riser(&mut music, &mut send, s0, sdur, 0.2, &mut rng);
                }
            }
            t += sdur;
            bar_index += sec.bars;
        }

        // Sidechain: everything melodic ducks under the kick.
        let duck = duck_curve(drums.len(), &kicks, beat);
        for buf in [&mut bass, &mut music] {
            duck_apply(buf, &duck);
        }
        let mut delayed = pingpong(&delay_send, beat * 0.75, 0.38);
        send.mix_from(&delayed, 0.5);
        duck_apply(&mut delayed, &duck);
        let verb = reverb(&send, 0.86, 0.35);

        let mut out = Stereo::new(total);
        out.mix_from(&drums, 0.9);
        out.mix_from(&bass, 1.0);
        out.mix_from(&music, 1.0);
        out.mix_from(&delayed, 0.55);
        out.mix_from(&verb, 0.35);
        for i in 0..out.len() {
            out.l[i] *= self.gain as f32;
            out.r[i] *= self.gain as f32;
        }
        out.l.truncate(((len + 0.01) * SR) as usize + 1);
        out.r.truncate(((len + 0.01) * SR) as usize + 1);
        out
    }
}

fn duck_apply(buf: &mut Stereo, duck: &[f32]) {
    for ((l, r), d) in buf.l.iter_mut().zip(buf.r.iter_mut()).zip(duck) {
        *l *= d;
        *r *= d;
    }
}

fn duck_curve(n: usize, kicks: &[(f64, f64)], beat: f64) -> Vec<f32> {
    let mut d = vec![1.0f32; n];
    let release = (beat * 0.6).min(0.35);
    for &(t, depth) in kicks {
        let i0 = (t * SR) as usize;
        let len = (release * SR) as usize;
        for j in 0..len {
            let i = i0 + j;
            if i >= n {
                break;
            }
            let x = j as f64 / len as f64;
            // Fast attack, curved release.
            let g = 1.0 - depth * 0.7 * (1.0 - x).powf(2.2);
            d[i] = d[i].min(g as f32);
        }
    }
    d
}

fn idx(t: f64) -> usize {
    (t.max(0.0) * SR) as usize
}

fn kick(buf: &mut Stereo, t: f64, vel: f64, tight: f64) {
    let n = (0.45 * SR) as usize;
    let i0 = idx(t);
    let mut ph = 0.0f64;
    let mut click_rng = Rng::new((t * 1000.0) as u64 + 3);
    for j in 0..n {
        let x = j as f64 / SR;
        let f = 44.0 + 120.0 * (-x * 32.0).exp() + 30.0 * (-x * 8.0).exp();
        ph += 2.0 * PI * f / SR;
        let env = (-x * 6.5 * tight).exp() * (1.0 - (-x * 900.0).exp());
        let body = ph.sin() * env;
        let click = click_rng.bi() * (-x * 420.0).exp() * 0.25;
        let v = ((body * 1.6 + click).tanh()) * vel * 0.95;
        buf.add(i0 + j, v, 0.0);
    }
}

fn tom(buf: &mut Stereo, send: &mut Stereo, t: f64, vel: f64, f0: f64) {
    let n = (1.1 * SR) as usize;
    let i0 = idx(t);
    let mut ph = 0.0f64;
    let mut r = Rng::new((t * 977.0) as u64 + 11);
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let hz = f0 + f0 * 1.3 * (-x * 18.0).exp();
        ph += 2.0 * PI * hz / SR;
        let env = (-x * 3.2).exp();
        let (lo, _, _) = f.run(r.bi(), 400.0, 0.7);
        let v = ((ph.sin() * 1.3 + lo * 0.6 * (-x * 12.0).exp()) * env).tanh() * vel;
        buf.add(i0 + j, v, 0.0);
        send.add(i0 + j, v * 0.25, 0.0);
    }
}

fn clap(buf: &mut Stereo, send: &mut Stereo, t: f64, vel: f64, rng: &mut Rng) {
    let n = (0.35 * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let bursts = [0.0, 0.011, 0.022];
        let mut env = 0.0;
        for b in bursts {
            if x >= b {
                env += (-(x - b) * 190.0).exp() * 0.7;
            }
        }
        if x >= 0.03 {
            env += (-(x - 0.03) * 16.0).exp() * 0.55;
        }
        let (_, bp, _) = f.run(rng.bi(), 1250.0, 0.9);
        let v = bp * env * vel * 1.8;
        buf.add(i0 + j, v, 0.05);
        send.add(i0 + j, v * 0.5, 0.0);
    }
}

fn snare(buf: &mut Stereo, send: &mut Stereo, t: f64, vel: f64, rng: &mut Rng) {
    let n = (0.22 * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    let mut ph = 0.0f64;
    for j in 0..n {
        let x = j as f64 / SR;
        let (_, _, hp) = f.run(rng.bi(), 1800.0, 0.7);
        ph += 2.0 * PI * 185.0 / SR;
        let v = (hp * (-x * 24.0).exp() * 0.9 + ph.sin() * (-x * 40.0).exp() * 0.5) * vel;
        buf.add(i0 + j, v, -0.05);
        send.add(i0 + j, v * 0.4, 0.0);
    }
}

fn rim(buf: &mut Stereo, send: &mut Stereo, t: f64, vel: f64, rng: &mut Rng) {
    let n = (0.08 * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    let mut ph = 0.0f64;
    for j in 0..n {
        let x = j as f64 / SR;
        let (_, bp, _) = f.run(rng.bi(), 2400.0, 3.0);
        ph += 2.0 * PI * 820.0 / SR;
        let v = (bp * 0.8 + ph.sin() * 0.6) * (-x * 70.0).exp() * vel;
        buf.add(i0 + j, v, 0.15);
        send.add(i0 + j, v * 0.5, 0.0);
    }
}

fn hat(buf: &mut Stereo, t: f64, vel: f64, open: bool, rng: &mut Rng) {
    let dec = if open { 11.0 } else { 55.0 };
    let n = ((if open { 0.4 } else { 0.08 }) * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    let pan = if open { 0.25 } else { -0.2 };
    for j in 0..n {
        let x = j as f64 / SR;
        let (_, _, hp) = f.run(rng.bi(), 7500.0, 0.8);
        buf.add(i0 + j, hp * (-x * dec).exp() * vel, pan);
    }
}

fn crash(buf: &mut Stereo, send: &mut Stereo, t: f64, vel: f64, rng: &mut Rng) {
    let n = (2.2 * SR) as usize;
    let i0 = idx(t);
    let mut fl = Svf::default();
    let mut fr = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let env = (-x * 2.2).exp() * (1.0 - (-x * 400.0).exp());
        let (_, _, l) = fl.run(rng.bi(), 5000.0, 0.6);
        let (_, _, r) = fr.run(rng.bi(), 5000.0, 0.6);
        buf.add2(i0 + j, l * env * vel * 0.5, r * env * vel * 0.5);
        send.add2(i0 + j, l * env * vel * 0.2, r * env * vel * 0.2);
    }
}

fn impact(buf: &mut Stereo, send: &mut Stereo, t: f64, vel: f64, rng: &mut Rng, big: bool) {
    let n = ((if big { 3.0 } else { 2.2 }) * SR) as usize;
    let i0 = idx(t);
    let mut ph = 0.0f64;
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let hz = 32.0 + 70.0 * (-x * 9.0).exp();
        ph += 2.0 * PI * hz / SR;
        let sub = ph.sin() * (-x * 1.6).exp();
        let (lo, _, _) = f.run(rng.bi(), 180.0 + 2500.0 * (-x * 14.0).exp(), 0.7);
        let noise = lo * (-x * 5.0).exp() * 0.9;
        let v = ((sub * 1.2 + noise) * 1.2).tanh() * vel * 0.6;
        buf.add(i0 + j, v, 0.0);
        send.add(i0 + j, noise * vel * 0.4, 0.0);
    }
}

/// A slow chord of detuned saws, filtered and spread across the stereo field.
#[allow(clippy::too_many_arguments)]
fn pad(
    buf: &mut Stereo,
    send: &mut Stereo,
    t: f64,
    dur: f64,
    notes: &[f64],
    cutoff: f64,
    gain: f64,
    dark: bool,
) {
    let attack = 0.25_f64.min(dur * 0.4);
    let release = 0.5;
    let n = ((dur + release) * SR) as usize;
    let i0 = idx(t);
    let detune_l = [-0.11, 0.0, 0.07];
    let detune_r = [-0.06, 0.02, 0.12];
    for (ni, &note) in notes.iter().enumerate() {
        let mut oscl = [Saw::default(); 3];
        let mut oscr = [Saw::default(); 3];
        for (k, o) in oscl.iter_mut().enumerate() {
            o.phase = ((ni * 3 + k) as f64 * 0.37) % 1.0;
        }
        for (k, o) in oscr.iter_mut().enumerate() {
            o.phase = ((ni * 3 + k) as f64 * 0.61) % 1.0;
        }
        let mut fl = Svf::default();
        let mut fr = Svf::default();
        for j in 0..n {
            let x = j as f64 / SR;
            let env = if x < attack {
                (x / attack).powf(1.5)
            } else if x < dur {
                1.0
            } else {
                (1.0 - (x - dur) / release).max(0.0).powf(2.0)
            };
            let lfo = (2.0 * PI * 0.18 * (t + x)).sin();
            let c = cutoff * (1.0 + 0.25 * lfo) * if dark { 0.7 } else { 1.0 };
            let mut l = 0.0;
            let mut r = 0.0;
            for k in 0..3 {
                l += oscl[k].next(midi_hz(note + detune_l[k]));
                r += oscr[k].next(midi_hz(note + detune_r[k]));
            }
            let (l, _, _) = fl.run(l / 3.0, c, 0.6);
            let (r, _, _) = fr.run(r / 3.0, c, 0.6);
            buf.add2(i0 + j, l * env * gain, r * env * gain);
            send.add2(i0 + j, l * env * gain * 0.6, r * env * gain * 0.6);
        }
    }
}

/// An electric-piano-ish chord: sines with a bell partial.
fn keys_chord(buf: &mut Stereo, send: &mut Stereo, t: f64, dur: f64, notes: &[f64], gain: f64) {
    let n = ((dur + 0.8) * SR) as usize;
    let i0 = idx(t);
    for (ni, &note) in notes.iter().enumerate() {
        let hz = midi_hz(note + 12.0);
        let pan = (ni as f64 - 1.0) * 0.3;
        for j in 0..n {
            let x = j as f64 / SR;
            let env = (-x * 1.1).exp() * (1.0 - (-x * 300.0).exp());
            let v = ((2.0 * PI * hz * x).sin()
                + 0.3 * (2.0 * PI * hz * 2.0 * x).sin() * (-x * 6.0).exp()
                + 0.12 * (2.0 * PI * hz * 7.0 * x).sin() * (-x * 14.0).exp())
                * env
                * gain;
            buf.add(i0 + j, v, pan);
            send.add(i0 + j, v * 0.5, pan);
        }
    }
}

fn drone(buf: &mut Stereo, t: f64, dur: f64, note: f64, gain: f64) {
    let n = ((dur + 0.1) * SR) as usize;
    let i0 = idx(t);
    let hz = midi_hz(note);
    let mut s = Saw::default();
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let env = (x / 0.05).min(1.0) * ((dur + 0.1 - x) / 0.1).clamp(0.0, 1.0);
        let sub = (2.0 * PI * hz * x).sin();
        let (lo, _, _) = f.run(s.next(hz), 260.0, 0.7);
        buf.add(i0 + j, (sub * 0.8 + lo * 0.5) * env * gain, 0.0);
    }
}

fn bass_note_pluck(buf: &mut Stereo, t: f64, dur: f64, note: f64, gain: f64, bright: f64) {
    let n = ((dur + 0.02) * SR) as usize;
    let i0 = idx(t);
    let hz = midi_hz(note);
    let mut s = Saw::default();
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let env = (x / 0.003).min(1.0)
            * (0.75 + 0.25 * (-x * 12.0).exp())
            * ((dur + 0.02 - x) / 0.02).clamp(0.0, 1.0);
        let cutoff = 120.0 + bright * (-x * 16.0).exp();
        let (lo, _, _) = f.run(
            s.next(hz) * 0.8 + (2.0 * PI * hz * 0.5 * x).sin() * 0.5,
            cutoff,
            0.9,
        );
        buf.add(i0 + j, (lo * 1.4).tanh() * env * gain, 0.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn pluck(
    buf: &mut Stereo,
    delay_send: &mut Stereo,
    t: f64,
    dur: f64,
    note: f64,
    gain: f64,
    open: f64,
    pan: f64,
) {
    let tail = 0.25;
    let n = ((dur + tail) * SR) as usize;
    let i0 = idx(t);
    let hz = midi_hz(note);
    let mut a = Saw::default();
    let mut b = Saw::default();
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let env = (x / 0.002).min(1.0) * (-x * 9.0).exp();
        let cutoff = 350.0 + (900.0 + 5200.0 * open) * (-x * 22.0).exp();
        let raw = a.next(hz) * 0.6 + b.next(hz * 1.005) * 0.4;
        let (lo, _, _) = f.run(raw, cutoff, 1.1);
        let v = lo * env * gain;
        buf.add(i0 + j, v, pan);
        delay_send.add(i0 + j, v, pan);
    }
}

fn stab(buf: &mut Stereo, send: &mut Stereo, t: f64, dur: f64, notes: &[f64], gain: f64) {
    let n = ((dur + 0.15) * SR) as usize;
    let i0 = idx(t);
    let det = [-0.18, -0.07, 0.0, 0.08, 0.19];
    for &note in notes {
        let mut osc = [Saw::default(); 5];
        for (k, o) in osc.iter_mut().enumerate() {
            o.phase = k as f64 * 0.21;
        }
        let mut fl = Svf::default();
        let mut fr = Svf::default();
        for j in 0..n {
            let x = j as f64 / SR;
            let env = (x / 0.003).min(1.0)
                * if x < dur {
                    (-x * 5.0).exp()
                } else {
                    (-dur * 5.0).exp() * (1.0 - (x - dur) / 0.15).max(0.0)
                };
            let mut l = 0.0;
            let mut r = 0.0;
            for (k, o) in osc.iter_mut().enumerate() {
                let v = o.next(midi_hz(note + det[k]));
                if k % 2 == 0 {
                    l += v;
                } else {
                    r += v;
                }
                if k == 2 {
                    r += v;
                }
            }
            let c = 1800.0 + 4000.0 * (-x * 10.0).exp();
            let (l, _, _) = fl.run(l / 3.0, c, 0.7);
            let (r, _, _) = fr.run(r / 3.0, c, 0.7);
            buf.add2(i0 + j, l * env * gain, r * env * gain);
            send.add2(i0 + j, l * env * gain * 0.5, r * env * gain * 0.5);
        }
    }
}

/// The low brass blast of a trailer: a detuned saw chord an octave down, filter swelling open.
fn braam(buf: &mut Stereo, send: &mut Stereo, t: f64, dur: f64, notes: &[f64], gain: f64) {
    let n = ((dur + 0.6) * SR) as usize;
    let i0 = idx(t);
    for &note in notes {
        let mut a = Saw::default();
        let mut b = Saw::default();
        let mut fl = Svf::default();
        let mut fr = Svf::default();
        for j in 0..n {
            let x = j as f64 / SR;
            let env = (x / 0.08).min(1.0)
                * if x < dur {
                    1.0 - 0.4 * x / dur
                } else {
                    0.6 * (1.0 - (x - dur) / 0.6).max(0.0)
                };
            let c = 200.0 + 1400.0 * (x / 0.3).min(1.0) * (-x * 1.5).exp();
            let (l, _, _) = fl.run(a.next(midi_hz(note - 12.0 - 0.1)), c, 0.9);
            let (r, _, _) = fr.run(b.next(midi_hz(note - 12.0 + 0.1)), c, 0.9);
            let l = (l * 2.0).tanh();
            let r = (r * 2.0).tanh();
            buf.add2(i0 + j, l * env * gain, r * env * gain);
            send.add2(i0 + j, l * env * gain * 0.4, r * env * gain * 0.4);
        }
    }
}

fn riser(buf: &mut Stereo, send: &mut Stereo, t: f64, dur: f64, gain: f64, rng: &mut Rng) {
    let n = (dur * SR) as usize;
    let i0 = idx(t);
    let mut fl = Svf::default();
    let mut fr = Svf::default();
    let mut s = Saw::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let p = x / dur;
        let c = 300.0 * (20.0f64).powf(p);
        let (_, l, _) = fl.run(rng.bi(), c, 2.0);
        let (_, r, _) = fr.run(rng.bi(), c * 1.05, 2.0);
        let env = p.powf(2.2) * gain;
        let tone = s.next(midi_hz(48.0 + 24.0 * p)) * 0.12 * p.powf(3.0);
        buf.add2(i0 + j, (l + tone) * env, (r + tone) * env);
        send.add2(i0 + j, l * env * 0.5, r * env * 0.5);
    }
}

fn pingpong(src: &Stereo, delay: f64, fb: f64) -> Stereo {
    let d = (delay * SR) as usize;
    let n = src.len();
    let mut out = Stereo {
        l: vec![0.0; n],
        r: vec![0.0; n],
    };
    if d == 0 {
        return out;
    }
    let mut lp_l = 0.0f32;
    let mut lp_r = 0.0f32;
    for i in d..n {
        // Mono in, bouncing left and right, darkening each time round.
        let inp = (src.l[i - d] + src.r[i - d]) * 0.5;
        let l = inp + out.r[i - d] * fb as f32;
        let r = out.l[i - d] * fb as f32;
        lp_l += 0.35 * (l - lp_l);
        lp_r += 0.35 * (r - lp_r);
        out.l[i] = lp_l;
        out.r[i] = lp_r;
    }
    out
}

/// A Freeverb-style room: parallel damped combs into series allpasses, per side.
fn reverb(src: &Stereo, room: f64, damp: f64) -> Stereo {
    const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const ALLP: [usize; 4] = [556, 441, 341, 225];
    let scale = SR / 44_100.0;
    let n = src.len();
    let mut out = Stereo {
        l: vec![0.0; n],
        r: vec![0.0; n],
    };
    for (side, spread) in [(0usize, 0usize), (1, 23)] {
        let input = if side == 0 { &src.l } else { &src.r };
        let mut acc = vec![0.0f32; n];
        for c in COMBS {
            let len = ((c + spread) as f64 * scale) as usize;
            let mut buf = vec![0.0f32; len];
            let mut pos = 0usize;
            let mut store = 0.0f32;
            for i in 0..n {
                let o = buf[pos];
                store = o * (1.0 - damp as f32) + store * damp as f32;
                buf[pos] = input[i] * 0.015 + store * room as f32;
                pos += 1;
                if pos == len {
                    pos = 0;
                }
                acc[i] += o;
            }
        }
        for a in ALLP {
            let len = ((a + spread) as f64 * scale) as usize;
            let mut buf = vec![0.0f32; len];
            let mut pos = 0usize;
            for s in acc.iter_mut() {
                let b = buf[pos];
                let o = -*s + b;
                buf[pos] = *s + b * 0.5;
                pos += 1;
                if pos == len {
                    pos = 0;
                }
                *s = o;
            }
        }
        if side == 0 {
            out.l = acc;
        } else {
            out.r = acc;
        }
    }
    out
}

/// Every cue, rendered on top of what is already there, with a room of its own.
pub fn render_cues(mix: &mut Stereo, cues: &[Cue], grid: Grid) {
    if cues.is_empty() {
        return;
    }
    let secs = mix.len() as f64 / SR + 3.0;
    let mut dry = Stereo::new(secs);
    let mut send = Stereo::new(secs);
    for (ci, c) in cues.iter().enumerate() {
        let mut rng = Rng::new(ci as u64 * 7919 + 17);
        let g = c.gain;
        let t = c.at;
        match c.kind.as_str() {
            "whoosh" => {
                let d = c.dur.unwrap_or(0.5).clamp(0.12, 3.0);
                whoosh(&mut dry, &mut send, t, d, g, c.pan, &mut rng);
            }
            "impact" => {
                impact(&mut dry, &mut send, t, g, &mut rng, true);
                crash(&mut dry, &mut send, t, g * 0.4, &mut rng);
            }
            "boom" => impact(&mut dry, &mut send, t, g, &mut rng, false),
            "riser" => {
                let d = c.dur.unwrap_or(grid.bar()).clamp(0.2, 30.0);
                riser(&mut dry, &mut send, t, d, g * 0.5, &mut rng);
            }
            "swell" => {
                let d = c.dur.unwrap_or(grid.beat() * 2.0).clamp(0.2, 10.0);
                swell(&mut dry, &mut send, t, d, g, &mut rng);
            }
            "pop" => pop(&mut dry, &mut send, t, g, c.pitch, c.pan),
            "click" => click(&mut dry, t, g, c.pan, &mut rng),
            "tick" => tick(&mut dry, t, g * 0.7, c.pitch, c.pan),
            "glitch" => glitch(
                &mut dry,
                t,
                c.dur.unwrap_or(0.3).clamp(0.05, 2.0),
                g,
                &mut rng,
            ),
            "shimmer" => shimmer(&mut dry, &mut send, t, g, c.pitch),
            "chime" => chime(&mut dry, &mut send, t, g, c.pitch),
            "type" => {
                let d = c.dur.unwrap_or(1.0).clamp(0.05, 30.0);
                let n = (d * 22.0).round().max(1.0) as usize;
                for k in 0..n {
                    let jitter = (rng.next() - 0.5) * 0.35 * d / n as f64;
                    let kt = t + k as f64 * d / n as f64 + jitter;
                    click(
                        &mut dry,
                        kt,
                        g * (0.5 + 0.3 * rng.next()),
                        (rng.next() - 0.5) * 0.3,
                        &mut rng,
                    );
                }
            }
            _ => {}
        }
    }
    let verb = reverb(&send, 0.84, 0.3);
    for i in 0..mix.len() {
        mix.l[i] += dry.l[i] + verb.l[i] * 0.35;
        mix.r[i] += dry.r[i] + verb.r[i] * 0.35;
    }
}

fn whoosh(buf: &mut Stereo, send: &mut Stereo, t: f64, d: f64, g: f64, pan0: f64, rng: &mut Rng) {
    let n = (d * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    let mut f2 = Svf::default();
    for j in 0..n {
        let p = j as f64 / n as f64;
        let bell = (PI * p).sin().powf(1.6);
        let c = 350.0 + 3800.0 * (PI * p).sin().powf(2.0);
        let x = rng.bi();
        let (_, bp, _) = f.run(x, c, 1.4);
        let (_, bp2, _) = f2.run(x, c * 1.9, 2.0);
        let v = (bp * 0.9 + bp2 * 0.4) * bell * g * 0.8;
        let pan = (pan0 + (p * 2.0 - 1.0) * 0.8).clamp(-1.0, 1.0);
        buf.add(i0 + j, v, pan);
        send.add(i0 + j, v * 0.4, pan);
    }
}

fn swell(buf: &mut Stereo, send: &mut Stereo, t: f64, d: f64, g: f64, rng: &mut Rng) {
    let n = (d * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    for j in 0..n {
        let p = j as f64 / n as f64;
        let env = p.powf(3.0);
        let (_, _, hp) = f.run(rng.bi(), 2500.0 + 5000.0 * p, 0.7);
        let v = hp * env * g * 0.5;
        buf.add(i0 + j, v, 0.0);
        send.add(i0 + j, v * 0.5, 0.0);
    }
}

fn pop(buf: &mut Stereo, send: &mut Stereo, t: f64, g: f64, pitch: f64, pan: f64) {
    let n = (0.12 * SR) as usize;
    let i0 = idx(t);
    let mut ph = 0.0f64;
    let p = pitch.clamp(0.25, 4.0);
    for j in 0..n {
        let x = j as f64 / SR;
        let hz = (260.0 + 700.0 * (-x * 45.0).exp()) * p;
        ph += 2.0 * PI * hz / SR;
        let v = ph.sin() * (-x * 38.0).exp() * (1.0 - (-x * 2000.0).exp()) * g * 0.55;
        buf.add(i0 + j, v, pan);
        send.add(i0 + j, v * 0.3, pan);
    }
}

fn click(buf: &mut Stereo, t: f64, g: f64, pan: f64, rng: &mut Rng) {
    let n = (0.03 * SR) as usize;
    let i0 = idx(t);
    let mut f = Svf::default();
    for j in 0..n {
        let x = j as f64 / SR;
        let (_, bp, _) = f.run(rng.bi(), 3200.0, 1.5);
        let tone = (2.0 * PI * 1900.0 * x).sin() * (-x * 260.0).exp() * 0.4;
        buf.add(i0 + j, (bp * (-x * 320.0).exp() + tone) * g * 0.7, pan);
    }
}

fn tick(buf: &mut Stereo, t: f64, g: f64, pitch: f64, pan: f64) {
    let n = (0.05 * SR) as usize;
    let i0 = idx(t);
    let hz = 2600.0 * pitch.clamp(0.25, 4.0);
    for j in 0..n {
        let x = j as f64 / SR;
        buf.add(
            i0 + j,
            (2.0 * PI * hz * x).sin() * (-x * 140.0).exp() * g * 0.5,
            pan,
        );
    }
}

fn glitch(buf: &mut Stereo, t: f64, d: f64, g: f64, rng: &mut Rng) {
    let n = (d * SR) as usize;
    let i0 = idx(t);
    let mut hold = 0.0;
    let mut seg = 0usize;
    let mut hz = 400.0;
    let mut on = true;
    for j in 0..n {
        if seg == 0 {
            seg = (SR * (0.008 + rng.next() * 0.03)) as usize;
            hz = 120.0 + rng.next() * 1600.0;
            on = rng.next() > 0.25;
        }
        seg -= 1;
        if j % 6 == 0 {
            // Sample-and-hold gives it the crushed, stepped sound.
            hold = if (j as f64 * hz / SR).fract() < 0.5 {
                1.0
            } else {
                -1.0
            };
            hold += rng.bi() * 0.4;
        }
        let v = if on { hold * g * 0.22 } else { 0.0 };
        buf.add(i0 + j, v, if (j / 2400) % 2 == 0 { -0.5 } else { 0.5 });
    }
}

fn shimmer(buf: &mut Stereo, send: &mut Stereo, t: f64, g: f64, pitch: f64) {
    let base = 84.0 + 12.0 * pitch.log2().clamp(-2.0, 2.0);
    let notes = [0.0, 7.0, 12.0, 16.0, 19.0, 24.0];
    for (k, n) in notes.iter().enumerate() {
        let nt = t + k as f64 * 0.045;
        let hz = midi_hz(base + n);
        let len = (1.2 * SR) as usize;
        let i0 = idx(nt);
        let pan = (k as f64 / 5.0) * 1.2 - 0.6;
        for j in 0..len {
            let x = j as f64 / SR;
            let v = (2.0 * PI * hz * x).sin()
                * (-x * 4.5).exp()
                * (1.0 - (-x * 800.0).exp())
                * g
                * 0.09;
            buf.add(i0 + j, v, pan);
            send.add(i0 + j, v * 1.2, pan);
        }
    }
}

fn chime(buf: &mut Stereo, send: &mut Stereo, t: f64, g: f64, pitch: f64) {
    let hz = 1320.0 * pitch.clamp(0.25, 4.0);
    let len = (1.6 * SR) as usize;
    let i0 = idx(t);
    for j in 0..len {
        let x = j as f64 / SR;
        let v = ((2.0 * PI * hz * x).sin()
            + 0.4 * (2.0 * PI * hz * 2.76 * x).sin() * (-x * 6.0).exp())
            * (-x * 3.0).exp()
            * (1.0 - (-x * 1500.0).exp())
            * g
            * 0.18;
        buf.add(i0 + j, v, 0.0);
        send.add(i0 + j, v, 0.0);
    }
}

/// Glue and loudness: a soft-knee saturator, a peak ceiling of -1 dBFS, and a
/// fade at the end so the file never stops on a click.
pub fn master(mix: &mut Stereo, len: f64, fade_out: f64) {
    let n = ((len * SR) as usize + 1).min(mix.len());
    mix.l.truncate(n);
    mix.r.truncate(n);
    // DC and rumble below 25Hz go; they are headroom nobody hears.
    for ch in [&mut mix.l, &mut mix.r] {
        let mut prev_x = 0.0f32;
        let mut prev_y = 0.0f32;
        let a = (1.0 - 2.0 * PI * 25.0 / SR) as f32;
        for s in ch.iter_mut() {
            let y = *s - prev_x + a * prev_y;
            prev_x = *s;
            prev_y = y;
            *s = y;
        }
    }
    let drive = 1.3f32;
    let norm = drive.tanh();
    for i in 0..n {
        mix.l[i] = (mix.l[i] * drive).tanh() / norm;
        mix.r[i] = (mix.r[i] * drive).tanh() / norm;
    }
    let peak = mix
        .l
        .iter()
        .chain(mix.r.iter())
        .fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 1e-6 {
        let g = 0.89 / peak;
        for i in 0..n {
            mix.l[i] *= g;
            mix.r[i] *= g;
        }
    }
    let fade_in = (0.004 * SR) as usize;
    for i in 0..fade_in.min(n) {
        let g = i as f32 / fade_in as f32;
        mix.l[i] *= g;
        mix.r[i] *= g;
    }
    let fo = ((fade_out.max(0.0) * SR) as usize).min(n);
    for k in 0..fo {
        let i = n - 1 - k;
        let g = (k as f32 / fo as f32).powf(1.5);
        mix.l[i] *= g;
        mix.r[i] *= g;
    }
}

/// 16-bit PCM WAV.
pub fn write_wav(path: &Path, mix: &Stereo) -> std::io::Result<()> {
    let n = mix.l.len().min(mix.r.len());
    let data_len = (n * 4) as u32;
    let mut b = Vec::with_capacity(44 + n * 4);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        for s in [mix.l[i], mix.r[i]] {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            bpm: 120.0,
            beats_per_bar: 4.0,
            offset: 0.0,
        }
    }

    #[test]
    fn keys_and_numerals_read_as_musicians_write_them() {
        let k = parse_key("F#m").unwrap();
        assert_eq!((k.root, k.minor), (6, true));
        let k = parse_key("Eb").unwrap();
        assert_eq!((k.root, k.minor), (3, false));
        assert!(parse_key("H").is_err());
        assert_eq!(parse_numeral("VI").unwrap(), (5, false));
        assert_eq!(parse_numeral("iv7").unwrap(), (3, true));
        assert!(parse_numeral("VIII").is_err());
        // A minor, chord i is A C E.
        let am = Key {
            root: 9,
            minor: true,
        };
        assert_eq!(am.chord(0, false), vec![0, 3, 7]);
        assert_eq!(am.chord(5, false), vec![8, 12, 15]);
    }

    #[test]
    fn the_score_is_as_long_as_its_sections_and_never_clips() {
        let op = json!({"bpm": 120, "sections": [{"bars": 1, "part": "build"}, {"bars": 1, "part": "drop"}]});
        let m = Music::from_op(&op, grid()).unwrap();
        assert_eq!(m.length(), 4.0);
        let mut s = m.render(4.0);
        render_cues(
            &mut s,
            &[Cue {
                kind: "whoosh".into(),
                at: 1.0,
                gain: 1.0,
                pan: 0.0,
                pitch: 1.0,
                dur: Some(0.5),
            }],
            grid(),
        );
        master(&mut s, 4.0, 0.5);
        let peak =
            s.l.iter()
                .chain(s.r.iter())
                .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.5 && peak <= 0.9, "peak {peak}");
        assert!(s.l.iter().all(|v| v.is_finite()));
        assert_eq!(s.l.len(), (4.0 * SR) as usize + 1);
    }

    #[test]
    fn the_same_score_renders_the_same_samples() {
        let op = json!({"bpm": 128, "sections": [{"bars": 1, "part": "drop"}], "seed": 3});
        let a = Music::from_op(&op, grid()).unwrap().render(1.0);
        let b = Music::from_op(&op, grid()).unwrap().render(1.0);
        assert!(a.l == b.l && a.r == b.r);
    }

    #[test]
    fn a_bad_section_is_named() {
        let op = json!({"sections": [{"bars": 2, "part": "chorus"}]});
        let e = Music::from_op(&op, grid()).err().unwrap();
        assert!(e.contains("chorus") && e.contains("drop"), "{e}");
    }
}
