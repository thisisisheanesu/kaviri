//! lensa — a programmable browser that records itself and produces
//! Screen Studio-style auto-zoomed videos. "Screen Studio for AI agents."
//!
//!   lensa record --script demo.jsonl --out demo.mp4
//!   lensa serve [--port 9222]

mod backdrop;
mod cdp;
mod ops;
mod zoom;

use ops::{CursorCfg, Session, DEFAULT_CURSOR_SCALE};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

const USAGE: &str = "\
lensa — programmable recording browser (Screen Studio for AI agents)

USAGE:
  lensa record --script <file.jsonl> --out <file.mp4> [options]
  lensa serve [--port <n>] [--out <file.mp4>] [options]
  lensa presets | lensa backgrounds

OPTIONS:
  --script <path>     newline-delimited JSON ops to run (record mode)
  --out <path>        output MP4 (default lensa-out.mp4)
  --port <n>          serve ops over TCP instead of stdin/stdout
  --width <px>        logical viewport width  (default 1470)
  --height <px>       logical viewport height (default 830)
  --scale <f>         device scale factor / capture supersampling (default 2)
  --preset <name>     a named viewport/capture/output shape; --presets lists them
  --out-width <px>    video width  (default: the viewport width)
  --out-height <px>   video height (default: the viewport height)
  --background <name> backdrop the take is composited onto: a name, auto or
                      none (default auto); --backgrounds lists the names
  --cursor <name>     pointer shape: auto, arrow, hand, text or none
                      (default auto: whatever the OS would show)
  --cursor-scale <f>  pointer size against a 1x system cursor (default 1.75)
  --chromium <path>   browser binary (default: autodetect / $LENSA_CHROMIUM)
  --keep-temp         keep the intermediate CFR video (.lensa-tmp/)
  --audio             reserved; audio capture is not yet implemented

OPS (one JSON object per line):
  {\"op\":\"start_recording\"[,\"path\":\"out.mp4\"]}
  {\"op\":\"navigate\",\"url\":\"https://…\"}       (bare paths become file://)
  {\"op\":\"click\",\"selector\":\"css\"}  or  {\"op\":\"click\",\"x\":.., \"y\":..}
  {\"op\":\"type\",\"selector\":\"css\",\"text\":\"…\",\"typewriter_ms\":45}
  {\"op\":\"scroll\",\"y\":600,\"smooth\":true}
  {\"op\":\"wait\",\"ms\":800}  or  {\"op\":\"wait\",\"selector\":\"css\"}
  {\"op\":\"mark\",\"label\":\"checkout\"}
  {\"op\":\"stop_recording\"}
";

/// A named shape for a take: how the page lays out, how much is captured, and how big
/// the file is.
///
/// The three are separate on purpose. A vertical take wants a 432px viewport so the site
/// lays out the way it would on a phone, 2.5x capture so a zoom still has real pixels to
/// crop into, and a 1080x1920 file so it is not a postage stamp on TikTok. Collapsing
/// them into one number, which is what `--width` alone did, makes any two of those three
/// impossible at once.
struct Preset {
    name: &'static str,
    css: (u32, u32),
    scale: f64,
    out: (u32, u32),
    about: &'static str,
}

const PRESETS: &[Preset] = &[
    Preset {
        name: "desktop",
        css: (1470, 830),
        scale: 2.0,
        out: (1470, 830),
        about: "the default: a laptop window, captured at 2x",
    },
    Preset {
        name: "tiktok",
        css: (432, 768),
        scale: 2.5,
        out: (1080, 1920),
        about: "9:16 vertical, phone layout, 1080x1920. Also for Reels and Shorts",
    },
    Preset {
        name: "reels",
        css: (432, 768),
        scale: 2.5,
        out: (1080, 1920),
        about: "same as tiktok",
    },
    Preset {
        name: "shorts",
        css: (432, 768),
        scale: 2.5,
        out: (1080, 1920),
        about: "same as tiktok",
    },
    Preset {
        name: "square",
        css: (540, 540),
        scale: 2.0,
        out: (1080, 1080),
        about: "1:1 for a feed post",
    },
    Preset {
        name: "landscape",
        css: (960, 540),
        scale: 2.0,
        out: (1920, 1080),
        about: "16:9 1080p. Large type for a screen at the back of a room",
    },
    Preset {
        name: "readme",
        css: (1100, 620),
        scale: 2.0,
        out: (1100, 620),
        about: "wide and light, sized to sit in a README without scaling",
    },
    Preset {
        name: "phone",
        css: (390, 844),
        scale: 3.0,
        out: (1170, 2532),
        about: "a real phone's viewport and pixel count, for a device mock",
    },
];

fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

fn preset_help() -> String {
    PRESETS
        .iter()
        .map(|p| {
            format!(
                "  {:<10} {}x{} viewport, {}x{} video  {}",
                p.name, p.css.0, p.css.1, p.out.0, p.out.1, p.about
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct Args {
    mode: String,
    script: Option<String>,
    out: String,
    port: Option<u16>,
    width: u32,
    height: u32,
    scale: f64,
    out_w: Option<u32>,
    out_h: Option<u32>,
    background: backdrop::Choice,
    cursor: CursorCfg,
    chromium: Option<String>,
    keep_temp: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut argv = std::env::args().skip(1);
    let mode = argv.next().ok_or(USAGE.to_string())?;
    if mode == "--help" || mode == "-h" || mode == "help" {
        return Err(USAGE.to_string());
    }
    if mode == "--presets" || mode == "presets" {
        return Err(format!("Presets:\n{}\n\nUse one with: lensa record --preset <name> ...", preset_help()));
    }
    if mode == "--backgrounds" || mode == "backgrounds" {
        return Err(format!(
            "Backgrounds:\n{}\n\nUse one with: lensa record --background <name> ...\n\nCursors (--cursor):\n{}",
            backdrop::help(),
            ops::cursor_help()
        ));
    }
    /*
     * Held as text until the whole line has been read, so --cursor-scale works
     * whichever side of --cursor it lands on and a bad name is reported once,
     * with the list, rather than at the point it was parsed.
     */
    let mut background = "auto".to_string();
    let mut cursor = "auto".to_string();
    let mut cursor_scale = DEFAULT_CURSOR_SCALE;
    let mut a = Args {
        mode,
        script: None,
        out: "lensa-out.mp4".into(),
        port: None,
        width: 1470,
        height: 830,
        scale: 2.0,
        out_w: None,
        out_h: None,
        background: backdrop::Choice::Auto,
        cursor: CursorCfg::default(),
        chromium: None,
        keep_temp: false,
    };
    while let Some(flag) = argv.next() {
        let mut val = |name: &str| -> Result<String, String> {
            argv.next().ok_or(format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--script" => a.script = Some(val("--script")?),
            "--out" => a.out = val("--out")?,
            "--port" => a.port = Some(val("--port")?.parse().map_err(|_| "bad --port")?),
            "--width" => a.width = val("--width")?.parse().map_err(|_| "bad --width")?,
            "--height" => a.height = val("--height")?.parse().map_err(|_| "bad --height")?,
            "--scale" => a.scale = val("--scale")?.parse().map_err(|_| "bad --scale")?,
            "--out-width" => a.out_w = Some(val("--out-width")?.parse().map_err(|_| "bad --out-width")?),
            "--out-height" => a.out_h = Some(val("--out-height")?.parse().map_err(|_| "bad --out-height")?),
            /*
             * Applied where it is read, so an explicit --width after --preset still wins
             * and the preset stays a starting point rather than a cage.
             */
            "--preset" => {
                let name = val("--preset")?;
                let p = preset(&name).ok_or_else(|| {
                    format!("unknown preset: {name}\n\nPresets:\n{}", preset_help())
                })?;
                a.width = p.css.0;
                a.height = p.css.1;
                a.scale = p.scale;
                a.out_w = Some(p.out.0);
                a.out_h = Some(p.out.1);
            }
            "--presets" => {
                return Err(format!("Presets:\n{}", preset_help()));
            }
            "--background" => background = val("--background")?,
            "--backgrounds" => {
                return Err(format!("Backgrounds:\n{}", backdrop::help()));
            }
            "--cursor" => cursor = val("--cursor")?,
            "--cursor-scale" => {
                cursor_scale = val("--cursor-scale")?
                    .parse()
                    .map_err(|_| "bad --cursor-scale")?
            }
            "--chromium" => a.chromium = Some(val("--chromium")?),
            "--keep-temp" => a.keep_temp = true,
            "--audio" => eprintln!("lensa: --audio is not implemented yet (headless backend); ignoring"),
            other => return Err(format!("unknown flag: {other}\n\n{USAGE}")),
        }
    }
    a.background = backdrop::parse_choice(&background)?;
    a.cursor = CursorCfg::parse(&cursor, cursor_scale)?;
    Ok(a)
}

fn run_record(a: &Args) -> Result<(), String> {
    let script_path = a.script.as_ref().ok_or("record mode needs --script")?;
    let body = std::fs::read_to_string(script_path)
        .map_err(|e| format!("cannot read {script_path}: {e}"))?;
    let mut ops_list: Vec<Value> = Vec::new();
    for (i, line) in body.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("{script_path}:{}: {e}", i + 1))?;
        ops_list.push(v);
    }
    let has_start = ops_list.iter().any(|o| o["op"] == "start_recording");
    let has_stop = ops_list.iter().any(|o| o["op"] == "stop_recording");

    eprintln!("lensa: launching browser ({}x{}@{}x) ...", a.width, a.height, a.scale);
    let mut s = Session::launch(
        a.chromium.as_deref(), a.width, a.height, a.scale,
        (a.out_w.unwrap_or(a.width), a.out_h.unwrap_or(a.height)),
        a.keep_temp,
        a.cursor,
    )?;
    s.out_path = Some(a.out.clone());
    s.background = a.background;

    if !has_start {
        report(&s.exec(&json!({"op": "start_recording"}))?);
    }
    for op in &ops_list {
        // --out wins over any path inside the script.
        let mut op = op.clone();
        if op["op"] == "start_recording" {
            op["path"] = json!(a.out.clone());
        }
        report(&s.exec(&op)?);
    }
    if !has_stop {
        report(&s.exec(&json!({"op": "stop_recording"}))?);
    }
    if let Some((dur, n)) = s.rendered {
        eprintln!("lensa: done — {} ({dur:.1}s, {n} zoom events)", a.out);
    }
    Ok(())
}

fn report(v: &Value) {
    println!("{v}");
    let _ = std::io::stdout().flush();
}

fn serve_stream<R: BufRead, W: Write>(s: &Mutex<Session>, reader: R, mut writer: W) {
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(line) {
            /*
             * One browser, so ops are serialized here rather than raced. The lock is
             * held for a single op: a client that goes quiet mid-script blocks nobody,
             * and a slow op (a navigate, a render) makes the others wait their turn
             * instead of interleaving into the same page.
             */
            Ok(op) => {
                let mut guard = match s.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                match guard.exec(&op) {
                    Ok(v) => json!({"ok": true, "result": v}),
                    Err(e) => json!({"ok": false, "error": e}),
                }
            }
            Err(e) => json!({"ok": false, "error": format!("bad json: {e}")}),
        };
        if writeln!(writer, "{resp}").is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

fn run_serve(a: &Args) -> Result<(), String> {
    eprintln!("lensa: launching browser ({}x{}@{}x) ...", a.width, a.height, a.scale);
    let mut session = Session::launch(
        a.chromium.as_deref(), a.width, a.height, a.scale,
        (a.out_w.unwrap_or(a.width), a.out_h.unwrap_or(a.height)),
        a.keep_temp,
        a.cursor,
    )?;
    session.out_path = Some(a.out.clone());
    session.background = a.background;
    let s = Arc::new(Mutex::new(session));
    match a.port {
        Some(port) => {
            let listener = std::net::TcpListener::bind(("127.0.0.1", port))
                .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
            eprintln!("lensa: listening on 127.0.0.1:{port} (NDJSON ops, concurrent connections)");
            /*
             * A connection per thread. They share one browser through the mutex, so a
             * second client attaching does not have to wait for the first to hang up:
             * an editor watching telemetry and a script driving the page can hold
             * sockets open at the same time.
             */
            for conn in listener.incoming() {
                let conn = match conn {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let peer = conn
                    .peer_addr()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|_| "?".into());
                let reader = match conn.try_clone() {
                    Ok(c) => BufReader::new(c),
                    Err(e) => {
                        eprintln!("lensa: cannot clone socket for {peer}: {e}");
                        continue;
                    }
                };
                let shared = Arc::clone(&s);
                std::thread::spawn(move || {
                    eprintln!("lensa: client {peer} connected");
                    serve_stream(&shared, reader, conn);
                    eprintln!("lensa: client {peer} disconnected");
                });
            }
            Ok(())
        }
        None => {
            eprintln!("lensa: reading NDJSON ops from stdin");
            let stdin = std::io::stdin();
            serve_stream(&s, stdin.lock(), std::io::stdout());
            // EOF: finish any open recording.
            let mut guard = s.lock().unwrap_or_else(|p| p.into_inner());
            if guard.cdp.is_recording() {
                report(&guard.exec(&json!({"op": "stop_recording"}))?);
            }
            Ok(())
        }
    }
}

fn main() {
    let a = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let r = match a.mode.as_str() {
        "record" => run_record(&a),
        "serve" => run_serve(&a),
        m => Err(format!("unknown mode: {m}\n\n{USAGE}")),
    };
    if let Err(e) = r {
        eprintln!("lensa: error: {e}");
        std::process::exit(1);
    }
}
