# kaviri for agents

You are an AI agent and something has asked you for a video of a web app. This file is the
whole of what you need. It assumes you can run a command and write a file, and nothing else.

kaviri is a recording browser. You do not describe a camera move, you describe what happens,
and the camera is derived from that. There is no screen recorder involved, no window to keep
in focus, no display server, no Wayland portal and no permission dialog. It works the same on
your laptop and on a CI runner with no screen at all.

## The one paragraph version

Write a `.jsonl` file, one JSON object per line, each one an op. Run
`kaviri record --script take.jsonl --out take.mp4`. You get an MP4. Every op answers on stdout
with `{"ok":true,"result":{…}}` or `{"ok":false,"error":"…"}`, and the process exits non-zero
if any op failed, so you can tell success from failure without watching the video.

## The ops

```jsonl
{"op":"navigate","url":"http://127.0.0.1:8099/"}
{"op":"wait","selector":"#app","timeout_ms":30000}
{"op":"start_recording"}
{"op":"wait","ms":800}
{"op":"hover","selector":"#plans","ms":700}
{"op":"press","key":"Meta+K"}
{"op":"click","selector":"#new-invoice"}
{"op":"type","selector":"#amount","text":"1450.00"}
{"op":"scroll","y":600,"smooth":true}
{"op":"mark","label":"submitted"}
{"op":"wait","ms":1600}
{"op":"stop_recording"}
```

`click` also takes `x` and `y` instead of a selector, and so does `hover`, which moves the
mouse along a path (so pointer-tracking effects follow it) and is framed by the camera like a
click; `at: [fx, fy]` aims it at a fraction of the element instead of its centre. `type` takes `typewriter_ms` to set the
per character delay; the default of 18 is deliberately fast, because a demo of someone typing
slowly is a demo of someone typing slowly. `press` sends a real key or chord (`"Enter"`, `"ArrowDown"`, `"?"`, `"Meta+Shift+P"`) to
whatever has focus, with `repeat`, `interval_ms` and `hold_ms`, for apps driven from the keyboard.
`navigate` turns a bare path into a `file://` URL.
`mark` puts a label in the telemetry and nothing on screen.

The full reference, including every field and what each one does to the camera, is
`docs/script-protocol.md`. How the camera decides what to do is `docs/camera.md`.

## The seven things agents get wrong

**1. Recording before the page is ready.** `start_recording` after the `wait` that proves the
app is up, not before. Otherwise the first two seconds of your video are a blank page, and the
zoom on the first interaction fires while the layout is still moving.

**2. No trailing wait.** An interaction needs about two seconds after it to be zoomed at all,
and a script that ends `{"op":"click"},{"op":"stop_recording"}` throws away the zoom on the one
thing it was demonstrating. kaviri says so on stderr rather than silently:
`the interaction at 28.4s is too close to the end of the 29.0s take`. End with a `wait` of at
least 1600ms.

**3. Assuming a selector resolved.** If a modal, a cookie banner or a loading overlay is over
your target, kaviri refuses rather than clicking through it:
`selector #q is covered by <div#loader> at its centre point`. That message is telling you the
page is not in the state you think it is. Dismiss the overlay in the script.

**4. Not serving the app.** `navigate` does not start anything. In CI, start the server and
poll it until it answers before the recorder runs. `docs/ci.md` has the step.

**5. One video per step.** If you are driving a long session, use `kaviri serve`, which holds
one browser open across many ops and produces one take. Spawning `record` per step gives you a
folder of two-second clips.

**6. Shipping a choppy take.** If stderr says `the browser produced N frames a second`, the
video will stutter, and a canvas-heavy page (a game, a chart, a generative comic) usually does:
headless Chromium paints it in software at 5 to 10 frames a second. Record again with
`--slowmo <k>`, where k is roughly 30 divided by N (8 is a good start for a heavy canvas). The
page's clock runs k times slower while kaviri films it and the frames are stamped in page time,
so the video plays at normal speed with k times the real frames. Script timings stay exactly as
written. It costs k times the take's length in wall time, so run long takes detached. Prefer it
to `--smooth on`, which invents frames by interpolation and smears fast motion.

**7. Expecting the camera to lean left.** It only does in a take that types. The lean exists
so the text already written stays in shot, and it applies to every click in a take that has a
`type` op. A take with no text entry centres every click and hover on its target. If your
clicks look off-centre, check whether the script types somewhere.

## Serve mode, which is the one built for you

```
kaviri serve
```

NDJSON ops on stdin, one JSON result line per op on stdout. The browser stays open between
ops, so you can interleave kaviri ops with your own reasoning, read each result, and decide
the next op from what actually happened. Send `{"op":"start_recording"}` when the part worth
filming begins and `{"op":"stop_recording"}` when it ends; everything before and after still
drives the browser, it is just not in the video.

`kaviri serve --port <n>` exists and you should think before using it. The op set can navigate
to `file://` and write an MP4 to any path, and any web page the user visits can `fetch()` a
loopback port. The first line of a connection must be `{"op":"hello","token":"…"}` with the
token printed on stderr at startup, and that is the only thing standing between a visited page
and your filesystem. Prefer stdin, where the ops come from the process you started.

## Reading the result

```json
{"ok":true,"result":{"duration":14.63,"event":"recording_rendered","frames":95,
                     "path":"take.mp4","zoom_events":1}}
```

`zoom_events: 0` means no `click` or `type` ever resolved a bounding box, so the take is a flat
screen recording with no camera work at all. That is almost always a broken script rather than
a choice, and it is the single most useful thing to assert on.

## Framing it as a device

Declare the platform in the script, as its first line, so the file carries it:

```jsonl
{"op":"frame","platform":"ios"}
{"op":"frame","platform":"ios","style":"recording"}
{"op":"frame","platform":"macos","desktop":"on"}
```

`platform` is `macos`, `windows`, `linux`, `ios`, `android`, `android-emulator` or
`ios-simulator`; every `--frame*` flag has a field of the same meaning, listed in
`docs/script-protocol.md`. The flags still work and win over the op. The take is filmed inside
that system's window or handset, with the page's real title, favicon and URL in the chrome.
A phone platform also switches the browser to that phone's viewport and user agent, so write
the script against the phone layout, and it draws a tap dot instead of a mouse pointer.
`"style":"recording"` on `ios` or `android` makes the take look like the phone's own screen
recording: the screen edge to edge, no bezel, the red recording indicator in the status bar.
Under a frame the zoom takes in the whole screen (chrome, dock and wallpaper zoom with the page)
and anchors on the card a control sits on, so several ops on one card hold the camera still.
Keep a card's interactions together in the script and the take stays steady. `--desktop on` adds the menu bar and dock or the
taskbar; `--dock on` draws the dock without the menu bar, `--dock-position` puts it on a side, and
`--icon-set` restyles the icons; `kaviri frames` lists the frames, icons, dock groups and
icon sets. The chrome is read off the page when `stop_recording` runs, so stop on the page you
want the address bar to show.

## Making a video that is not a recording

If you were asked for a launch video, a showreel or an ad rather than a take of an app, use
`kaviri motion`. For a standard product video, read `docs/motion-simple.md`
(https://kaviri.dev/motion-simple.txt): a `brand` line, one `beat` line per idea and an `end`
line, and kaviri does the rest, music included. For full control, read `docs/motion-llm.md`
(or https://kaviri.dev/motion-llm.txt): the whole format in one file. Then work in a loop: write the `.jsonl`, run
`kaviri motion --script reel.jsonl --check`, render the frames that matter with
`--still 1b,2bar,4bar+2b --out look.png` and look at them, fix what is wrong, and only then
render with `--out reel.mp4`. The checker names the line and the real options for every
misspelled effect, ease, kind or target, so read its error and correct the script.

## What it will not do

It will not record a native application, a terminal, or anything outside the browser it starts.
It will not record audio yet. It will not find the interesting part of your app for you: the
script is the storyboard, and a bad storyboard films perfectly.

## Determinism

The same script against the same app gives the same take, within the noise of how fast the page
renders. That is the reason for the whole design: a video that can be regenerated in CI is a
video that is never out of date, and one that has to be re-recorded by a human never gets
re-recorded.
