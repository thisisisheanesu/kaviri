# The script protocol

A kaviri script is newline-delimited JSON. One object per line, one op per object. The same
protocol is what `kaviri record --script` reads from a file, what `kaviri serve` reads from
stdin, and what `kaviri serve --port` reads from a socket. There is no other input format.

Everything on this page is taken from `src/ops.rs` (the ops), `src/main.rs` (the script
reader and the response envelope) and `src/cdp.rs` (what the waits actually wait on).

## The file

```jsonl
# kaviri demo: local page (no network), typing with typewriter effect
{"op":"start_recording"}
{"op":"navigate","url":"examples/demo.html"}
{"op":"wait","ms":600}
{"op":"click","selector":"#name"}
{"op":"type","selector":"#name","text":"Ada Lovelace","typewriter_ms":60}
{"op":"wait","ms":400}
{"op":"type","selector":"#email","text":"ada@analytical.engine","typewriter_ms":45}
{"op":"wait","ms":400}
{"op":"click","selector":"#submit"}
{"op":"wait","selector":"#done"}
{"op":"wait","ms":1500}
{"op":"stop_recording"}
```

That is `examples/form.jsonl`, unedited. It runs.

Reader rules, from `run_record` in `src/main.rs`:

- Each line is trimmed. A line that is empty, or begins with `#` or `//`, is skipped. That is
  not JSON, it is kaviri's own preprocessing, so a comment cannot appear at the end of a line
  with an op on it.
- A line that is not valid JSON aborts the whole run before the browser launches, reported as
  `examples/form.jsonl:7: <serde error>`. The line number is the physical line in the file,
  comments and blanks included.
- The whole file is parsed before anything is executed. A typo on the last line costs you
  nothing but the parse.
- `serve` mode applies the same comment and blank-line handling, so a script file can be piped
  straight into `kaviri serve` with no preprocessing.

Two rewrites happen to your script that are worth knowing about:

- **If the script contains no `start_recording`, one is injected before the first op.** Its
  path is `--out`.
- **`--out` overwrites the `path` on any `start_recording` in the script.** In `serve` mode a
  `path` that disagrees with `--out` is ignored with a line on stderr. A client-supplied path
  is an arbitrary file write, and the MP4 and its telemetry sidecar both land wherever it
  points, so the process that started kaviri decides where the output goes, not the script.

## The response envelope

Every op answers with exactly one JSON line on stdout.

```jsonc
{"ok":true,"result":{"event":"mark","t":3.14,"kind":"click","label":"#go","box":[40,120,180,44],"recording":true}}
{"ok":false,"error":"selector not found: #go"}
```

In `record` mode a failing op carries two extra fields so you can find it in the file without
counting:

```jsonc
{"ok":false,"error":"selector not found: #go","op":"click","index":8}
```

`index` is 1-based over the ops that survived the comment filter, not over file lines.

**A failing op ends the script but does not throw the take away.** Every frame captured so far
is already on disk. kaviri stops the script, sends `stop_recording` itself, renders what it
has, prints the error and exits 1. You get a partial video and a named failure rather than
nothing. In `serve` mode a failing op does not end the connection: the error goes back in band
and the next op runs, but the process exits non-zero at the end if any op failed.

### The mark object

Most ops return a mark. It is the same shape every time:

| field | meaning |
|---|---|
| `event` | always `"mark"` |
| `t` | seconds on the recording clock, 0.0 when not recording |
| `kind` | the op that produced it: `navigate`, `click`, `type`, `scroll`, `wait`, `mark` |
| `label` | op-specific text (see each op below) |
| `box` | `[x, y, w, h]` in CSS pixels, or `null` |
| `recording` | whether this mark was stored for the zoom planner |

A mark is only appended to the timeline if a recording is in flight. One returned with
`"recording": false` is informational and will not produce a zoom.

**Only `click` and `type` marks that carry a `box` produce camera movement.** `navigate`,
`scroll`, `wait` and `mark` all return `"box": null` by construction, so they are timeline
anchors for you and the telemetry sidecar, not zoom triggers. See [camera.md](camera.md).

## Durations

Every duration in a script (`wait` ms, `hover` ms, `typewriter_ms`, the built-in settles) is in
**page time**. Normally that is real time. Under `--slowmo k` the page's clock runs k times slower
and kaviri waits k times longer in real terms, so a script means the same video at any k. Only
the timeouts (`timeout_ms` on `navigate` and `wait`) stay in real time, because they guard
against a page that never loads, not against pacing.

`ms`, `timeout_ms` and `typewriter_ms` are all read by one function, `duration_ms`. The rules
are the same for all three:

- Absent, or `null`: the op's default.
- Any finite number that is at least zero: accepted, rounded to the nearest millisecond.
  `{"typewriter_ms": 62.5}` types at 63ms per character.
- Anything else, including a stringified number: an error naming the field, for example
  `timeout_ms must be a non-negative number of milliseconds, got "800"`.

The string case is the one that matters. A shell or a template that stringifies everything used
to have its duration silently replaced by the default, so a script asking for a three-minute
timeout got twenty seconds and looked like a flaky page.

---

## `start_recording`

```jsonc
{"op":"start_recording"}
{"op":"start_recording","path":"out.mp4"}
```

| field | type | default | notes |
|---|---|---|---|
| `path` | string | `--out`, else `kaviri-out.mp4` | overridden by `--out` in both modes |

Opens the frame spool, sets the recording clock to zero, clears any marks left from a previous
take, starts the capture path chosen by `--scale`, nudges a paint and waits 200ms so the first
frame lands promptly.

Returns `{"event":"recording_started"}`. This is the one op that does not return a mark.

Errors:

- `already recording; send stop_recording to finish the take in flight before starting another`.
  Starting over a live take used to discard its frames silently. The frames are the whole value
  of a recording, so the script is told and gets to decide.
- The spool refusals from `src/cdp.rs`: see [troubleshooting.md](troubleshooting.md#the-take-refuses-to-start).

## `navigate`

```jsonc
{"op":"navigate","url":"http://127.0.0.1:8099/"}
{"op":"navigate","url":"examples/demo.html"}
{"op":"navigate","url":"https://slow.example","timeout_ms":60000}
```

| field | type | default | notes |
|---|---|---|---|
| `url` | string | required | see URL handling below |
| `timeout_ms` | number | `25000` | budget for the load event |

**URL handling.** A value containing `://`, or beginning with `about:`, is used as written.
Anything else is treated as a filesystem path, canonicalized against kaviri's current working
directory, and turned into a `file://` URL. A relative path that does not exist fails before
navigation with `cannot resolve path examples/demo.html: No such file or directory`, which is a
better error than a Chrome error page filmed at full resolution.

**What it waits for.** `Page.navigate`, then `Page.loadEventFired` within the budget. If the
load event never arrives, `document.readyState` is checked: `complete` is a success, anything
else is an error. This is deliberate. A same-document navigation, a hash change or a pushState
route, never fires `load` because the document never changed, and treating that as a timeout
would break every single-page app script.

A `navigate` that Chromium reports as `net::ERR_ABORTED` is not an error. That is what it says
when the navigation turned into a download or was superseded by another one, and the page is
fine. Every other `errorText` fails the op, because a Chrome error page is not what the script
asked to film.

After the load, 350ms of settle time, then a mark with `kind: "navigate"`, `label` set to the
fully resolved URL and no box.

> The label is the resolved URL including any query string or token. That is why the telemetry
> sidecar is off by default. See the README's telemetry section before you upload one.

Errors:

- `cannot resolve path <p>: <io error>`
- `navigate failed: <errorText>: <url>`
- `navigate: no load event after 25.0s and the document is still "loading": <url>` followed by a
  line telling you to raise `timeout_ms`.

## `click`

```jsonc
{"op":"click","selector":"#go"}
{"op":"click","x":400,"y":300}
```

| field | type | default | notes |
|---|---|---|---|
| `selector` | string | none | a CSS selector, matched with `querySelector` |
| `x`, `y` | number | none | CSS pixels in the viewport, used when there is no selector |

One of the two is required: `click needs selector or x/y`.

**The selector path** does more than find a node.

1. `el.scrollIntoView({block:'center', inline:'center', behavior:'instant'})`, then 120ms for
   layout and scroll to settle. You do not need to scroll to a control before clicking it.
2. One round trip that fetches the bounding rect, the computed style, the pointer shape the OS
   would use, and what `elementFromPoint` finds at the box's centre.
3. Four refusals, in this order:
   - `selector not found: #go`
   - `selector matched a non-visible element (display:none or visibility:hidden): #go`
   - `selector matched a zero-size element, so there is no point to aim at: #go`
   - `selector #go is covered by <div#modal> at its centre point; close the overlay, or target
     the element that is actually on top`

   These exist because a node that matches a selector is not necessarily a thing that can be
   clicked. A hidden or zero-size element has a box of all zeros, and dispatching to its centre
   is a real click on whatever sits in the viewport's top-left corner, reported as a success.

4. The drawn pointer glides to the target over 500ms (the CSS transition on the injected
   cursor). **The 500ms happens whether or not the cursor is drawn**, because the pause before a
   click is part of the pacing the zoom is cut against.
5. The mark is emitted, then the mouse events, then 250ms of settle.

The mark is emitted *before* the click is dispatched, so the zoom is already arriving when the
thing happens rather than chasing it.

**The x/y path** dispatches at the point and synthesizes a 20x20 box centred on it, so a
coordinate click still earns a zoom. The pointer shape comes from `elementFromPoint`.

`label` is the selector, or the literal string `point` for a coordinate click.

## `hover`

```jsonc
{"op":"hover","selector":".card"}
{"op":"hover","selector":".card","at":[0.15,0.5],"ms":900}
{"op":"hover","x":400,"y":300}
```

| field | type | default | notes |
|---|---|---|---|
| `selector` | string | none | resolved exactly as `click` resolves it, with the same refusals |
| `x`, `y` | number | none | CSS pixels in the viewport, used when there is no selector |
| `at` | [number, number] | the centre | where in the element's box to aim, as fractions from its top-left, each 0..1 |
| `ms` | number | 500 | how long the pointer takes to get there |

The mouse travels from wherever it last was, sending a `mouseMoved` roughly every 33ms along an
eased path, and the drawn cursor moves with it. The path matters: one move at the target fires
`:hover` styles, but a page that tracks the pointer (a tilt, a parallax, eyes that follow it)
gets a single sample and jumps. A few hovers with different `at` values sweep the pointer across
one element.

A hover is a camera target like a click: the mark carries the element's box and the camera
frames it, centred unless the take also types (see camera.md, the left bias). No button is pressed. The mark is emitted before the move, then 150ms of settle.

## `type`

```jsonc
{"op":"type","selector":"#q","text":"14 Rue Lafayette, Paris"}
{"op":"type","selector":"#name","text":"Ada Lovelace","typewriter_ms":60}
{"op":"type","text":"and the rest of the sentence"}
{"op":"type","selector":"#chat","text":"line one\nline two"}
```

| field | type | default | notes |
|---|---|---|---|
| `text` | string | required | `\n` is dispatched as Enter, everything else as text |
| `selector` | string | none | focuses the field first, and is what earns the zoom |
| `typewriter_ms` | number | `18` | per character |

**18ms is the default on purpose.** 45ms per character is a person hunting for keys. An agent
does not hunt, and a viewer does not want to watch one. Raise it when the point of the shot is
that the words are read along with.

**With a selector**, the field is resolved exactly as `click` resolves one, with the same four
refusals, then clicked, then given 150ms before the first character. The field's box is the mark
box, so the op earns a zoom.

**Without a selector**, characters go to `document.activeElement`. kaviri first checks that
something editable is focused, and refuses otherwise:

```
type without a selector needs a focused editable element; click one first, or pass a selector
```

Editable means `contenteditable`, a `<textarea>`, or an `<input>` whose type is not one of
`button submit reset checkbox radio range color file image`. `<body>` and
`<html>` are explicitly not editable. Without this check, `Input.insertText` on a freshly
navigated page delivers to `<body>`, the characters go nowhere, no box is recorded so the op
earns no zoom, and the result JSON is indistinguishable from a take that worked.

A selectorless `type` contributes no bounding box, so it produces no zoom of its own. Use it to
continue typing into a field you already clicked.

### The caret pan

While typing, the camera follows the text caret, not the mouse pointer. kaviri records agents.
There is no hand on a mouse, and the drawn pointer is parked where the field was clicked and
stays there while a whole sentence is typed. The thing that moves, and the thing a viewer is
reading, is the caret.

The caret is measured **twice**: once before the first character, once after the last. Never
between keystrokes. Asking the page where the caret is costs a CDP round trip, and doing that
inside the typing loop put the round trip inside the typing rhythm. The words came out slower
than `typewriter_ms` asked for, and the camera moved in steps because the samples were as uneven
as the latency.

From those two measurements, if the typing took more than 0.2s and the caret moved more than
1px, kaviri lays down evenly spaced synthetic marks across the interval it actually took:
`round(span / 0.12)` of them, clamped to between 2 and 40. Each carries a 2px-wide box at the
interpolated caret x, with the field's own y and height, so following the caret does not tighten
the zoom onto a single line of text.

Measurement works through the selection for `contenteditable`, and for `input` and `textarea` by
mirroring the text up to the caret into a hidden element with the same typography and reading
where it ends. If nothing is focused it returns null and no pan is laid down; the typing still
happens.

## `press`

```jsonc
{"op":"press","key":"Enter"}
{"op":"press","key":"ArrowDown","repeat":4,"interval_ms":150}
{"op":"press","key":"Meta+Shift+P"}
{"op":"press","key":"?"}
{"op":"press","key":"Shift","hold_ms":1200}
{"op":"press","key":"Escape","selector":"#search"}
```

| field | type | default | notes |
|---|---|---|---|
| `key` | string | none | one key, or a chord of modifiers and a key joined by `+` |
| `repeat` | integer | 1 | how many times to press it, 1..200 |
| `interval_ms` | number | 120 | the pause between repeats |
| `hold_ms` | number | 0 | how long the key stays down before it comes up |
| `selector` | string | none | focus this element first (no click, no pointer move) |

A key is a single character (`a`, `K`, `7`, `/`, `?`) or a name: `Enter`, `Tab`, `Escape`,
`Backspace`, `Delete`, `Space`, `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` (or `Up`,
`Down`, ...), `Home`, `End`, `PageUp`, `PageDown`, `Insert`, `F1` to `F24`, or a modifier on its
own. Modifiers are `Shift`, `Control` (`Ctrl`), `Alt` (`Option`) and `Meta` (`Cmd`), in any
case. A literal plus is the last part: `"+"` or `"Control++"`.

Each press is what a keyboard sends: a keydown for every modifier in order, the key down, the
key up, and the modifiers up in reverse, all through `Input.dispatchKeyEvent`, so page
listeners see real `keydown` / `keyup` events with `key`, `code`, `shiftKey`, `metaKey` and
the rest set. The layout is US: a capital letter or a shifted symbol (`?`, `!`, `+`) holds
Shift for you, so a handler bound to `?` sees `shiftKey` true, as it would from a person.
In a chord a letter's case does not matter: `Meta+L` is Command-L, and Shift is held only
when the chord names it (`Meta+Shift+L`). `repeat`, `interval_ms` and `hold_ms` are paced
by the clock in page time, so they hold under `--slowmo` and on a busy machine.
A printable key with no Control or Meta also inserts its character into a focused field; a
chord with either one is a shortcut and inserts nothing.

`press` is not a replacement for `type`. It exists for apps driven from the keyboard (command
palettes, Vim bindings, games, list navigation), where the key is the interaction. For text,
`type` is faster and follows the caret with the camera.

A press has no box, so it does not move the camera by itself. Put a `hover` or `click` on the
thing the keys act on when the take needs to be looking at it. `label` is the key as written.

## `scroll`

```jsonc
{"op":"scroll","y":600}
{"op":"scroll","y":0,"smooth":false}
```

| field | type | default | notes |
|---|---|---|---|
| `y` | number | required | absolute document offset in CSS pixels, not a delta |
| `smooth` | bool | `true` | `true` waits 800ms, `false` waits 120ms |

There is no default `y`, and that is the surprising part. `{"op":"scroll","top":600}` and a `y`
that arrived as the string `"600"` both used to scroll the page to the top and report success,
which looks exactly like a scroll that worked. Now:

```
scroll needs y: a number of CSS pixels, absolute from the top of the document
scroll y must be a finite, non-negative number of CSS pixels, got -40
```

The mark's label is `y=600`. There is no box, so a scroll produces no zoom. Zoom comes from what
you interact with after the scroll.

## `wait`

```jsonc
{"op":"wait","ms":800}
{"op":"wait","selector":".loaded"}
{"op":"wait","selector":"#q","timeout_ms":30000}
{"op":"wait","selector":".done","visible":false}
```

| field | type | default | notes |
|---|---|---|---|
| `ms` | number | none | a fixed pause |
| `selector` | string | none | poll until satisfied |
| `timeout_ms` | number | `20000` | selector waits only |
| `visible` | bool | `true` | selector waits only |

**`ms` and `selector` together is an error**, not a combination:

```
wait takes ms or selector, not both; use timeout_ms to bound a selector wait
```

The natural reading of both together is "wait for this, but at most that long", which is what
`timeout_ms` is for. The old code took the `ms` branch and never looked at the selector, so the
script raced ahead of the page it meant to wait for. Neither field at all is
`wait needs ms or selector`.

**A selector wait checks visibility by default.** The probe is
`e.getClientRects().length && getComputedStyle(e).visibility !== 'hidden'`. Existence is not
what a script means by "wait for the success message": a `display:none` node is in the DOM from
first paint, so a presence-only wait returned on its first poll. Pass `"visible": false` when you
genuinely mean presence.

Polling is every 100ms, and the frame pump runs throughout, so a long wait is filmed rather than
frozen. On timeout:

```
wait: selector never became visible after 20.0s: .done
  raise it with {"op":"wait","selector":"...","timeout_ms":60000}, or add "visible": false to wait for presence only
```

The elapsed time in that message is measured, not the budget, so a wait cut short by something
else is visible as such.

20 seconds is fine for a page to render something and far too short for anything that has to
think first. A take that waits on a model finishing its answer should say how long it is
prepared to wait.

## `mark`

```jsonc
{"op":"mark","label":"checkout"}
```

| field | type | default |
|---|---|---|
| `label` | string | `""` |

A named point on the recording clock. No box, so no zoom. It exists for the telemetry sidecar
and for you, when you are working out which second of a two-minute take to look at.

## `stop_recording`

```jsonc
{"op":"stop_recording"}
```

No fields. This is the op that renders, and on a long take it is the op that takes a while: the
CFR normalize pass, the backdrop probe and plate, and the zoompan pass all happen here. In
`serve` mode it holds the session mutex for the whole render, so other clients wait out the
encode.

Teardown is not allowed to lose the take. Every frame is already on disk by the time this runs,
so a browser that died at second 55 of a sixty second recording still renders, and the CDP
failure is reported alongside the video rather than instead of it, as a `warning` field.

On success:

```jsonc
{"ok":true,"result":{
  "event":"recording_rendered",
  "path":"demo.mp4",
  "duration":31.4,
  "zoom_events":6,
  "frames":842,
  "spooled_bytes":14238711
}}
```

`duration` is the rendered length in seconds, which includes any tail padding the last
interaction needed (see [camera.md](camera.md#the-tail)). `zoom_events` is how many camera moves
the planner produced. **A take with `"zoom_events": 0` rendered a video with no camera movement
at all**, which usually means no `click` or `type` op ever resolved a box.

The one hard failure:

```
stop_recording: no frames were captured; was start_recording sent?
```

or, when capture had already been abandoned:

```
stop_recording: no frames were captured (ws read: <error>)
```

Both leave you with no file. See
[troubleshooting.md](troubleshooting.md#a-take-that-films-nothing).

If the script never sends `stop_recording`, both modes send one themselves: `record` mode after
the last op or the first failure, `serve` mode at EOF or on a signal.

## `hello`

```jsonc
{"op":"hello","token":"…"}
```

Only meaningful on a `--port` connection, where it must be the first line. The token is printed
on stderr at startup, or set with `KAVIRI_TOKEN`. An empty token is never accepted, whatever the
client claims. Answers
`{"ok":true,"result":{"event":"hello","version":"0.1.0"}}`.

A `--port` connection is also strict about JSON: one unparseable line drops the connection
rather than being partly executed, so a stray HTTP request from a page the filmed browser
happens to be visiting cannot half-run a script. On stdin, where the channel is already private,
a bad line is reported and skipped.

Anything else is `unknown op: <name>`, and an object with no `op` key is `missing "op" field`.

## A script that runs, end to end

This is `examples/demo.jsonl`, which is what `.github/workflows/demo.yml` records on every push
that touches `src/`. It is worth reading as a shape rather than as an example of syntax.

```jsonl
{"op":"navigate","url":"http://127.0.0.1:8099/"}
{"op":"wait","selector":"#q","timeout_ms":30000}
{"op":"start_recording"}
{"op":"wait","ms":2200}
{"op":"mark","label":"start"}
{"op":"type","selector":"#q","text":"14 Rue Lafayette, Paris"}
{"op":"wait","ms":500}
{"op":"click","selector":"#go"}
{"op":"wait","selector":"li","timeout_ms":15000}
{"op":"wait","ms":1400}
{"op":"mark","label":"tracked"}
{"op":"type","selector":"#q","text":"Kigali Heights, KG 7 Ave"}
{"op":"wait","ms":500}
{"op":"click","selector":"#go"}
{"op":"wait","ms":1800}
{"op":"mark","label":"second"}
{"op":"wait","ms":1600}
{"op":"stop_recording"}
```

Four things it does deliberately:

- **Navigate and wait for the app before `start_recording`.** Cold start, a dev server warming
  up and a first paint are not part of the demo, and filming them costs spool space for footage
  you will cut.
- **A 2200ms hold after `start_recording`.** The first interaction's zoom begins easing in
  `LEAD_IN` (0.45s) before the mark, so a take that opens on a click has nowhere to ease from.
- **A `wait` on a selector after the click, then a fixed `wait`.** The selector wait is
  correctness, the fixed wait is pacing: it gives the result a beat on screen.
- **A 1600ms tail before `stop_recording`.** A zoom needs roughly `HOLD_AFTER + EASE` of footage
  after its mark. Without the tail the last interaction, which is usually the thing the demo is
  about, loses its zoom. kaviri warns on stderr when a mark lands too late, and pads the tail up
  to 2.8s to rescue it, but padding holds a frozen frame and a real wait films the real page.
