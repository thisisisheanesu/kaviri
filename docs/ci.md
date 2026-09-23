# Recording in CI

The point of kaviri is that the demo video is a build artifact. The script lives in the repo
next to the code it films, CI re-records it when the product changes, and a take that filmed
nothing turns the build red.

Two files in this repo do that work, and they are not the same thing:

- `action.yml` is the composite action. It installs the tools, builds kaviri and runs the take.
  It fails if the recorder fails.
- `.github/workflows/demo.yml` is kaviri recording itself, and it is where the checks on the
  finished video live: a duration gate and a first-frame-to-last-frame PSNR comparison.

**The action does not check your video.** It checks that the recorder exited zero and that the
file exists. If you want the "the take filmed nothing" failure, copy the two check steps out of
`demo.yml` into your own workflow. They are ten lines and they are reproduced in full below.

## Using the action

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
    preset: readme
```

### Inputs

| input | default | what it is |
|---|---|---|
| `script` | required | path to the `.jsonl` script of ops |
| `out` | `demo.mp4` | where to write the MP4 |
| `preset` | `desktop` | `desktop`, `tiktok`, `reels`, `shorts`, `square`, `landscape`, `readme`, `phone` |
| `background` | `auto` | a background name, `auto`, or `none` |
| `scale` | `2` | capture supersampling |
| `chromium-args` | empty | extra Chromium flags, appended to the ones the action already sets |
| `upload` | `true` | upload the MP4 as a workflow artifact |
| `artifact-name` | `demo` | name for that artifact |

One output, `video`, which is whatever `out` was.

There is no `ref` input. There used to be, and it made the action clone the remote and film a
different commit than the one under review. A composite action already has its source tree on
disk, so it builds from `github.action_path` and nothing else.

### What it does, in order

1. **Install ffmpeg and a browser.** Looks for `google-chrome`, `google-chrome-stable`,
   `chromium`, `chromium-browser` on PATH. On Linux, installs what is missing through apt,
   taking `sudo` if it is not already root. On macOS, `brew install ffmpeg` and Chrome from
   `/Applications` if it is there. Any other `RUNNER_OS` exits with a message rather than
   guessing: **Linux and macOS runners only.**
2. **Pin `KAVIRI_CHROMIUM` to the absolute path it found**, unless you already set it. This
   matters on Ubuntu, where the `chromium` package is often a transitional stub for the snap. A
   snap-confined browser cannot read a profile or a frame spool under `RUNNER_TEMP`, and it
   fails by never opening the debugger port, which surfaces forty seconds later as an error
   naming neither snap nor the profile.
3. **Set `CARGO_HOME`.** The official Rust images put cargo somewhere other than `$HOME`, and a
   cache keyed on a literal `~/.cargo` silently stores nothing.
4. **Install a stable Rust toolchain** rather than assuming one is present.
5. **Restore the build cache**, keyed `kaviri-<os>-<arch>-<sha>` with a prefix restore key. The
   key carries the commit on purpose: a GitHub cache entry is immutable once written, so a
   constant key freezes the first build's `target/` forever and every later build creeps back
   towards a cold one.
6. **`cargo build --release --locked --quiet`** in the action's own checkout.
7. **Record.**
8. **Upload**, with `if-no-files-found: error`.

### The two environment variables the action sets that you should not set locally

```yaml
KAVIRI_CHROMIUM_ARGS: --no-sandbox --disable-dev-shm-usage ${{ inputs.chromium-args }}
TMPDIR: ${{ runner.temp }}
```

`--no-sandbox` is a real reduction in isolation. kaviri never passes it on its own; the runner
opts in, because the runner is the thing that owns the container. `--disable-dev-shm-usage`
moves Chromium's shared memory off `/dev/shm`, which is 64MB in most containers and is the
single most common reason a browser dies during startup in CI.

`TMPDIR` moves the frame spool and the CFR intermediate off `/tmp`, which on a hosted runner is
small and shared. At `--scale 2` a busy take writes on the order of 15 to 25 MB/s.

Note that `chromium-args` adds to those two rather than replacing them.

### Everything caller-controlled is passed as an environment variable

```yaml
env:
  SCRIPT: ${{ inputs.script }}
  OUT: ${{ inputs.out }}
run: |
  "$ACTION_PATH/target/release/kaviri" record --script "$SCRIPT" --out "$OUT" …
```

Interpolating `${{ }}` into a `run:` body substitutes it before bash parses the line, so quoting
there buys nothing and a branch name or a pull request title can execute commands. Through
`env` they are only ever strings. Do the same in your own workflow, including for matrix
values.

## The failure checks

`demo.yml` runs two checks after the action. They are the differentiator, so here they are in
full, and here is what each one catches that the other does not.

### Check one: is it a real video

```yaml
- name: Check it is a real video
  env:
    OUT: ${{ matrix.out }}
  run: |
    set -euo pipefail
    dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 -i "$OUT")
    echo "duration: ${dur}s"
    awk -v d="$dur" 'BEGIN { exit (d > 2.0) ? 0 : 1 }' \
      || { echo "recording is too short to be real"; exit 1; }
```

This catches a container with no muxed video, a take that captured two frames, and a render that
produced a file before falling over. It does not catch much else, because kaviri already
refuses to render zero frames.

### Check two: did the page actually move

```yaml
- name: Check the page actually moved
  env:
    OUT: ${{ matrix.out }}
  run: |
    set -euo pipefail
    ffmpeg -nostdin -v error -y -ss 0.5     -i "$OUT" -frames:v 1 "$RUNNER_TEMP/first.png"
    ffmpeg -nostdin -v error -y -sseof -0.5 -i "$OUT" -frames:v 1 "$RUNNER_TEMP/last.png"
    ffmpeg -nostdin -v error \
      -i "$RUNNER_TEMP/first.png" -i "$RUNNER_TEMP/last.png" \
      -lavfi "psnr=stats_file=$RUNNER_TEMP/psnr.log" -f null -
    psnr=$(sed -n 's/.*psnr_avg:\([^ ]*\).*/\1/p' "$RUNNER_TEMP/psnr.log" | head -1)
    echo "first-to-last psnr: ${psnr:-none}"
    if [ -z "$psnr" ] || [ "$psnr" = "inf" ]; then
      echo "the first and last frames are identical; the take filmed nothing"
      exit 1
    fi
    awk -v p="$psnr" 'BEGIN { exit (p + 0 < 50.0) ? 0 : 1 }' \
      || { echo "the first and last frames are the same picture; the take filmed nothing"; exit 1; }
```

**This is the check that earns its place.** The duration gate passes on a take that filmed a
Chrome error page, or a dev server that never came up, or a page stuck behind a spinner, because
a freeze frame held for thirty seconds is still thirty seconds of video. Comparing the first
frame against the last one catches all three: a demo script that navigates, clicks and types
cannot end on the same picture it started on.

Three details in there are load-bearing:

- **`-ss 0.5` and `-sseof -0.5`**, not the literal first and last frames. The very first frame
  can be a partial paint and the very last is often the held tail frame; half a second in from
  each end is representative.
- **`psnr_avg` is the literal string `inf` for two identical frames.** `awk` coerces that to a
  number inconsistently between implementations, so that case is decided in the shell before
  the expression ever sees it.
- **The threshold is 50dB.** Above roughly 50dB two frames are indistinguishable by eye. For a
  demo that navigates, clicks and types, indistinguishable means nothing happened.

An empty `psnr` (the `sed` found nothing) is also a failure. A silently missing measurement is
how a check stops checking.

### What a broken take looks like in the log

| symptom | what happened |
|---|---|
| `kaviri: error: cannot read demos/checkout.jsonl: No such file or directory` | `script` input is relative to the workspace, not to the action |
| `kaviri: error: ffmpeg not found. Looked in: …` | the install step did not run, or apt is unavailable in your container |
| `chromium did not expose a debugger endpoint in 40s` | almost always `/dev/shm`; see below |
| `navigate failed: net::ERR_CONNECTION_REFUSED: http://127.0.0.1:8099/` | the app server step did not wait for the port |
| `wait: selector never became visible after 20.0s: #q` | the app came up but the page did not render, or the runner is too loaded |
| duration check passes, PSNR check says `inf` | the browser filmed one static picture for the whole take |
| `zoom_events: 0` in the `stop_recording` result | no `click` or `type` op ever resolved a box |
| `kaviri: the interaction at 28.4s is too close to the end of the 29.0s take` | the script needs a longer `wait` before `stop_recording` |

## Debugging a take you cannot reproduce locally

The take happened on someone else's machine and the frames are gone. Three things bring them
back.

**1. `kaviri doctor`.** It prints the resolved Chromium and ffmpeg paths with their version
strings, where the frame spool will live and what the spool cap is, and exits non-zero if either
binary is missing. Half the "it works on my machine" questions are answered by that output.

The action does not run it for you and does not put the binary on PATH, so to get it in a job
you build kaviri yourself:

```yaml
- run: cargo install --git https://github.com/thisisisheanesu/kaviri --locked kaviri
- run: kaviri doctor
```

Locally it is the first thing to run when a take will not start.

**2. `KAVIRI_KEEP_TEMP=1`.** This keeps the frame spool, the CFR intermediate and the generated
filter graph, and writes the telemetry sidecar to `<out>.telemetry.json`. Upload the sidecar and
you can read every mark, its timestamp and its box, and every zoom event the planner produced,
without the video.

> The sidecar's `marks` array carries every `navigate` label verbatim: full URLs, query strings
> and any token in them. Read it before you upload it, and do not attach it to a public build.

**3. `KAVIRI_DEBUG=1`.** Logs the ffmpeg path it resolved, each failing `/json/version` poll
while waiting for the browser, and any `captureScreenshot` error the pump swallowed. This is the
one to reach for when the browser never comes up.

The stderr from a normal run is already fairly informative, and none of it is hidden behind a
flag:

```
kaviri 0.1.0: launching browser (1100x620@2x) ...
kaviri: captured 842 frames (13.6 MB spooled), rendering demo.mp4 ...
kaviri: viewport 1100x620, capture 2200x1240 (2.00x), output 1100x620
kaviri: backdrop tide (auto, content hue 34° / lightness 0.91), content 986x556 at 56,32
kaviri: done: demo.mp4 (31.4s, 6 zoom events)
```

If the render fails, the generated filter graph is written to disk and its path is in the error
message, and the temp directory is retained rather than cleaned up. Machine-generated
expressions are the thing most likely to be rejected by ffmpeg, and until that change the only
report was "pass 2 failed" with the expression that failed existing nowhere at all.

## Pinning, and what it does not buy you

Pinning the action to a tag fixes kaviri's behaviour, not its output. Two runs of the same script
on the same commit are not frame-identical: marks and frame times come from a wall clock, and
the capture cadence adapts to machine load, so durations, waypoint spacing and the exact pixels
all move a little run to run. The PSNR check is written with that in mind, which is why its
threshold is "did anything change at all" rather than a similarity budget.

What pinning buys you is that the framing rules, the zoom ladder and the backgrounds stay put.
If you need a byte-stable asset, render once and commit the MP4.

## Notes for self-hosted and container runners

- The action needs root or `sudo` on Linux to install anything. If it has neither it says so and
  exits, rather than half-installing. Pre-install `ffmpeg` and a Chromium in your image and the
  install step becomes a no-op.
- `--disable-dev-shm-usage` is already passed. If the browser still dies during startup, raise
  `/dev/shm` for the container.
- Do not use `kaviri serve --port` in CI unless you mean it. It opens a local control socket, and
  the op set can navigate to `file://` URLs and write an MP4 anywhere the process can write.
  `record` mode, or `serve` on stdin, is what a CI job wants.
- A recording needs an uncontended core. The default capture path is a screenshot pump running
  inside the same browser that is running the app, so a two-core runner films a visibly slower
  app than a human sees, and a `wait` that is comfortable by hand can time out. `scale: 1`
  switches to the push-based screencast and takes kaviri almost entirely out of the app's way,
  at the cost of supersampling.

## The other workflow

`.github/workflows/ci.yml` is the one that compiles the crate: `cargo fmt --check`, `cargo
clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, then a release build. It
exists because `demo.yml` only exercises the happy path through a release build, so a type error
or a failing unit test could otherwise reach main without a single red check.

It installs ffmpeg deliberately and leaves `KAVIRI_SKIP_FFMPEG_TESTS` unset. Several tests shell
out to a real ffmpeg to validate the generated filter graph and the hand-rolled PNG encoder, and
they used to skip themselves silently when ffmpeg was missing, so CI reported a pass for tests
that never ran. Now a missing encoder is a panic with the reason, and skipping has to be asked
for by name.
