//! lensa — a programmable browser that records itself and produces
//! Screen Studio-style auto-zoomed videos. "Screen Studio for AI agents."
//!
//!   lensa record --script demo.jsonl --out demo.mp4
//!   lensa serve [--port 9222]

mod cdp;
mod ops;
mod zoom;

use ops::Session;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};

const USAGE: &str = "\
lensa — programmable recording browser (Screen Studio for AI agents)

USAGE:
  lensa record --script <file.jsonl> --out <file.mp4> [options]
  lensa serve [--port <n>] [--out <file.mp4>] [options]

OPTIONS:
  --script <path>     newline-delimited JSON ops to run (record mode)
  --out <path>        output MP4 (default lensa-out.mp4)
  --port <n>          serve ops over TCP instead of stdin/stdout
  --width <px>        logical viewport width  (default 1470)
  --height <px>       logical viewport height (default 830)
  --scale <f>         device scale factor / capture supersampling (default 2)
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

struct Args {
    mode: String,
    script: Option<String>,
    out: String,
    port: Option<u16>,
    width: u32,
    height: u32,
    scale: f64,
    chromium: Option<String>,
    keep_temp: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut argv = std::env::args().skip(1);
    let mode = argv.next().ok_or(USAGE.to_string())?;
    if mode == "--help" || mode == "-h" || mode == "help" {
        return Err(USAGE.to_string());
    }
    let mut a = Args {
        mode,
        script: None,
        out: "lensa-out.mp4".into(),
        port: None,
        width: 1470,
        height: 830,
        scale: 2.0,
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
            "--chromium" => a.chromium = Some(val("--chromium")?),
            "--keep-temp" => a.keep_temp = true,
            "--audio" => eprintln!("lensa: --audio is not implemented yet (headless backend); ignoring"),
            other => return Err(format!("unknown flag: {other}\n\n{USAGE}")),
        }
    }
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
    let mut s = Session::launch(a.chromium.as_deref(), a.width, a.height, a.scale, a.keep_temp)?;
    s.out_path = Some(a.out.clone());

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

fn serve_stream<R: BufRead, W: Write>(s: &mut Session, reader: R, mut writer: W) {
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
            Ok(op) => match s.exec(&op) {
                Ok(v) => json!({"ok": true, "result": v}),
                Err(e) => json!({"ok": false, "error": e}),
            },
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
    let mut s = Session::launch(a.chromium.as_deref(), a.width, a.height, a.scale, a.keep_temp)?;
    s.out_path = Some(a.out.clone());
    match a.port {
        Some(port) => {
            let listener = std::net::TcpListener::bind(("127.0.0.1", port))
                .map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))?;
            eprintln!("lensa: listening on 127.0.0.1:{port} (NDJSON ops, one connection at a time)");
            for conn in listener.incoming() {
                let conn = match conn {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let reader = BufReader::new(conn.try_clone().map_err(|e| e.to_string())?);
                serve_stream(&mut s, reader, conn);
                eprintln!("lensa: connection closed; waiting for next");
            }
            Ok(())
        }
        None => {
            eprintln!("lensa: reading NDJSON ops from stdin");
            let stdin = std::io::stdin();
            serve_stream(&mut s, stdin.lock(), std::io::stdout());
            // EOF: finish any open recording.
            if s.cdp.is_recording() {
                report(&s.exec(&json!({"op": "stop_recording"}))?);
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
