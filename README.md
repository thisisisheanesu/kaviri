# lensa

A programmable browser that records what happens inside it and produces
Screen Studio-style videos — automatic smooth zoom and pan onto every
interaction. **Screen Studio for AI agents.**

One binary. An agent feeds it newline-delimited JSON ops (navigate, click,
type, scroll…); lensa drives a browser, captures the page content, tracks
every interaction's timestamp and bounding box, and renders a polished MP4
with cinematic ease-in/hold/ease-out zooms that follow consecutive nearby
interactions. No OS screen recorder, no Wayland portals, no coordinate
calibration — the capture is the page itself.

## Quick start

```sh
cargo build --release
./target/release/lensa record --script examples/form.jsonl --out demo.mp4
```

Requirements (all user-space, no sudo):
- a Chromium/Chrome binary (`chromium` on PATH, or `--chromium`/`$LENSA_CHROMIUM`)
- `ffmpeg` on PATH (a static build in `~/.local/bin` works, or `$LENSA_FFMPEG`)

## Modes

```
lensa record --script demo.jsonl --out demo.mp4     # scripted take, end to end
lensa serve                                          # NDJSON ops on stdin, results on stdout
lensa serve --port 9222                              # same protocol over TCP
```

## Presets

A take has three sizes, and they are not the same number: the **viewport** the page lays
out in, the **capture** resolution (viewport x scale), and the **video** it renders to. A
vertical take wants a phone-width viewport so the site lays out like a phone, a high
capture so zooms crop into real pixels, and a 1080x1920 file. `--preset` sets all three.

```
lensa presets                                   # list them
lensa record --preset tiktok --script s.jsonl --out s.mp4
```

| preset | viewport | video | for |
|---|---|---|---|
| `desktop` | 1470x830 | 1470x830 | the default: a laptop window |
| `tiktok` / `reels` / `shorts` | 432x768 | 1080x1920 | 9:16 vertical, phone layout |
| `square` | 540x540 | 1080x1080 | 1:1 feed posts |
| `landscape` | 960x540 | 1920x1080 | 16:9 1080p, large type for a projector |
| `readme` | 1100x620 | 1100x620 | sits in a README without scaling |
| `phone` | 390x844 | 1170x2532 | a real phone's viewport and pixels, for device mocks |

A preset is a starting point: `--width`, `--height`, `--scale`, `--out-width` and
`--out-height` after it still win. A vertical frame also wants its own shot list:
scroll rather than pan, open on the most legible image, and keep every beat short.
`examples/ngano-vertical.jsonl` is one.

## Op protocol (one JSON object per line)

```jsonc
{"op":"start_recording","path":"out.mp4"}   // path optional; --out wins
{"op":"navigate","url":"https://example.com"}  // bare paths become file://
{"op":"click","selector":"#buy"}            // or {"op":"click","x":400,"y":300}
{"op":"type","selector":"#name","text":"Ada","typewriter_ms":60}
{"op":"scroll","y":600,"smooth":true}
{"op":"wait","ms":800}                      // or {"op":"wait","selector":".loaded"}
{"op":"mark","label":"checkout"}
{"op":"stop_recording"}                     // renders the MP4
```

Every executed op auto-emits a timestamped mark (on the recording clock) with
the target element's bounding box; that telemetry drives the zoom generator.
A `<out>.telemetry.json` sidecar with marks and computed zoom events is
written next to the video.

## How the zoom works

- Frames are captured VFR via the DevTools screencast and normalized to CFR
  30fps **before** any time-based math.
- Each interaction becomes a zoom event `{t, end, cx, cy, z}` (z 1.5–1.85 by
  target size). Consecutive interactions within ~1.3s of the hold window are
  merged into one event with **path waypoints**, so the camera pans between
  them instead of zooming out and back in.
- All eases are cubic smoothstep `s = p·p·(3−2p)`, EASE = 0.7s; the crop is
  clamped to the frame.
- Rendering is a generated ffmpeg `zoompan` expression over the CFR
  intermediate (v2: render-time compositing in Rust for arbitrarily long
  takes and per-frame effects).

A fake cursor with smooth motion and click ripples is injected into every
page, so the video shows pointer intent even though headless capture has no
OS cursor. Content-only video is a feature: no window chrome, ever.

## Architecture

- `src/cdp.rs` — minimal synchronous CDP client (one websocket, short read
  timeout, single-threaded pump: every wait drains and acks screencast frames).
- `src/ops.rs` — the op protocol, cursor overlay injection, telemetry marks.
- `src/zoom.rs` — marks → zoom events → ffmpeg expressions → two-pass render.
- `src/main.rs` — CLI (`record` / `serve`).

Backend today is headless Chromium via CDP (zero native build deps, works on
any Linux including Wayland-only boxes). An embedded-webview backend
(wry/WebKitGTK) is planned where dev headers exist.

## Known gaps

- Audio capture (`--audio`) is not implemented yet; the plan is a dedicated
  PipeWire/Pulse sink for the browser process, muxed against the same clock.
- `serve` takes concurrent connections, one thread each, serialized onto the
  single browser by a mutex held for one op at a time.
- Frames spool to one append-only temp file as they arrive, and the render pass
  holds one frame at a time, so memory no longer grows with take length.
- Supersampling works when `--scale` is above 1: capture switches from the
  DevTools screencast, which caps frames at the CSS viewport no matter what
  `maxWidth` asks for, to a `Page.captureScreenshot` pump with `clip.scale`,
  which does not. The pump costs a round trip per frame, so at 1x the cheaper
  push-based screencast is still used.
