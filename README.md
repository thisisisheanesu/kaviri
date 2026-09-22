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
- Rust 1.75 or newer to build (`rust-version` in `Cargo.toml` is the source of truth)
- a Chromium/Chrome binary (`chromium` on PATH, or `--chromium`/`$LENSA_CHROMIUM`).
  Anything current enough to speak CDP `Page.captureScreenshot` and
  `Emulation.setDeviceMetricsOverride` works; that is Chromium 90 and up in practice.
  Both are discovered at run time, not bundled, so their licences are theirs
- `ffmpeg` on PATH (a static build in `~/.local/bin` works, or `$LENSA_FFMPEG`),
  built with `libx264`. Anything from ffmpeg 4 onwards is fine
- Linux or macOS. Windows is untested and unsupported today

lensa checks for both binaries before it launches the browser, so a missing
encoder costs you a second rather than a whole take. `lensa doctor` prints what
it found and where.

## Modes

```
lensa record --script demo.jsonl --out demo.mp4     # scripted take, end to end
lensa serve                                          # NDJSON ops on stdin, results on stdout
lensa serve --port 7800                              # same protocol over TCP, token required
lensa doctor                                         # what chromium and ffmpeg resolve to
lensa --version                                      # the build that produced your video
```

`serve` on stdin is the default and the one to prefer. `--port` opens a local
socket that anything on the machine can connect to, including a web page the
browser you are filming happens to be visiting, so it is opt-in and
token-gated; see [Serve over TCP](#serve-over-tcp).

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
{"op":"start_recording","path":"out.mp4"}   // path optional; --out wins in record mode
{"op":"navigate","url":"https://example.com"}  // bare paths become file://
{"op":"navigate","url":"https://slow.example","timeout_ms":60000}  // load budget, default 25s
{"op":"click","selector":"#buy"}            // or {"op":"click","x":400,"y":300}
{"op":"type","selector":"#name","text":"Ada"}  // typewriter_ms optional, default 18
{"op":"type","text":"Ada"}                  // no selector: types into the focused element
{"op":"scroll","y":600,"smooth":true}       // y is an absolute document offset
{"op":"wait","ms":800}                      // a fixed pause
{"op":"wait","selector":".loaded"}          // or wait for an element, default budget 20s
{"op":"wait","selector":".done","timeout_ms":180000}  // raise it for slow work
{"op":"wait","selector":".done","visible":false}      // presence only, skip the visibility check
{"op":"mark","label":"checkout"}
{"op":"stop_recording"}                     // renders the MP4
```

Every executed op auto-emits a timestamped mark (on the recording clock) with
the target element's bounding box; that telemetry drives the zoom generator.

### Responses

One JSON object per op, on stdout, in both `record` and `serve`:

```jsonc
{"ok":true,"result":{"event":"mark","kind":"click","t":3.14,"box":[40,120,180,44]}}
{"ok":false,"error":"selector matched a non-visible element: #done","op":{"op":"click","selector":"#done"}}
```

A failing op is reported in band and, in record mode, ends the script: lensa
stops the recording, renders what it captured, and exits non-zero. You get a
partial video and a clear error rather than nothing.

### Things worth knowing before you write a script

- **`wait` takes `ms` or `selector`, never both.** `ms` is a fixed pause;
  `selector` polls until the element is there and rendered, bounded by
  `timeout_ms` (default 20s). Passing both is an error, because the natural
  reading of it ("wait for this, at most this long") is not what it would do.
  Bound a selector wait with `timeout_ms`.
- **A selector wait checks visibility, not just presence.** An element that is
  in the DOM with `display:none` does not satisfy it. Pass `"visible":false`
  when you genuinely mean presence only.
- **`scroll` needs `y`, and `y` is absolute.** It is a document offset in CSS
  pixels, not a delta from where you are. There is no default; a missing or
  non-numeric `y` is an error rather than a silent scroll to the top.
- **`type` without a selector types into `document.activeElement`.** Use it to
  continue typing into a field you already clicked. If nothing editable is
  focused it is an error, because the alternative is characters going nowhere
  and the op reporting success. Typing without a selector also contributes no
  bounding box, so it produces no zoom of its own.
- **`click` and `type` refuse invisible and obscured targets.** A zero-area or
  `visibility:hidden` element is an error, and so is one covered by something
  else, which names the element that is on top.
- **Durations are numbers of milliseconds.** `ms`, `timeout_ms` and
  `typewriter_ms` accept any finite non-negative number; anything else is an
  error rather than a silent fall back to the default.
- **The last interaction wants a tail.** A zoom needs about 1.2s of footage
  after the mark to ease out, so end a script with a short `wait` before
  `stop_recording` if you want the final click zoomed. lensa warns on stderr
  when a mark lands too late to get one.

### Telemetry sidecar

lensa can write a JSON sidecar with the raw marks and the computed zoom events.
It is off by default. `--keep-temp` puts it next to the video as
`<out>.telemetry.json`, and `LENSA_TELEMETRY=<path>` writes it wherever you
name; `LENSA_TELEMETRY=0` suppresses it even under `--keep-temp`.

It stays off by default because it is a debugging artefact and because its
`marks` array carries every `navigate` label verbatim: full URLs, query strings
and any token in them, plus local `file://` paths. Do not upload it as a build
artifact without reading it first.

## How the zoom works

- Frames arrive at irregular intervals (see [Capture](#capture-and-what-it-costs-the-app-you-are-filming))
  and are normalized to CFR 30fps **before** any time-based math.
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

## Capture, and what it costs the app you are filming

There are two capture paths, and which one you get follows `--scale`:

- **`--scale` above 1 (the default, and every preset): a `Page.captureScreenshot`
  pump.** lensa asks the browser for a fresh viewport screenshot roughly every
  25ms, one request outstanding at a time, and spools the JPEG. The device
  metrics override supplies the extra pixels, so the request carries no `clip`.
  An earlier version passed `clip` with a `scale`, and it was removed: a clip is
  in document coordinates, so once the page had scrolled it pointed above the
  fold and came back blank, and carrying the scale on it squared the error.
- **`--scale 1`: the DevTools screencast.** The browser pushes frames as it
  paints them, which is cheaper and needs no round trip, but it caps frames at
  the CSS viewport no matter what `maxWidth` asks for. That is why it is not the
  default: a zoom into a 1x capture crops into upscaled pixels.

Because `--scale` defaults to 2 and every preset sets 2 or more, **the pump is
the path a normal take takes.** What that costs: each screenshot is a full
compositor pass plus a JPEG encode inside the same
browser that is running the app you are filming, and lensa then decodes and
writes it. On a 1470x830 viewport at 2x that is roughly 15-25 MB/s of JPEG and
a busy core. The filmed app runs measurably slower than it does unrecorded:
animations stutter, and a `wait` on a selector that is comfortable by hand can
time out on a loaded machine.

The escape hatch is `--scale 1`, which switches to the push-based screencast
and takes lensa almost entirely out of the app's way. You lose supersampling,
so zooms are softer. If the take is for a README at its native size, or the app
is timing-sensitive, or you are recording on a shared CI runner, `--scale 1` is
the better trade. Give the recording an uncontended core where you can.

## Resource envelope

Plan for this before a long take, because the failure mode is a full disk:

- **Temp space.** Frames spool to one append-only file under `TMPDIR`
  (`--spool-dir` moves it, e.g. next to a big scratch disk). At the default
  `--scale 2` that is roughly 15-25 MB/s, so **a five-minute take is several
  gigabytes**. A five-minute 1080x1920 take at `--scale 2.5` is more.
- **A spool cap.** The spool stops at 8 GiB by default (`--max-spool-bytes`);
  on hitting it lensa stops capturing cleanly and renders what it has rather
  than dying on ENOSPC. It also refuses to start a take with less than 512 MiB
  free, naming the directory, and clamps its own cap to the free space it sees.
- **A second large file at render time.** The CFR intermediate is a full-length
  H.264 encode in a per-run temp directory, plus the backdrop plate PNG at about
  4 bytes per output pixel (roughly 8 MB for 1080x1920). `--keep-temp` retains
  both and prints where they are.
- **CPU.** One core for the browser, one for lensa's pump, one for ffmpeg
  during the render, which runs after capture ends and is not gentle.
- **Memory is flat.** Frames go to disk as they arrive and the render pass holds
  one at a time, so a long take costs disk, not RAM.

### Environment

Every one of these has a flag; the variables exist so a CI job can set them
once for a whole matrix.

| variable | what it does |
|---|---|
| `LENSA_CHROMIUM` | browser binary, same as `--chromium` |
| `LENSA_CHROMIUM_ARGS` | extra Chromium flags, whitespace-separated |
| `LENSA_FFMPEG` | ffmpeg binary or a name to resolve on PATH |
| `TMPDIR` | where the frame spool and the CFR intermediate live |
| `LENSA_SPOOL_DIR` | the spool alone, same as `--spool-dir` |
| `LENSA_MAX_SPOOL_BYTES` | the spool cap, same as `--max-spool-bytes` |
| `LENSA_KEEP_TEMP` | keep the intermediates, same as `--keep-temp` |
| `LENSA_TELEMETRY` | where to write the sidecar; `0` or `off` suppresses it |
| `LENSA_TOKEN` | the `serve --port` token, instead of a generated one |
| `LENSA_DEBUG` | log the CDP traffic on stderr |

## Serve over TCP

`lensa serve` on stdin needs no authentication: the ops come from the process
that started it. `lensa serve --port <n>` does not have that property. Binding
to loopback is not a trust boundary against a browser, because any page the
user visits can `fetch()` a loopback port, and the op set can navigate to
`file://` URLs and write an MP4 to a path of the caller's choosing.

So `--port` prints a token on stderr at startup and the first line of every
connection must be `{"op":"hello","token":"…"}`. Any line that is not JSON
drops the connection rather than being partly executed, which is what makes a
stray HTTP request a disconnect instead of a script. Pick a port outside
Chrome's debugging range; lensa does not default to one.

## Architecture

- `src/cdp.rs` — minimal synchronous CDP client (one websocket, single-threaded
  pump: every wait drains events, acks screencast frames and keeps the
  screenshot pump on its cadence, with the socket timeout driven by the wait's
  own deadline rather than the other way round) plus the frame spool.
- `src/ops.rs` — the op protocol, cursor overlay injection, telemetry marks.
- `src/zoom.rs` — marks → zoom events → ffmpeg expressions → two-pass render.
- `src/backdrop.rs` — the built-in backgrounds, the auto picker, and the RGBA
  plate (gradient, shadow, rounded window) written out as a dependency-free PNG.
- `src/main.rs` — CLI (`record` / `serve` / `doctor` / `presets` / `backgrounds`).

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

The action installs ffmpeg and Chromium, builds lensa from its own checked-out
source and runs the take. **Supported runners: `ubuntu-*` and `macos-*`
(GitHub-hosted or self-hosted equivalents).** On Linux it installs through apt,
on macOS through brew; any other `RUNNER_OS`, and most bare containers, exit
with a message rather than guessing. It installs a Rust toolchain rather than
assuming one is present.

**Pinning the action to a tag fixes lensa's behaviour, not its output.** Two
runs of the same script on the same commit do not produce the same bytes and
are not frame-identical: marks and frame times come from a wall clock, and the
capture cadence follows machine load, so durations, waypoint spacing and the
exact pixels all move a little run to run. What pinning buys you is that the
framing rules, the zoom ladder and the backgrounds stay put. If you need a
byte-stable asset, render once and commit the MP4.

Two things it sets that a local run does not, and should not:

- `LENSA_CHROMIUM_ARGS=--no-sandbox --disable-dev-shm-usage`, because
  Chromium's sandbox cannot start inside a CI container. That is a genuine
  reduction in isolation, so lensa never does it on its own; the runner opts
  in. The same variable works locally if you need it.
- `TMPDIR` pointed at the runner temp, because `/tmp` on a hosted runner is
  small and shared, and both the frame spool and the CFR intermediate live
  there. `--spool-dir` does the same job for the spool alone.

`.github/workflows/demo.yml` in this repo is the real thing rather than an
illustration: lensa records `examples/` on every push that touches `src/`, in
two shapes, and fails the run if the result is shorter than two seconds or if
its frames never change, which is what an error page or a frozen take looks
like.

## Known gaps

- **Video only. Audio capture (`--audio`) is not implemented**; the flag warns
  and is otherwise ignored. The plan is a dedicated PipeWire/Pulse sink for the
  browser process, muxed against the same clock. If you need narration, add it
  in an editor afterwards.
- **Takes are not reproducible frame for frame.** The whole timeline is derived
  from a wall clock, so the same script on the same commit gives you the same
  film, not the same file. See [GitHub Actions](#github-actions).
- **The default capture path competes with the app it films** for CPU. See
  [Capture](#capture-and-what-it-costs-the-app-you-are-filming); `--scale 1` is
  the way out.
- The backdrop plate is a stored-deflate PNG, so it costs about 4 bytes per
  output pixel on disk (roughly 8 MB for a 1080x1920 take) in the run's temp
  directory while the render runs, and goes with the rest of the temp unless
  `--keep-temp`. A real deflate would shrink it; no dependency was worth it.
  If the plate cannot be written at all, the take still renders full-frame.
- `serve` takes concurrent connections, one thread each, serialized onto the
  single browser by a mutex held for one op at a time. `stop_recording` holds
  that mutex for the whole render, so other clients wait out the encode.
- Frames spool to one append-only temp file as they arrive, and the render pass
  holds one frame at a time, so memory does not grow with take length. Disk
  does; see [Resource envelope](#resource-envelope).
- There is no way to re-render an existing take: the telemetry sidecar is for
  reading, and nothing loads it back.
