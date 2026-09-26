# Changelog

All notable changes to kaviri are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and kaviri aims at [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is below 1.0 the op protocol and the CLI flags may change in
a minor release; each such change is listed under **Changed** with what a
caller has to do about it.

Two things are treated as part of the public interface and so always appear
here when they move: the NDJSON op protocol (op names, fields, and the
response envelope) and the CLI (modes, flags, exit codes). The exact pixels of
a rendered take are not: takes are not frame-identical between runs, by
design, because the zoom timeline is derived from wall-clock interaction
timings.

## [Unreleased]

The first release. Everything below is the production-readiness pass that
preceded it, recorded so that anyone reading the diff knows which changes were
deliberate. Trim this section down to a release summary, or promote it to a
dated `[0.1.0]` heading, when the tag is cut.

### Changed

- The tool is called **kaviri**, not lensa. The old name collided with an
  existing product. The binary, the crate, the temp paths and every message
  carry the new name, and the repository moved to
  `github.com/thisisisheanesu/kaviri`.
- Every environment variable moved from `LENSA_*` to `KAVIRI_*`:
  `KAVIRI_CHROMIUM`, `KAVIRI_CHROMIUM_ARGS`, `KAVIRI_FFMPEG`,
  `KAVIRI_SPOOL_DIR`, `KAVIRI_MAX_SPOOL_BYTES`, `KAVIRI_KEEP_TEMP`,
  `KAVIRI_TELEMETRY`, `KAVIRI_TOKEN`, `KAVIRI_DEBUG` and
  `KAVIRI_SKIP_FFMPEG_TESTS`. The old name is still read when the new one is
  unset, so a script written before the rename keeps working; it is a
  transition courtesy and will not be kept forever.
- The render intermediate left behind by `--keep-temp` is `.kaviri-tmp*`
  rather than `.lensa-tmp*`, so an old ignore rule no longer covers it.
- The licence is **Apache-2.0**, unconditionally. `LICENSE` was a placeholder
  that granted nothing while `Cargo.toml` claimed MIT; both now say
  Apache-2.0 and the full text ships with the crate.

### Added

- **The simple way to write a motion video.** `brand`, `beat` and `end` ops: a headline per
  beat and at most one `show` (image, type, code, list, stats, icons, chips, strike), expanded
  into scenes with rotating entrances and transitions, a drop on the third beat and a score
  whose sections are fitted to the beats. `docs/motion-simple.md`, published as
  `kaviri.dev/motion-simple.txt`, is short enough for any model to follow.
- **`kaviri motion`: motion graphics from a JSONL timeline.** A script of scenes, layers,
  animations, micro-interactions and sounds renders to an MP4 with a synthesized soundtrack on
  the same beat grid. Times take seconds, beats (`"2b"`) and bars (`"1bar"`); scenes lie end
  to end with eleven transitions; text animates per letter, word or line with 26 entrances
  and 18 exits; layers take keyframes with 27 eases, beat-synced loops, 3D, glows and trails;
  groups lay out as rows, grids, rings and 3D orbits; a component kit (window, phone, card,
  input, list, code, message, field, cursor, stat, rating and more) answers acts such as
  `type`, `click`, `toggle`, `select`, `count` and `strike`; particles cover stars, bursts,
  hyperspace rays, shockwaves and confetti. The score is built from a key, a progression and
  sections (`intro`, `build`, `drop`, `break`, `outro`) in three styles, or taken from a file,
  with whooshes, impacts, clicks and typing added where the picture implies them. Frames are
  rendered by seeking a deterministic page in parallel browsers (`--jobs`). `--check`,
  `--still`, `--preview`, `--from`/`--to`, `--audio-out`. Documented in `docs/motion.md`,
  with the whole format for a model in `docs/motion-llm.md` (published as
  `kaviri.dev/motion-llm.txt`) and prompts in `docs/motion-prompts.md`. Examples in
  `examples/motion/`.
- **Device frames.** `--frame macos|windows|linux|android|ios|android-emulator|ios-simulator`
  draws the take inside that system's window or handset. Desktop frames default to a browser
  window carrying the page's own favicon, title and URL (`--frame-style app` for a bare title
  bar). Phone frames draw the bezel, status bar and home indicator, and emulate the phone's
  viewport and user agent unless a size is given. `--desktop on|macos|windows|linux` adds the
  menu bar and dock or the taskbar. `--frame-theme`, `--frame-title`, `--frame-url`,
  `--frame-icon`, `--dock`, `--device-name`, `--clock` and `--battery` adjust the chrome, and
  `kaviri frames` lists the frames and the built-in icons. The dock stands on its own:
  `--dock on|off|<icons and groups>` (groups `dev`, `creative`, `office`, `social`, `media`,
  `minimal`), `--dock-position bottom|left|right` and `--dock-size`, and `--icon-set
  color|pastel|dark|mono|tinted|glass|outline` with `--icon-tint` restyles every built-in icon. The chrome is HTML rendered once per
  take by the recording browser, in a throwaway tab the filmed page never sees.
- **Under a frame the whole screen zooms.** Page, chrome, dock and wallpaper are composited
  first, at twice the video's size, and the camera runs over the composite, so a close-up
  brings the title bar or bezel in with it. The camera anchors on the card a control sits on:
  interactions on one card share one aim, so typing and clicking there hold the shot still with
  the button in frame. The playground zooms the whole scene the same way.
- **Platforms in the script, and phones that look filmed on a phone.** A `frame` op declares
  the platform in the `.jsonl` itself, with a field for every `--frame*` flag; `record` reads
  it before launch so a phone starts at the phone's viewport, and `serve` applies it on
  arrival. `--frame-style recording` (ios, android) looks like the phone's own screen
  recording: edge to edge, no bezel, the red recording indicator. The handsets, emulators
  included, are drawn as hardware with a metal band and side buttons, and phone takes show a
  tap dot (`--cursor touch`) instead of a mouse pointer. The playground has a platform picker
  that writes the `frame` line.
- **A dock that looks like one.** The built-in icons are illustrations rather than line
  glyphs, the macOS dock is frosted glass over a blurred wallpaper with squircle tiles, a
  separator and the Trash, and the menu bar's ink follows the wallpaper. `--dock` accepts icon
  image files, drawn as is.
- **Wallpapers and image backgrounds.** Five new backdrops, one per system (`hills`, `bloom`,
  `aubergine`, `material`, `aurora`), which `--background auto` uses under a frame and never
  picks for an unframed take. `--background <file>` takes any image ffmpeg can read.

- A `press` op that sends a real key or chord (`"Enter"`, `"ArrowDown"`,
  `"Meta+Shift+P"`, `"?"`) through `Input.dispatchKeyEvent`, with `repeat`,
  `interval_ms`, `hold_ms` and an optional `selector` to focus first. Until now
  a keyboard-driven app could not be filmed without hooks in the app itself.
- `kaviri doctor`, which prints the Chromium and ffmpeg kaviri resolved, and
  where it found them, without launching a take.
- `kaviri --version` / `-V`, and the version in the startup log line, so a
  rendered take can be traced back to the build that produced it.
- Preflight discovery of ffmpeg before the browser launches, so a missing
  encoder costs a second rather than the whole recording.
- SIGINT and SIGTERM handling. The first signal unwinds through the normal
  teardown so the browser, its profile and the frame spool are cleaned up; a
  second signal exits immediately.
- `--spool-dir` and `--max-spool-bytes`, plus `KAVIRI_SPOOL_DIR` and
  `KAVIRI_MAX_SPOOL_BYTES`, to place and bound the frame spool. The spool
  defaults to a cap of 8 GiB and refuses to start a take with less than
  512 MiB free, naming the directory in the error.
- A token handshake on `kaviri serve --port`. The token is printed on stderr at
  startup and must arrive as the first line of every connection.
- `timeout_ms` on the `navigate` op, matching `wait`.
- `LICENSE` (a placeholder pending the licence decision), `THIRD-PARTY.md`,
  `SECURITY.md` and this file.

### Changed

- Record mode now emits the same `{"ok":…}` response envelope as serve mode,
  including for the op that fails. Callers that parsed the bare mark object
  from record mode need to read `result`.
- `--help`, `kaviri presets` and `kaviri backgrounds` print to stdout and exit 0
  instead of printing to stderr and exiting 2.
- `serve --port` no longer suggests 9222, Chrome's own remote-debugging port,
  as an example.
- The telemetry sidecar is written on request rather than unconditionally, and
  a failure to write it is reported rather than swallowed. It contains every
  URL the take visited, which is now documented.
- Numeric CLI flags are range-checked at parse time. Width and height must be
  64..16384, output width and height 64..8192, `--scale` finite in 0.5..4, and
  `--port` non-zero. Each error names the flag and the accepted range.
- Duration fields on ops (`ms`, `timeout_ms`, `typewriter_ms`) accept any
  finite non-negative number rather than silently falling back to a default
  when handed a float.
- `wait` with a selector now tests rendered visibility rather than mere
  presence in the DOM, with `"visible": false` to opt out of that.
- The render intermediate lives in a unique per-run directory under the
  system temp directory rather than a fixed `.kaviri-tmp` beside the output, so
  concurrent takes into one directory no longer corrupt each other.

### Fixed

- Serve mode captured nothing between ops. The capture pump only ran inside an
  executing op, so every gap while the driving agent was thinking rendered as
  a freeze frame. Both serve readers now drive the pump on a clock.
- A failed op in record mode aborted the process and discarded every frame
  already captured. The take is now stopped, rendered and reported, and the
  exit code still says it was incomplete.
- A CDP error during `stop_recording` threw away a complete take. Teardown is
  best effort and the render runs whenever there are frames, with the CDP
  error surfaced as a warning on the result.
- A websocket or spool write error deleted the spool. The frames are kept, the
  path is printed, and the take can still be rendered.
- The frame spool was unbounded, so a long take filled the temp filesystem and
  died on ENOSPC. It is now capped and checked against free space, and hitting
  the cap stops the capture cleanly and renders what exists.
- `navigate` ignored both the CDP error and the load-event timeout, so a
  failed navigation filmed Chrome's error page and reported success.
- A selector matching a hidden or zero-area element clicked the page's
  top-left corner and reported success. `click` and `type` now error, and name
  the element that is on top when one is obscuring the target.
- A second `start_recording` kept the first take's marks, mistiming every zoom
  in the new take.
- `scroll` defaulted `y` to 0 on a typo, scrolling to the top of the page and
  reporting success.
- Only one screenshot is now outstanding at a time. Slow pages used to
  accumulate in-flight captures, which is what starved capture at `--scale 2`.
- The op clock is no longer the websocket read timeout, so `typewriter_ms`
  below about 35 is honoured instead of being rounded up to the socket's
  30 ms.
- The camera jumped about 433 px in a single frame at the end of every
  ease-in, because the pan was clocked from the start of the event rather than
  from where the ease-in lands.
- Typing pans were time-stretched and finished over a second after the typing,
  because the waypoint floor was coarser than the caret sampling interval.
- The zoom level was chosen from the target's height alone, so wide, short
  targets were cropped off on both sides.
- An interaction in the final 1.15 s of a take silently got no zoom. The tail
  is padded, and anything still dropped is named on stderr.
- The generated ffmpeg filter graph grew without bound and crossed the
  kernel's single-argument limit on long takes. Pan paths are decimated to a
  fixed budget, and a graph that still exceeds 32 KiB is passed to ffmpeg as a
  script file rather than as an argument.
- `--background none` stretched the content to the output aspect ratio instead
  of fitting and padding it.
- ffmpeg pass 2 inherited stdin and ate the op stream in `kaviri serve` stdin
  mode.
- The pass 1 ffmpeg child was abandoned unreaped when writing frames failed,
  and the real cause was reported as a bare "Broken pipe".
- The temp directory survived every render failure.
- Chromium and its profile directory were orphaned when the websocket connect
  failed during launch, and killing the browser missed the real process behind
  a snap wrapper script.
- `KAVIRI_FFMPEG` rejected a bare command name such as `ffmpeg7`, unlike
  `KAVIRI_CHROMIUM`, which accepts one.
- `content_box` underflowed on a zero-width or zero-height output.
- The spool file was not released after rendering, so `kaviri serve` held
  gigabytes of temp space while idle.
- `kaviri serve` exited 0 when every op had failed and no video was produced.

### Security

- `kaviri serve --port` was an unauthenticated remote-control socket. Any web
  page the user visited could reach it with a cross-origin `fetch` and drive
  the browser. It now requires a token, drops any connection whose first line
  is not JSON, and is documented as a remote-control socket in `SECURITY.md`.
- A script's `start_recording` path was an unvalidated arbitrary file write in
  serve mode, for both the MP4 and the telemetry sidecar.
- Temp paths were predictable and the spool's uniquifier was always zero, so
  the spool was effectively a fixed name in a world-writable directory opened
  with create-and-truncate. Both the spool and the browser profile now live in
  one per-run directory created mode 0700 with real entropy in the name, and
  the spool is created exclusively with mode 0600.
- The GitHub Action interpolated its inputs straight into shell, so a branch
  name or a PR title could execute code on the runner. Inputs are passed
  through the environment.
- The Action cloned an unpinned `main` from a remote repository instead of
  building the tree it was invoked from.

[Unreleased]: https://github.com/thisisisheanesu/kaviri/commits/main
