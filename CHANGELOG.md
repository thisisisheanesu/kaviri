# Changelog

All notable changes to lensa are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and lensa aims at [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
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

### Added

- `lensa doctor`, which prints the Chromium and ffmpeg lensa resolved, and
  where it found them, without launching a take.
- `lensa --version` / `-V`, and the version in the startup log line, so a
  rendered take can be traced back to the build that produced it.
- Preflight discovery of ffmpeg before the browser launches, so a missing
  encoder costs a second rather than the whole recording.
- SIGINT and SIGTERM handling. The first signal unwinds through the normal
  teardown so the browser, its profile and the frame spool are cleaned up; a
  second signal exits immediately.
- `--spool-dir` and `--max-spool-bytes`, plus `LENSA_SPOOL_DIR` and
  `LENSA_MAX_SPOOL_BYTES`, to place and bound the frame spool. The spool
  defaults to a cap of 8 GiB and refuses to start a take with less than
  512 MiB free, naming the directory in the error.
- A token handshake on `lensa serve --port`. The token is printed on stderr at
  startup and must arrive as the first line of every connection.
- `timeout_ms` on the `navigate` op, matching `wait`.
- `LICENSE` (a placeholder pending the licence decision), `THIRD-PARTY.md`,
  `SECURITY.md` and this file.

### Changed

- Record mode now emits the same `{"ok":…}` response envelope as serve mode,
  including for the op that fails. Callers that parsed the bare mark object
  from record mode need to read `result`.
- `--help`, `lensa presets` and `lensa backgrounds` print to stdout and exit 0
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
  system temp directory rather than a fixed `.lensa-tmp` beside the output, so
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
- ffmpeg pass 2 inherited stdin and ate the op stream in `lensa serve` stdin
  mode.
- The pass 1 ffmpeg child was abandoned unreaped when writing frames failed,
  and the real cause was reported as a bare "Broken pipe".
- The temp directory survived every render failure.
- Chromium and its profile directory were orphaned when the websocket connect
  failed during launch, and killing the browser missed the real process behind
  a snap wrapper script.
- `LENSA_FFMPEG` rejected a bare command name such as `ffmpeg7`, unlike
  `LENSA_CHROMIUM`, which accepts one.
- `content_box` underflowed on a zero-width or zero-height output.
- The spool file was not released after rendering, so `lensa serve` held
  gigabytes of temp space while idle.
- `lensa serve` exited 0 when every op had failed and no video was produced.

### Security

- `lensa serve --port` was an unauthenticated remote-control socket. Any web
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

[Unreleased]: https://github.com/OWNER/lensa/commits/main
