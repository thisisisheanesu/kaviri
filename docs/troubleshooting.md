# Troubleshooting

Real failures, with the error text kaviri actually prints and what it means. If you are reading
this because something broke in CI, [ci.md](ci.md#what-a-broken-take-looks-like-in-the-log) has a
symptom table aimed at runner logs.

**Start with `kaviri doctor`.** It prints the resolved Chromium and ffmpeg paths with their
version strings, where the frame spool will live and what the cap is, and exits non-zero if
either binary is missing. It creates nothing and records nothing.

```
$ kaviri doctor
kaviri 0.1.0
chromium: /usr/bin/google-chrome
  Google Chrome 141.0.7390.54
ffmpeg: /usr/bin/ffmpeg
  ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023 the FFmpeg developers
frame spool: under /tmp
spool limit: 8192 MiB (--max-spool-bytes)
```

Exit codes across the whole tool: `0` success, `1` a failure during the run, `2` a usage error on
the command line, `130` when you interrupt twice.

---

## ffmpeg not found

```
kaviri: error: ffmpeg not found. Looked in:
  ffmpeg (on PATH)
  /usr/bin/ffmpeg
  /usr/local/bin/ffmpeg
  /opt/homebrew/bin/ffmpeg
  /snap/bin/ffmpeg
  /home/you/.local/bin/ffmpeg
Install one (apt install ffmpeg, brew install ffmpeg, or a static build) or point
KAVIRI_FFMPEG at it.
```

**This is checked before the browser launches**, in both `record` and `serve`. It used to be
looked up inside the render, which only runs from `stop_recording`, so a machine without ffmpeg
launched the browser, drove the whole script, spooled every frame and only then said it could not
encode. The frames were deleted with the session. Now a missing encoder costs you a second.

The search order is: `KAVIRI_FFMPEG` if set, then `ffmpeg` on PATH, then the list above, then any
`ffmpeg` one level down inside `~/.local/opt/` (static tarballs unpack into a versioned directory
and people leave them there). Each candidate is actually executed with `-version` before it is
accepted: a stub, a dangling wrapper or a binary for the wrong architecture is worse than no
candidate, because it would be selected and then fail after the whole take had been captured.

ffmpeg is deliberately not bundled. The builds that can write H.264 are the GPL ones, and
shipping one would mean shipping its licence.

### KAVIRI_FFMPEG points at something that will not run

```
kaviri: error: KAVIRI_FFMPEG points at /opt/ffmpeg/bin/ffmpeg, which does not run
kaviri: error: KAVIRI_FFMPEG points at ffmpeg7, which is not a runnable ffmpeg on PATH
```

Two different messages because a bare name means "this one, off PATH" and a name with a separator
in it means a filesystem path. `KAVIRI_FFMPEG=ffmpeg7` is a supported way to pick a specific build.

### It is found, but the render fails

```
kaviri: error: ffmpeg pass 2 (zoompan) failed, exit exit status: 234. The filter graph is in
/tmp/kaviri-render-demo-31182-a3f1.../filter.txt
[Parsed_zoompan_1 @ 0x...] Error when evaluating the expression 'NaN'
```

The generated filter graph is written to disk on every render, and the render's temp directory is
retained when pass 2 fails. Machine-generated expressions are the thing most likely to be
rejected, and the only useful report is the expression itself. Open the file, find the axis that
went wrong, and check the telemetry sidecar for the mark that produced it.

If you see the literal `NaN` in there, something upstream produced a non-finite number. The
`--width`, `--height` and `--scale` flags now reject the values that cause it (`--scale nan`,
`--scale inf`, `--width 0`) at the flag rather than at render time, so this should be rare.

---

## Chromium will not start

```
kaviri: error: no Chromium/Chrome binary found (set KAVIRI_CHROMIUM or use --chromium)
```

kaviri tries `chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable` and
`/snap/bin/chromium`, in that order, running each with `--version`. `--chromium` and
`KAVIRI_CHROMIUM` skip the search entirely.

### It launched and then died

```
kaviri: error: /snap/bin/chromium exited during startup (signal: 9 (SIGKILL))
without exposing a debugger endpoint
```

There is a two second grace period before this is reported, for launchers that fork and let the
parent exit while the real browser is still coming up behind a process kaviri can no longer see.
A browser that has already exited is never going to answer, so this appears in about two seconds
rather than in forty.

`signal: 9` on a machine with little memory is the OOM killer. See below.

### It never answered

```
kaviri: error: chromium did not expose a debugger endpoint in 40s
```

The browser process is alive but the DevTools HTTP server never came up on the port kaviri
picked. Run with `KAVIRI_DEBUG=1`, which logs every failed `/json/version` poll and any body that
came back without a websocket URL.

Two causes account for nearly all of these.

**Snap confinement.** On Ubuntu the `chromium` package is often a transitional stub for the snap.
A snap-confined browser cannot read a profile directory or a frame spool under `/tmp`, and it
fails by never opening the debugger port, which surfaces forty seconds later as an error naming
neither snap nor the profile. Point `KAVIRI_CHROMIUM` at a real binary:

```sh
KAVIRI_CHROMIUM=/usr/bin/google-chrome kaviri record --script demo.jsonl --out demo.mp4
```

The GitHub Action pins the absolute path it discovered for exactly this reason.

**A container with no sandbox.** Chromium's sandbox cannot start inside most containers. kaviri
does not pass `--no-sandbox` on its own, because that is a genuine reduction in isolation and it
belongs to whoever owns the machine:

```sh
KAVIRI_CHROMIUM_ARGS="--no-sandbox --disable-dev-shm-usage" kaviri record …
```

### Low memory

This is the common one on a small VM, a shared runner or a container, and it shows up as
`signal: 9`, as a browser that dies partway through a take, or as a forty second timeout.

Three things, in order of how much they help:

**1. `--disable-dev-shm-usage`.** `/dev/shm` is 64MB in a default Docker container, and Chromium
uses it for shared memory between its processes. Running out of it kills renderers. This flag
moves that traffic to disk:

```sh
KAVIRI_CHROMIUM_ARGS="--disable-dev-shm-usage" kaviri record …
```

The action always passes it. If you are running kaviri in a container yourself, pass it too, or
give the container a larger `/dev/shm`.

**2. Drop `--scale`.** `--scale 2` makes every captured frame four times the pixels, and the
compositor surface is sized to match. At `--scale 1` kaviri switches to the push-based DevTools
screencast, which needs no round trip and no supersampled surface. Zooms get softer. On a
memory-constrained machine that is a good trade, and on a machine that cannot record at all it is
the difference between a soft take and no take.

**3. Use a smaller preset.** `--preset readme` is 1100x620 against `desktop`'s 1470x830, and
`--preset square` is smaller again. The memory a take costs scales with viewport times scale
squared.

Memory in kaviri itself is flat: frames go to disk as they arrive and the render pass holds one
at a time, so a long take costs disk rather than RAM. If a process is growing, it is the browser.

### It works locally and not over SSH

kaviri runs the browser with `--headless=new`, so there is no display dependency. If a take fails
only over SSH, check `TMPDIR`: the session directory, the browser profile and the frame spool all
live under it, and some hardened systems mount it `noexec` or per-session.

---

## A selector that never becomes visible

```
kaviri: error: examples/demo.jsonl op 9 (wait): wait: selector never became visible after
20.0s: .results li
  raise it with {"op":"wait","selector":"...","timeout_ms":60000}, or add "visible": false to
  wait for presence only
```

The elapsed time in that message is measured, not the budget. Work through it in this order.

**Is it actually visible, or merely present?** A selector wait requires
`getClientRects().length && getComputedStyle(e).visibility !== 'hidden'`. A `display:none` node is
in the DOM from first paint, which is why presence-only waits used to return on their first poll
and let the script race ahead of the page. If you genuinely mean presence, say so:

```jsonc
{"op":"wait","selector":".done","visible":false}
```

**Is 20 seconds enough?** It is fine for a page to render something and far too short for
anything that has to think first. A take that waits on a model finishing its answer should say
how long it is prepared to wait:

```jsonc
{"op":"wait","selector":".answer","timeout_ms":180000}
```

**Is the machine too loaded?** The default capture path is a screenshot pump running inside the
same browser that is running the app, so the filmed app is measurably slower than it is
unrecorded. A wait that is comfortable by hand can time out during a recording. `--scale 1` takes
kaviri almost entirely out of the app's way.

**Did you write both `ms` and `selector`?**

```
wait takes ms or selector, not both; use timeout_ms to bound a selector wait
```

That is an error rather than a combination. The old code took the `ms` branch and never looked at
the selector.

### Related: a selector that resolves but cannot be clicked

`click` and `type` refuse four ways, and each names the actual problem:

```
selector not found: #go
selector matched a non-visible element (display:none or visibility:hidden): #go
selector matched a zero-size element, so there is no point to aim at: #go
selector #go is covered by <div#cookie-banner> at its centre point; close the overlay, or
target the element that is actually on top
```

The last one is the one you will hit. Cookie banners, modals and toast notifications sit over the
thing you meant to click, and dispatching at the covered element's centre would be a real click on
the overlay, reported as a success. Dismiss the overlay first, or target what is on top.

The zero-size and hidden refusals exist for the same reason: a hidden element has a box of all
zeros, so a click at its centre lands in the viewport's top-left corner.

You do not need to scroll to an element before clicking it. Both ops call
`scrollIntoView({block:'center', inline:'center'})` and wait 120ms for layout to settle first.

---

## A take that films nothing

Three distinct failures wear this face.

### No frames at all

```
kaviri: error: stop_recording: no frames were captured; was start_recording sent?
```

There is no file. Either `start_recording` never ran, or capture was abandoned before the first
frame landed. The variant

```
stop_recording: no frames were captured (ws read: Connection reset by peer)
```

names the reason the socket died.

### Frames, but nothing changed on the page

This is the one the CI check exists for. You get a well-formed MP4 of the right length that is a
single held picture: a Chrome error page, a dev server that never came up, a spinner that never
resolved. kaviri cannot tell the difference, because from its side the take succeeded.

```
first-to-last psnr: inf
the first and last frames are identical; the take filmed nothing
```

Watch the first frame. It is almost always `ERR_CONNECTION_REFUSED` or a blank page, which means
the `navigate` op resolved but the app was not there. Note that `navigate` only fails when
Chromium reports an `errorText`; a server that answers with a 500 page is a successful
navigation to a page you did not want.

Add a selector wait on something that only exists when the app is really up, before
`start_recording`:

```jsonc
{"op":"navigate","url":"http://127.0.0.1:8099/"}
{"op":"wait","selector":"#q","timeout_ms":30000}
{"op":"start_recording"}
```

### Frames and movement, but no zooms

```jsonc
{"ok":true,"result":{"event":"recording_rendered","path":"demo.mp4","duration":31.4,"zoom_events":0,…}}
```

`"zoom_events": 0` means the camera never moved. Only `click` and `type` marks that resolved a
bounding box produce zooms; `navigate`, `scroll`, `wait` and `mark` never do. A script made
entirely of navigations and scrolls renders a correct, static, wide take.

If you did click and type, check stderr for:

```
kaviri: the interaction at 28.4s is too close to the end of the 29.0s take to be zoomed;
add a trailing wait before stop_recording
```

A zoom needs roughly `HOLD_AFTER + EASE` of footage after its mark. End the script with a
`{"op":"wait","ms":1600}`. See [camera.md](camera.md#the-tail).

### Capture stopped partway

```
kaviri: capture stopped after ws read: Connection reset by peer: the 612 frames already
captured are kept for rendering
kaviri: warning: capture ended early: ws read: Connection reset by peer
```

The browser died mid-take. Teardown is not allowed to lose the take: every frame is already on
disk, so kaviri renders the 612 frames it has and reports the CDP failure alongside the video as a
`warning` field rather than instead of it. The video is short and real. Fix the browser and
re-run.

---

## Disk and the frame spool

### It refuses to start

```
kaviri: error: only 218 MiB free on /tmp; kaviri needs at least 512 MiB for the frame spool
(use --spool-dir)
```

Checked before the take rather than during it. A spool that runs out of space mid-write costs the
whole recording; a refusal costs a re-run with `--spool-dir` somewhere roomier.

```sh
kaviri record --spool-dir /mnt/scratch --script demo.jsonl --out demo.mp4
```

### It silently lowers its own cap

```
kaviri: frame spool limited to 3148 MiB by free space on /tmp
```

Not an error. kaviri clamps its 8 GiB default down to the free space it can see, minus 256 MiB of
headroom for everything else on the filesystem.

### It stops early

```
kaviri: frame spool reached its 8192 MiB limit; stopping capture and rendering what was
recorded (raise --max-spool-bytes)
```

The take ends there and renders what it has, rather than dying on ENOSPC. At `--scale 2` a busy
take writes roughly 15 to 25 MB/s, so 8 GiB is somewhere between five and ten minutes. Plan for
this before a long take: `--max-spool-bytes`, `--spool-dir`, or `--scale 1`.

Note that the whole run also needs room for the CFR intermediate, which is a full-length H.264
encode in a per-run temp directory, plus the backdrop plate PNG at about four bytes per output
pixel (roughly 8MB for 1080x1920).

### Where things live, and what survives

Every temporary file for a run lives in one 0700 directory under `TMPDIR`, named
`kaviri-<pid>-<token>`. The browser profile and the frame spool are both inside it, so one
removal cleans the run up and two concurrent kaviri runs sharing a temp directory cannot collide.
Renders get their own `kaviri-render-<stem>-<pid>-<salt>` directory, also under `TMPDIR`, for the
same reason: two takes rendering into the same output directory used to share one scratch
directory and overwrite each other's intermediates.

`--keep-temp` (or `KAVIRI_KEEP_TEMP=1`) keeps all of it and prints where:

```
kaviri: 842 captured frames kept at /tmp/kaviri-31182-9f2c.../spool-4a1b.jpgs
kaviri: render intermediates kept in /tmp/kaviri-render-demo-31182-a3f1...
```

A broken capture also switches spool retention on by itself, so the frames from a take that died
are recoverable even without the flag.

---

## Output and rendering

### The video is stretched or letterboxed unexpectedly

The viewport, the capture and the video are three different sizes, and a preset sets all three.
Overriding one of them by hand after a preset changes the aspect ratio of the capture without
changing the output, and the content is fitted and padded rather than stretched. That is
deliberate, and it is why `--preset tiktok --width 500` gives you black bands rather than a
distorted page. If you want a different shape, change `--out-width` and `--out-height` to match.

### `could not parse first frame's JPEG header`

The first frame on the spool is not a JPEG kaviri can read the SOF marker out of. This should not
happen with a healthy browser; treat it as a corrupt spool and re-run with `--keep-temp` so the
file survives for inspection.

### The backdrop is not the one you expected, or is missing

```
kaviri: backdrop unavailable (create /tmp/…/backdrop-tide.png: Permission denied);
rendering full-frame
```

A backdrop is decoration, so nothing about it is fatal. A failed probe falls back to `dusk`, and
a failed plate render falls back to the full-frame take. `auto` is deterministic: the same
recording always picks the same background. The line kaviri prints says which one and why:

```
kaviri: backdrop tide (auto, content hue 34° / lightness 0.91), content 986x556 at 56,32
```

`--background none` turns the whole thing off.

---

## Serve mode

### The process exits non-zero after a session that looked fine

```
kaviri: error: 3 op(s) failed
kaviri: error: a recording was started but never rendered
```

Both are deliberate. An exit status of 0 from a mode where every op failed and no video exists is
a green CI run over a broken take. In `serve` mode a failing op does not end the connection, so
the count is reported at the end instead.

### A connection is refused

```
kaviri: refusing a connection; 16 clients already attached
```

`--port` accepts up to 16 concurrent clients, each on its own thread, all serialized onto the one
browser by a mutex held for a single op at a time. `stop_recording` holds that mutex for the whole
render, so other clients wait out the encode.

### A connection is dropped immediately

```
this connection must start with {"op":"hello","token":"…"}; the token was printed on stderr
at startup
```

Every `--port` connection must open with a matching `hello`. Binding to loopback is not a trust
boundary against a browser: any page the user visits can `fetch()` a loopback port, and the op set
can navigate to `file://` URLs and write an MP4 to a path of the caller's choosing. The token is
what makes that fetch useless. Set it yourself with `KAVIRI_TOKEN` if you need it stable.

A `--port` connection is also strict about JSON: one unparseable line drops the connection rather
than being partly executed, so a stray HTTP request cannot half-run a script. On stdin a bad line
is reported and skipped.

Prefer stdin. It needs no token because the ops come from the process that started kaviri.

### A warning about a poisoned session

```
kaviri: warning: an op panicked while holding the session; the browser may be in an unknown
state
```

Something panicked mid-op. The session may be half-mutated. Finish the take if you can and
restart the process; do not start another recording on it.

---

## Interrupting a take

Ctrl-C once sets a flag. Every op loop checks it between ops and unwinds through the normal
teardown path, so the browser is killed, its profile is removed and any open recording is
rendered. Every cleanup kaviri does lives in a `Drop`, and the default signal disposition runs
none of them, which is why the handler flips a flag instead of exiting.

Ctrl-C twice exits immediately with 130. By then the browser is often the thing that is stuck, and
someone pressing it twice wants out now. Temp files may survive; they are in the run's
`kaviri-<pid>-<token>` directory under `TMPDIR`.

A take interrupted the first way reports `interrupted` and still renders what it captured.

---

## Things that are not bugs

- **Two runs of the same script are not frame-identical.** Marks and frame times come from a wall
  clock and the capture cadence adapts to machine load, so durations, waypoint spacing and the
  exact pixels move run to run. If you need a byte-stable asset, render once and commit the MP4.
- **`--audio` prints a warning and does nothing.** Audio capture is not implemented.
  `kaviri: --audio is not implemented yet (headless backend); ignoring`.
- **The filmed app runs slower than it does unrecorded.** Each screenshot is a full compositor
  pass plus a JPEG encode inside the browser that is running the app. `--scale 1` is the way out.
- **No telemetry sidecar was written.** It is off by default. `--keep-temp` puts it at
  `<out>.telemetry.json`, and `KAVIRI_TELEMETRY=<path>` puts it where you name.
  `KAVIRI_TELEMETRY=0` suppresses it even under `--keep-temp`. It is off by default because its
  `marks` array carries every `navigate` label verbatim: full URLs, query strings and any token in
  them. Read it before you upload it.
- **`LENSA_*` variables still work.** The tool was called lensa until the rename. Every variable
  is read under `KAVIRI_` first and `LENSA_` second, and the new name wins. That is a courtesy for
  scripts written before the rename, and nothing else carries the old name.
- **Windows is not supported.** Linux and macOS only, and the action refuses any other
  `RUNNER_OS` rather than guessing.
