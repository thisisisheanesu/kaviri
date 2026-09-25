# kaviri

kaviri records a web app from a script and renders a finished MP4: a headless
browser driven over CDP by newline-delimited JSON ops, with the camera zooming
and panning onto each interaction the way Screen Studio does for a human.
It is built for agents rather than people, so the demo video becomes a build
artifact: it is re-recorded in CI, and the build fails when the take filmed
nothing.

Apache 2.0, one binary, no account, nothing phones home.

*kaviri* is ChiShona for "twice, a second time" (ka-VEE-ree).

```sh
kaviri record --script demos/checkout.jsonl --out checkout.mp4
```

## The demo video

There is no video embedded in this file yet, and the honest reason is that the
one worth showing is the one CI makes. `.github/workflows/demo.yml` records
`examples/demo.jsonl` on every push that touches `src/`, in two shapes (a wide
README take and a 9:16 vertical one), and uploads each MP4 as a workflow
artifact. The newest pair is on the Actions tab of this repo.

Making your own takes about thirty seconds, below.

## Install

Build it from source:

```sh
git clone https://github.com/thisisisheanesu/kaviri
cd kaviri
cargo build --release
```

What it needs, all in user space, no sudo:

- **Rust 1.75 or newer** to build. `rust-version` in `Cargo.toml` is the source
  of truth.
- **A Chromium or Chrome binary**, found at run time rather than bundled:
  `chromium` on PATH, or `--chromium`, or `$KAVIRI_CHROMIUM`. Anything that
  speaks CDP `Page.captureScreenshot` and `Emulation.setDeviceMetricsOverride`,
  which in practice is Chromium 90 and up.
- **ffmpeg with libx264** on PATH, or `$KAVIRI_FFMPEG`. Version 4 onwards. It is
  deliberately not bundled: the builds that write H.264 are the GPL ones, and
  kaviri's licence stays its own business.
- **Linux or macOS.** Windows is untested and unsupported.

Both binaries are checked before the browser launches, so a missing encoder
costs you a second rather than a whole take. `kaviri doctor` prints what it
found and where.

## Thirty seconds

From the repo root, with nothing running and no network:

```sh
./target/release/kaviri record --script examples/form.jsonl --out demo.mp4
```

That script opens the local page in `examples/demo.html`, types a name and an
email into it, clicks the button and waits for the confirmation. You get
`demo.mp4`: a zoom into each field as it fills, a pan that follows the text
across, and a return to the wide shot in between.

Run it from the repo root. `examples/form.jsonl` navigates to a relative path,
and a bare path is resolved against the working directory before it becomes a
`file://` URL.

The four subcommands:

```
kaviri record --script demo.jsonl --out demo.mp4   # a scripted take, end to end
kaviri serve                                        # NDJSON ops on stdin, results on stdout
kaviri doctor                                       # what chromium and ffmpeg resolve to
kaviri presets | kaviri backgrounds | kaviri frames # the named shapes, backdrops and device frames
```

`serve` is the mode an agent holds open: it drives one browser across many ops,
so a long session is one take rather than one video per step.

## The op protocol

One JSON object per line, in and out.

```jsonc
{"op":"start_recording","path":"out.mp4"}   // path optional; --out wins in record mode
{"op":"navigate","url":"https://example.com"}  // bare paths become file://
{"op":"navigate","url":"https://slow.example","timeout_ms":60000}  // load budget, default 25s
{"op":"click","selector":"#buy"}            // or {"op":"click","x":400,"y":300}
{"op":"hover","selector":".card","at":[0.2,0.5],"ms":800}  // glide the mouse; pointer effects follow
{"op":"type","selector":"#name","text":"Ada"}  // typewriter_ms optional, default 18
{"op":"type","text":"Ada"}                  // no selector: types into the focused element
{"op":"press","key":"Meta+K"}              // a real key or chord; repeat, interval_ms, hold_ms
{"op":"scroll","y":600,"smooth":true}       // y is an absolute document offset
{"op":"wait","ms":800}                      // a fixed pause
{"op":"wait","selector":".loaded"}          // or wait for an element, default budget 20s
{"op":"wait","selector":".done","timeout_ms":180000}  // raise it for slow work
{"op":"wait","selector":".done","visible":false}      // presence only, skip the visibility check
{"op":"mark","label":"checkout"}
{"op":"stop_recording"}                     // renders the MP4
```

Every executed op answers with one line on stdout, in `record` and `serve`
alike:

```jsonc
{"ok":true,"result":{"event":"mark","kind":"click","t":3.14,"box":[40,120,180,44]}}
{"ok":false,"error":"selector matched a non-visible element: #done","op":"click","index":7}
```

Each op also emits a timestamped mark carrying the target's bounding box. That
telemetry is the entire input to the camera.

A failing op is reported in band and, in record mode, ends the script. kaviri
still stops the recording, renders what it captured and exits non-zero, so you
get a partial video and a clear error rather than nothing.

### Things worth knowing before you write a script

- **`wait` takes `ms` or `selector`, never both.** Passing both is an error,
  because the natural reading of it ("wait for this, at most this long") is not
  what it would do. Bound a selector wait with `timeout_ms`.
- **A selector wait checks visibility, not just presence.** An element in the
  DOM with `display:none` does not satisfy it. Pass `"visible":false` when you
  genuinely mean presence only.
- **`scroll` needs `y`, and `y` is absolute.** A document offset in CSS pixels,
  not a delta. A missing or non-numeric `y` is an error rather than a silent
  scroll to the top.
- **`type` without a selector types into `document.activeElement`,** and errors
  if nothing editable is focused, because the alternative is characters going
  nowhere and the op reporting success. It contributes no bounding box, so it
  earns no zoom of its own.
- **`click` and `type` refuse invisible and obscured targets.** A zero-area or
  `visibility:hidden` element is an error, and so is one covered by something
  else, which names the element that is on top.
- **Durations are numbers of milliseconds.** `ms`, `timeout_ms` and
  `typewriter_ms` take any finite non-negative number. A string, a negative or a
  NaN is an error rather than a silent fall back to the default.

### Presets

A take has three sizes, and they are not one number: the **viewport** the page
lays out in, the **capture** resolution (viewport times scale), and the
**video** it renders to. A vertical take wants a phone-width viewport so the
site lays out like a phone, a high capture so zooms crop into real pixels, and a
1080x1920 file. `--preset` sets all three.

| preset | viewport | video | for |
|---|---|---|---|
| `desktop` | 1470x830 | 1470x830 | the default: a laptop window |
| `tiktok` / `reels` / `shorts` | 432x768 | 1080x1920 | 9:16 vertical, phone layout |
| `square` | 540x540 | 1080x1080 | 1:1 feed posts |
| `landscape` | 960x540 | 1920x1080 | 16:9 1080p, large type for a projector |
| `readme` | 1100x620 | 1100x620 | sits in a README without scaling |
| `phone` | 390x844 | 1170x2532 | a real phone's viewport and pixels |

A preset is a starting point: `--width`, `--height`, `--scale`, `--out-width`
and `--out-height` after it still win. A vertical frame also wants its own shot
list, so scroll rather than pan and keep every beat short.
`examples/ngano-vertical.jsonl` is one written that way.

Every take is composited onto a backdrop, the way Screen Studio and Cleanshot
frame a recording: the content inset with rounded corners and a soft drop
shadow. `kaviri backgrounds` lists the nine built-in plates. The default is
`auto`, which probes the rendered take at two frames a second, folds it into a
chroma-weighted mean hue and a mean lightness, and picks the backdrop nearest a
split-complementary target while penalising one that sits within 0.35 of the
content's own lightness. Nothing is random: the same recording always picks the
same backdrop. `--background none` renders full frame, and `--background
photo.jpg` uses any image ffmpeg can read, scaled to cover.

`--frame` films the take as if it were on a device: `macos`, `windows`,
`linux`, `android`, `ios`, `android-emulator` or `ios-simulator`. The desktop
frames draw a browser window by default, with the page's own favicon, title and
URL in the tab and address bar; `--frame-style app` draws a bare title bar
instead, as an installed app. The phone frames draw the bezel, status bar and
home indicator, and make the browser claim to be that phone, so the site serves
its phone layout; with no size given they film at that phone's viewport.
`--frame-style browser` adds the phone's address bar. The emulators draw the
Android Emulator's side toolbar or the iOS Simulator's device title bar.

```
kaviri record --script demo.jsonl --out demo.mp4 --frame macos --desktop on
kaviri record --script demo.jsonl --out demo.mp4 --frame ios --clock 10:08 --battery 40
kaviri record --script demo.jsonl --out demo.mp4 --frame windows --frame-style app \
  --frame-icon terminal --frame-title "Parcel" --background wall.jpg
```

`--desktop on` puts the window on its desktop: the macOS menu bar and dock, the
Windows taskbar, or the GNOME top bar and dash; an emulator gets the desktop it
runs on (`--desktop windows` to choose). `--frame-icon` is `auto` (the favicon),
`none`, one of the built-in icons `kaviri frames` lists, or an image file.

The dock is its own option. `--dock on` draws the macOS dock (or the GNOME dash,
or the Windows taskbar) on a desktop frame even without the menu bar, and
`--desktop on --dock off` keeps the menu bar and drops it. `--dock` also takes
icons, named groups and icon image files, mixed freely: `--dock dev,maps,myapp.png`
(the groups are `dev`, `creative`, `office`, `social`, `media` and `minimal`). A
file is taken as a finished icon and drawn as is, so a real app icon you have
the rights to sits in the dock looking like itself. `--dock-position left`
or `right` stands it up a side, as macOS allows, and `--dock-size` sets the tile
size in points. The built-in icons are small illustrations in the desktop app
icon idiom (a folder, an envelope, a calendar page, a notepad, a gear), clipped
to the macOS squircle on a Mac and a rounded square elsewhere. `--icon-set`
restyles them all: `color` (the illustrations, the default), `pastel`, `dark`, `mono`, `tinted` (in `--icon-tint`), `glass` or
`outline`. `--frame-theme`, `--frame-title`,
`--frame-url`, `--device-name` and `--clock` override what the chrome would
otherwise read off the page. Under a frame, `--background auto` is that
system's own wallpaper. The chrome is HTML, drawn once per take by the same
headless browser, so its text uses real fonts and stays sharp at any size.

Headless capture has no OS cursor, so kaviri draws one into the page as vector
SVG, with a click ripple. It is part of the page, so the zoom transform carries
it along and it cannot drift away from the click it belongs to. `--cursor none`
turns it off.

A canvas-heavy page (a game, a chart, a generative comic) paints five to ten
frames a second in headless Chromium, and no camera makes that smooth.
`--slowmo 8` runs the page's own clock eight times slower while it is filmed,
stamps every frame in page time, and so plays back at normal speed with eight
times the real frames. Script timings mean the same at any factor; the take
costs that many times its length to record.

## The GitHub Action

A demo video goes stale the moment the UI moves, and nobody re-records it,
because that means blocking out an afternoon. Put the script in the repo next
to the code it films and the demo changes when the product does.

```yaml
- uses: actions/checkout@v4

- name: Serve the app
  run: |
    npm run build && npx serve -l 8099 dist &
    until curl -sf http://127.0.0.1:8099/ >/dev/null; do sleep 0.25; done

- uses: thisisisheanesu/kaviri@v1
  with:
    script: demos/checkout.jsonl
    out: checkout.mp4
    preset: readme          # or tiktok, square, landscape, desktop, phone
```

The MP4 is uploaded as a workflow artifact. Pass `upload: false` if you would
rather push it somewhere yourself.

The action installs ffmpeg and Chromium, installs a Rust toolchain rather than
assuming one, builds kaviri from its own checked-out source and runs the take.
Supported runners are `ubuntu-*` and `macos-*`, GitHub-hosted or self-hosted:
apt on Linux, brew on macOS, and any other `RUNNER_OS` exits with a message
rather than guessing.

Two things it sets that a local run does not, and should not:

- `KAVIRI_CHROMIUM_ARGS=--no-sandbox --disable-dev-shm-usage`, because
  Chromium's sandbox cannot start inside a CI container. That is a genuine
  reduction in isolation, so kaviri never does it on its own and the runner opts
  in. The same variable works locally if you need it.
- `TMPDIR` pointed at the runner temp, because `/tmp` on a hosted runner is
  small and shared, and both the frame spool and the CFR intermediate live
  there.

**Pinning the action to a tag fixes kaviri's behaviour, not its output.** Two
runs of the same script on the same commit are not frame-identical: marks and
frame times come from a wall clock and the capture cadence follows machine load,
so durations, waypoint spacing and the exact pixels all move a little. What
pinning buys you is that the framing rules, the zoom ladder and the backdrops
stay put. If you need a byte-stable asset, render once and commit the MP4.

## Failing the build when the take filmed nothing

A recorder that produces a file is not a recorder that produced a video. A take
that filmed a Chrome error page, or a dev server that never came up, is a freeze
frame held for thirty seconds, and a duration check passes it.

`.github/workflows/demo.yml` in this repo is the real thing rather than an
illustration. After each take it runs two gates:

```sh
# 1. it is longer than two seconds
dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 -i "$OUT")
awk -v d="$dur" 'BEGIN { exit (d > 2.0) ? 0 : 1 }'

# 2. the page actually moved: compare the first frame to the last
ffmpeg -nostdin -v error -y -ss 0.5     -i "$OUT" -frames:v 1 first.png
ffmpeg -nostdin -v error -y -sseof -0.5 -i "$OUT" -frames:v 1 last.png
ffmpeg -nostdin -v error -i first.png -i last.png -lavfi "psnr=stats_file=psnr.log" -f null -
```

`psnr_avg` comes back as the literal `inf` for two identical frames, which is
handled as its own case rather than coerced to a number, and anything above 50dB
is indistinguishable by eye. The script navigates, clicks and types, so a demo
whose first and last frames are the same picture did not film the product. The
job goes red.

That is the part that makes the video a build artifact. The zoom is what makes
it watchable.

## How the camera works

Frames arrive at irregular intervals and are normalised to constant-rate 30fps
before any time-based arithmetic. Each interaction mark then becomes a zoom
event, and the events are rendered as a generated ffmpeg `zoompan` expression
over that intermediate.

- **Zoom level** starts on a ladder set by the target's height (1.85 for a small
  control, 1.7, 1.5 for a large one) and is then bounded by whichever axis runs
  out first, with a 15% margin around the target. A target wider than the frame
  gets no zoom at all, which is the honest answer. Choosing on height alone used
  to hang a full-width nav bar off both sides of the crop.
- **Timing** leads the interaction by 0.45s, holds for 2.1s after it, and eases
  with a cubic smoothstep `s = p*p*(3-2p)` over 0.7s at each end. The crop is
  clamped to the frame.
- **Consecutive interactions** within 1.3s of the end of a hold merge into one
  event with path waypoints, so the camera pans between them instead of zooming
  out and back in. Waypoints are joined by straight lines, with the ease at the
  two ends of the whole move: easing each segment separately brings the camera
  to a stop at every sample, which reads as stepping.
- **The last interaction** still gets its zoom. kaviri holds the final frame for
  up to 2.8 extra seconds so the hold and the ease-out have somewhere to live.
  If an interaction still lands too late, it says so on stderr rather than
  quietly dropping the zoom.
- **The frame sits left of the control, not centred on it.** A control sits to
  the right of whatever it acts on: the send button after the message, the caret
  after the words already typed. Centre on the control and the frame fills with
  empty space on its right while the thing you wanted to read falls off the left.
  The crop leans left by 18% of its width while typing and 12% on a click,
  capped so the target's own right edge always stays comfortably inside.

### Why it follows the caret

kaviri records agents. There is no hand on a mouse to follow, and the pointer it
draws is a prop: it is parked wherever the field was clicked and stays there
while a whole sentence is typed. The thing that moves, and the thing a viewer is
reading, is the caret. So that is what the camera follows.

Finding it means measuring it. For contenteditable that is the selection's
rectangle; for `input` and `textarea` there is no caret rectangle in the DOM, so
the text up to the caret is mirrored into a hidden element with the same
typography and the offset of a zero-width span at its end is read back.

That measurement costs a round trip to the page, so it happens exactly twice per
typing op, once before the first character and once after the last. The pan is
laid down afterwards, interpolated between the two at 0.12s spacing. Measuring
between keystrokes put the round trip inside the typing rhythm: the words came
out slower than the requested speed, and the camera moved in steps because the
samples were as uneven as the latency.

## What it costs the app you are filming

There are two capture paths, and `--scale` chooses.

Above 1, which is the default and every preset, kaviri runs a
`Page.captureScreenshot` pump: a fresh viewport screenshot roughly every 25ms,
one request outstanding at a time, spooled as JPEG. Each one is a full
compositor pass plus a JPEG encode inside the same browser that is running the
app you are filming. On a 1470x830 viewport at 2x that is roughly 15 to 25 MB/s
and a busy core. The filmed app runs measurably slower than it does unrecorded:
animations stutter, and a `wait` that is comfortable by hand can time out on a
loaded machine.

At `--scale 1` kaviri switches to the DevTools screencast, where the browser
pushes frames as it paints them. That is far cheaper and takes kaviri almost
entirely out of the app's way, but it caps frames at the CSS viewport whatever
`maxWidth` asks for, so zooms crop into upscaled pixels. If the take is for a
README at native size, or the app is timing-sensitive, or you are on a shared CI
runner, that is the better trade.

### Resource envelope

Plan for this before a long take, because the failure mode is a full disk.

- **Temp space.** Frames spool to one append-only file under `TMPDIR`
  (`--spool-dir` moves it). At the default `--scale 2` that is 15 to 25 MB/s, so
  a five-minute take is several gigabytes.
- **A spool cap.** 8 GiB by default (`--max-spool-bytes`). On reaching it kaviri
  stops capturing cleanly and renders what it has rather than dying on ENOSPC.
  It also refuses to start a take with less than 512 MiB free, naming the
  directory, and clamps its own cap to the free space it sees.
- **A second large file at render time.** The CFR intermediate is a full-length
  H.264 encode in a per-run temp directory, plus the backdrop plate PNG at about
  4 bytes per output pixel. `--keep-temp` retains both and prints where they are.
- **CPU.** One core for the browser, one for kaviri's pump, one for ffmpeg
  during the render, which runs after capture ends and is not gentle.
- **Memory is flat.** Frames go to disk as they arrive and the render holds one
  at a time, so a long take costs disk, not RAM.

### Environment

Every one of these has a flag. The variables exist so a CI job can set them once
for a whole matrix.

| variable | what it does |
|---|---|
| `KAVIRI_CHROMIUM` | browser binary, same as `--chromium` |
| `KAVIRI_CHROMIUM_ARGS` | extra Chromium flags, whitespace-separated |
| `KAVIRI_FFMPEG` | ffmpeg binary, or a name to resolve on PATH |
| `TMPDIR` | where the frame spool and the CFR intermediate live |
| `KAVIRI_SPOOL_DIR` | the spool alone, same as `--spool-dir` |
| `KAVIRI_MAX_SPOOL_BYTES` | the spool cap, same as `--max-spool-bytes` |
| `KAVIRI_KEEP_TEMP` | keep the intermediates, same as `--keep-temp` |
| `KAVIRI_TELEMETRY` | where to write the sidecar; `0` or `off` suppresses it |
| `KAVIRI_TOKEN` | the `serve --port` token, instead of a generated one |
| `KAVIRI_DEBUG` | log the CDP traffic on stderr |

The tool was called lensa before the rename, so each variable is also read under
its old `LENSA_` name when the `KAVIRI_` one is unset. `KAVIRI_` wins if both
are set. Nothing else carries the old name.

### The telemetry sidecar

kaviri can write a JSON sidecar with the raw marks and the computed zoom events.
It is off by default. `--keep-temp` puts it next to the video as
`<out>.telemetry.json`, and `KAVIRI_TELEMETRY=<path>` writes it wherever you
name. `KAVIRI_TELEMETRY=0` suppresses it even under `--keep-temp`.

It stays off because its `marks` array carries every `navigate` label verbatim:
full URLs, query strings, any token in them, and local `file://` paths. Read it
before you upload it as a build artifact.

### Serving over TCP

`kaviri serve` on stdin needs no authentication: the ops come from the process
that started it. `kaviri serve --port <n>` does not have that property. Binding
to loopback is not a trust boundary against a browser, because any page the user
visits can `fetch()` a loopback port, and the op set can navigate to `file://`
URLs and write an MP4 to a path of the caller's choosing.

So `--port` prints a token on stderr at startup and the first line of every
connection must be `{"op":"hello","token":"…"}`. Any line that is not JSON drops
the connection rather than being partly executed, which is what makes a stray
HTTP request a disconnect instead of a script. Pick a port outside Chrome's
debugging range; kaviri does not default to one. Prefer stdin when one client is
enough.

## Limitations

- **Video only.** Audio capture is not implemented. `--audio` warns and is
  otherwise ignored. The plan is a dedicated PipeWire or Pulse sink for the
  browser process, muxed against the same clock. For now, add narration in an
  editor afterwards.
- **Takes are not reproducible frame for frame.** The whole timeline comes from
  a wall clock, so the same script on the same commit gives you the same film,
  not the same file.
- **The default capture path competes with the app it films** for CPU, as above.
  `--scale 1` is the way out.
- **Window chrome is opt in.** A take is a backdrop and nothing else until
  `--frame` asks for a device. The chrome is drawn, not captured: it is a
  plausible likeness of each system, with the fonts the machine running kaviri
  has, and the icons are kaviri's own rather than any vendor's.
- **`serve` serialises.** Concurrent connections each get a thread, but they are
  serialised onto the single browser by a mutex held for one op at a time.
  `stop_recording` holds it for the whole render, so other clients wait out the
  encode.
- **There is no way to re-render an existing take.** The telemetry sidecar is
  for reading; nothing loads it back.
- **The backdrop plate is a stored-deflate PNG**, written without an image
  crate, so it costs about 4 bytes per output pixel on disk while the render
  runs. A real deflate would shrink it and no dependency was worth it. If the
  plate cannot be written at all, the take renders full frame.
- **Windows is untested.** So is an embedded-webview backend (wry, WebKitGTK),
  which is planned where the dev headers exist. Today it is headless Chromium
  over CDP, which needs no native build dependencies and works on Wayland-only
  machines.

## Where the code is

- `src/cdp.rs` is the synchronous CDP client (one websocket, single-threaded
  pump: every wait drains events, acks screencast frames and keeps the
  screenshot pump on cadence) plus the frame spool.
- `src/ops.rs` is the op protocol, the cursor overlay, the caret measurement and
  the telemetry marks.
- `src/zoom.rs` turns marks into zoom events into ffmpeg expressions, and runs
  the two-pass render.
- `src/backdrop.rs` is the built-in backdrops, the auto picker and the
  dependency-free PNG plate.
- `src/device.rs` is the device frames: where the window or handset sits, and
  the chrome drawn over it as HTML.
- `src/main.rs` is the CLI.

`cargo test` runs the suite, including two tests that shell out to a real ffmpeg
to validate the generated filter graph and the hand-written PNG. They fail
loudly rather than skipping when no ffmpeg is present; set
`KAVIRI_SKIP_FFMPEG_TESTS=1` if you genuinely want them skipped.

## Licence

Apache 2.0, unconditional, on everything in this repository. No revenue
threshold, no field-of-use restriction, no contributor licence agreement to
sign. Run it locally, run it in your CI, put it in your product. See
[LICENSE](LICENSE).

A hosted version is being built at kaviri.dev for people who would rather not
run the browser themselves. It is not live yet, and nothing here depends on it.
