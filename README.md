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

## Backdrop

Every take is composited onto a wallpaper: the content inset with rounded
corners and a soft drop shadow, the way Screen Studio and Cleanshot frame a
recording. The background fills the output frame, so the preset's aspect ratio
is what it was; the content is fitted inside it at its own aspect ratio and
centred, never stretched.

```
lensa backgrounds                                    # list them
lensa record --background tide  --script s.jsonl --out s.mp4
lensa record --background none  --script s.jsonl --out s.mp4   # full-frame, as before
```

| background | what it is |
|---|---|
| `dusk` | indigo to violet to magenta wash |
| `dawn` | peach to rose to lilac, light and warm |
| `tide` | deep teal to blue to cyan |
| `moss` | forest to olive to sand |
| `ember` | oxblood to orange to amber |
| `slate` | solid muted blue-grey |
| `linen` | solid warm off-white |
| `mesh-cool` | dark mesh gradient, blue and violet blobs |
| `mesh-warm` | light mesh gradient, rose and amber blobs |
| `auto` | **the default**: picked from the recording itself |
| `none` | no backdrop |

**How `auto` picks.** After the frames are normalised, the intermediate is
probed with ffmpeg at two frames a second scaled to 12x12, and every sample is
folded into two numbers: a chroma-weighted mean hue (so a white page with one
brand colour resolves to the brand colour, not to white) and a mean lightness.
Then each background is scored on its own measured hue and lightness:

- **hue**: distance from `content_hue + 150°`, a split-complementary target
  rather than the flat opposite, which reads as deliberate instead of as a
  clash;
- **lightness**: a penalty for landing within 0.35 of the content's lightness,
  so a white app gets a deep wash and a dark app gets a light one;
- **ties**: solids carry a small constant penalty so a wash wins an otherwise
  equal contest, and declaration order breaks anything still level.

Nothing is random. The same recording always picks the same background, and if
the probe fails the fallback is `dusk`. The chosen name, and the content box it
was fitted to, land in the telemetry sidecar.

The plate itself is one RGBA PNG rendered in `src/backdrop.rs` (no image crate:
the encoder writes stored-deflate PNGs) and handed to ffmpeg as a second input.
It is a frame rather than a backdrop: opaque everywhere except the rounded
window the content shows through, so a single `overlay` gives background,
shadow and rounded corners in one pass. Gradients are eased with the same cubic
smoothstep the zoom uses and dithered by about one 8-bit level, because a smooth
wash bands badly once h264 has had its way with it.

## Cursor

Headless capture has no OS cursor, so lensa draws one into the page: a large
macOS-style pointer with a white outline and a soft shadow, plus a click ripple.

```
lensa record --cursor-scale 2.0 --script s.jsonl --out s.mp4
lensa record --cursor hand --script s.jsonl --out s.mp4   # pin one shape
lensa record --cursor none --script s.jsonl --out s.mp4   # draw no pointer
```

- **Vector, not bitmap.** The pointer is an SVG path drawn at the requested
  size, so a zoom crops into a crisp edge rather than an upscaled one.
- **Size.** `--cursor-scale` is a multiplier over a 1x system pointer (a 24 CSS
  px arrow). The default is 1.75, large enough to read once a 1470px take is
  playing in a phone-sized player. It is measured in the viewport's own pixels,
  so it scales with the content: on the narrow vertical presets the pointer is a
  much bigger share of the frame, and `--cursor-scale 1.0` suits them better.
- **Shapes.** `auto` (the default) asks the element being clicked what the OS
  would show and picks `arrow`, `hand` (links, buttons, `cursor: pointer`) or
  `text` (inputs, textareas, contenteditable), falling back to `arrow` for
  anything it cannot classify. Pass a shape name to pin it for the whole take.
- **Zoom.** The pointer is part of the page, so it is captured in the frame and
  the zoom transform carries it along: its position cannot drift away from the
  click it belongs to, and it scales with the content the way a magnified screen
  recording does.

## Op protocol (one JSON object per line)

```jsonc
{"op":"start_recording","path":"out.mp4"}   // path optional; --out wins
{"op":"navigate","url":"https://example.com"}  // bare paths become file://
{"op":"click","selector":"#buy"}            // or {"op":"click","x":400,"y":300}
{"op":"type","selector":"#name","text":"Ada"}  // typewriter_ms optional, default 18
{"op":"scroll","y":600,"smooth":true}
{"op":"wait","ms":800}                      // or {"op":"wait","selector":".loaded"}
{"op":"wait","selector":".done","timeout_ms":180000}  // default 20s; raise it for slow work
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
- The zoomed content is composited onto the backdrop plate in that same pass:
  scaled into the content box, padded out to the frame, and the plate laid over
  it.

Content-only video is a feature: no window chrome, ever. What sits around the
content is the backdrop, not a fake title bar, and the pointer is drawn into
the page (see [Cursor](#cursor)) because headless capture has no OS cursor.

## Architecture

- `src/cdp.rs` — minimal synchronous CDP client (one websocket, short read
  timeout, single-threaded pump: every wait drains and acks screencast frames).
- `src/ops.rs` — the op protocol, cursor overlay injection, telemetry marks.
- `src/zoom.rs` — marks → zoom events → ffmpeg expressions → two-pass render.
- `src/backdrop.rs` — the built-in backgrounds, the auto picker, and the RGBA
  plate (gradient, shadow, rounded window) written out as a dependency-free PNG.
- `src/main.rs` — CLI (`record` / `serve` / `presets` / `backgrounds`).

Backend today is headless Chromium via CDP (zero native build deps, works on
any Linux including Wayland-only boxes). An embedded-webview backend
(wry/WebKitGTK) is planned where dev headers exist.

## GitHub Actions

A demo video goes stale the moment the UI moves, and nobody re-records it,
because that means blocking out an afternoon. Put the script in the repo next
to the code it films and the demo changes when the product does.

```yaml
- uses: actions/checkout@v4

- name: Serve the app
  run: |
    npm run build && npx serve -l 8099 dist &
    until curl -sf http://127.0.0.1:8099/ >/dev/null; do sleep 0.25; done

- uses: vamboai/lensa@v1
  with:
    script: demos/checkout.jsonl
    out: checkout.mp4
    preset: readme          # or tiktok, square, landscape, desktop
```

The MP4 is uploaded as a workflow artifact. `upload: false` if you would
rather push it somewhere yourself.

The action installs ffmpeg and Chromium, builds lensa and runs the take. Pin
`ref:` to a tag if you want the framing to stay byte-identical between runs.

Two things it sets that a local run does not, and should not:

- `LENSA_CHROMIUM_ARGS=--no-sandbox --disable-dev-shm-usage`, because
  Chromium's sandbox cannot start inside a CI container. That is a genuine
  reduction in isolation, so lensa never does it on its own; the runner opts
  in. The same variable works locally if you need it.
- `TMPDIR` pointed at the runner temp, because `/tmp` on a hosted runner is
  small and shared, and frames spool there.

`.github/workflows/demo.yml` in this repo is the real thing rather than an
illustration: lensa records `examples/` on every push that touches `src/`, in
two shapes, and fails the run if the result is shorter than two seconds. A
take that produces a file but no frames otherwise passes quietly.

## Known gaps

- Audio capture (`--audio`) is not implemented yet; the plan is a dedicated
  PipeWire/Pulse sink for the browser process, muxed against the same clock.
- The backdrop plate is a stored-deflate PNG, so it costs about 4 bytes per
  output pixel on disk (roughly 8 MB for a 1080x1920 take) in `.lensa-tmp/`
  while the render runs, and goes with the rest of the temp unless
  `--keep-temp`. A real deflate would shrink it; no dependency was worth it.
  If the plate cannot be written at all, the take still renders full-frame.
- `serve` takes concurrent connections, one thread each, serialized onto the
  single browser by a mutex held for one op at a time.
- Frames spool to one append-only temp file as they arrive, and the render pass
  holds one frame at a time, so memory no longer grows with take length.
- Supersampling works when `--scale` is above 1: capture switches from the
  DevTools screencast, which caps frames at the CSS viewport no matter what
  `maxWidth` asks for, to a `Page.captureScreenshot` pump with `clip.scale`,
  which does not. The pump costs a round trip per frame, so at 1x the cheaper
  push-based screencast is still used.
