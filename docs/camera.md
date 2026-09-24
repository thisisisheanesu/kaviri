# The camera

kaviri's camera is not a camera. It is a crop rectangle over a fixed-size recording, expressed as
three ffmpeg expressions (`z`, `cx`, `cy`) evaluated per frame by a single `zoompan` filter. But
it behaves like a camera, and the decisions that make it behave like one are the subject of this
page.

Everything here is in `src/zoom.rs`, with the caret sampling in `src/ops.rs`. Constant names in
this document are the constant names in the source.

## What the camera moves for

Nothing except interactions. `events_from_marks` filters the mark timeline down to marks whose
`kind` is `click`, `hover` or `type` **and** which carry a bounding box. Everything else, `navigate`,
`scroll`, `wait`, `mark`, is on the timeline for the telemetry sidecar and does not move the
camera.

This is the part of the design most worth arguing with, so here is the argument. A screen
recorder that zooms on scroll or on page load is guessing at what matters. kaviri is not
guessing: the script told it what it was interacting with, and the page told it exactly where
that thing is in CSS pixels. A zoom is only ever cut to something the recording knows the
geometry of. A take of a page that is merely scrolled past is a take with no zooms in it, and
that is the honest result.

There is one synthetic exception, and it is still an interaction: while typing, kaviri lays down
extra `type` marks that track the caret. See [the caret pan](#the-caret-pan).

## The zoom level

Each target starts on a three-rung ladder read off the target's height in source pixels, against
the frame height:

| target height | zoom |
|---|---|
| more than 45% of the frame | 1.5 |
| more than 25% of the frame | 1.7 |
| otherwise | 1.85 |

Bigger things get gentler zooms, because a zoom is for making a small thing legible and a big
thing is already legible.

The ladder reads height only, which was a bug for a long time. A wide, short element, a nav bar,
a table row, a full-width button, took the tightest rung on the ladder and then hung off both
sides of the crop. So the ladder is now only the first of three candidates:

```rust
let fit_w = frame_w / (w * scale * FIT_MARGIN);
let fit_h = frame_h / (h * scale * FIT_MARGIN);
let z = ladder.min(fit_w).min(fit_h).max(1.0);
```

`FIT_MARGIN` is 1.15, so the target is fitted with about 15% of breathing room rather than
filling the crop exactly. Whichever axis runs out first wins. A target wider than the frame comes
out at `z = 1.0` and gets no zoom at all, which is the honest answer rather than a crop that
cuts it in half.

`scale` here is source pixels per CSS pixel, derived at render time from the actual JPEG header
of the first captured frame divided by the CSS viewport width. It is not `--scale` read back; it
is measured.

## The left bias

Centring on the thing being interacted with is the obvious choice and the wrong one.

A control sits to the **right** of whatever it acts on. The send button is after the message.
The search button is after the query. The caret is after the words already typed. Centre the
crop on the control and the frame fills with empty space to its right while the thing you
actually wanted the viewer to read falls off the left edge.

So, **in a take that types**, the crop sits a little left of the target:

```
LEFT_BIAS_TYPE  = 0.18   // fraction of the cropped width
LEFT_BIAS_CLICK = 0.12
```

Typing leans further because a line of text grows away to the right as it is written, so the
frame has to be holding more of the already-written text than a click does.

**No typing, no lean.** Everything the lean buys is text to the left of the target. A take
with no `type` op (a canvas, a game, a comic, a dashboard being clicked around) has nothing
there worth reading, and leaning there just pushed every shot off-centre; sweeping hovers
across a canvas made the camera sit left of the pointer the whole way. So `events_from_marks`
checks whether the take has any `type` mark: if it does, clicks lean by `LEFT_BIAS_CLICK`; if it
does not, clicks and hovers centre on the target (the surface slide below still applies).
Hovers never take the typing lean.

The lean is capped so the target cannot leave the frame it is the subject of:

```rust
let room = 0.5 - (w * scale / 2.0) / crop_w - KEEP_IN_FRAME;   // KEEP_IN_FRAME = 0.06
let bias = want.min(room.max(0.0));
let cx   = (x + w / 2.0) * scale - crop_w * bias;
```

`room` is how far the crop centre can move left before the target's own right edge gets within
6% of the crop's right edge. On a wide target `room` goes to zero or negative and the bias
disappears entirely: the crop centres, because there is no room to lean. On a small target the
full 0.18 or 0.12 applies.

`cx` and `cy` are then clamped into the frame, and `zoompan` clamps the resulting crop origin
again, so a target near an edge produces a crop that sits against that edge rather than hanging
off it.

The vertical is not biased. Content is read left to right and top to bottom, but a control's
vertical relationship to what it acts on is not consistent the way its horizontal one is, so
`cy` is the target's centre and nothing else.

## The shape of one move

```
        LEAD_IN            HOLD_AFTER
      |<-0.45s->|         |<--2.1s-->|
      t0        mark                 end
      |----------------------------------|
       ease in      hold        ease out
      |<-0.7s->|              |<-0.7s->|
```

- `t0 = mark - LEAD_IN`, floored at zero. The camera starts arriving before the thing happens,
  which is what makes it read as anticipation rather than reaction.
- `end = mark + HOLD_AFTER`, so the result of the interaction is on screen, zoomed, for about two
  seconds.
- `EASE` is 0.7s at each end, and every ease in kaviri is the same cubic smoothstep
  `s = p * p * (3 - 2 * p)`. The gradients in the backdrop use it too, which is not a
  coincidence: one easing curve for the whole tool means nothing in a frame moves on a different
  rhythm from anything else.

Outside `[t0, end]` the expression falls back to its base: `z = 1`, `cx = iw/2`, `cy = ih/2`.
Every zoom returns to the wide shot. There is no state carried between events.

## Merging: why the camera pans instead of pumping

Two clicks a second apart would otherwise be zoom in, zoom out, zoom in, zoom out. That is the
single most distracting thing an auto-zoom can do.

So while the next target starts within `MERGE_GAP` (1.3s) of the current event's end, it is
absorbed into the same event:

- `end` moves out to the new target's time plus `HOLD_AFTER`.
- `z` becomes the **minimum** of the two, never tighter than the loosest target merged in. A run
  of interactions is framed so that all of them fit.
- If the new target is more than 6 source pixels from where the event is anchored, it becomes a
  **path waypoint**: `(t, cx, cy)`. The camera stays zoomed and pans to it.

Note that distance is measured from the event's original anchor, not from the previous waypoint.
The anchor `cx`/`cy` stay put for the whole event; the path is what carries the camera away from
them and the ease-out is what brings it back.

Waypoint times are forced to be at least `WAYPOINT_MIN_GAP` (0.04s) apart so no segment has zero
length. That number used to be 0.15s, which was a bug: `ops.rs` samples the caret every 0.12s, so
a floor above the sampling interval pushed every single sample later than it happened, and the
error accumulated. A 4.3s typing pan measured 5.4s of waypoints and finished a second and a half
after the typing did. The floor only has to prevent a zero-length segment, which is far below the
sampling rate.

Finally, waypoints outside the eased window, not strictly between `t0 + EASE` and `end - EASE`,
are dropped, with a line on stderr saying how many and from which interaction. A pan during an
ease would fight the ease.

## The caret pan

While typing, the camera follows the text caret.

kaviri records agents. There is no hand on a mouse. The pointer kaviri draws is a prop: it is
parked wherever the field was clicked and stays there while a whole sentence is typed. Following
it would be following a stationary object. The thing that moves, and the thing a viewer is
reading, is the caret.

`ops.rs` measures the caret position twice, before the first character and after the last, and
then lays down evenly spaced synthetic `type` marks across the interval the typing actually took:
`round(span / CARET_SAMPLE_S)` of them with `CARET_SAMPLE_S = 0.12`, clamped to between 2 and 40.
Each mark's box is 2 pixels wide at the interpolated caret x, but carries the **field's** y and
height, so following the caret does not tighten the zoom down onto one line of text.

Measuring twice rather than per keystroke is the surprising part, and it is deliberate. Asking
the page where the caret is costs a CDP round trip. Doing that between keystrokes put the round
trip inside the typing rhythm: the words came out slower than `typewriter_ms` asked for, and the
camera moved in visible steps because the samples were as uneven as the latency. Two measurements
and a linear interpolation give both a smooth pan and typing at the speed that was requested.

The cost is that a caret that jumps around mid-typing, a wrapped line, an autocomplete that
rewrites the field, is interpolated through rather than followed. That is a trade taken
knowingly.

## Interpolation between waypoints, and why it is not eased

Between waypoints the camera moves at a constant rate. The eases live only at the two ends of the
whole move.

Easing each segment separately is the obvious thing to write and it is wrong: smoothstep drives
velocity to zero at both ends of each segment, so a run of close waypoints brings the camera to a
complete stop at every one of them. Over a typing pan, where samples are 0.12s apart, that reads
as stepping rather than gliding.

There is a test that asserts exactly this by counting easing terms in the generated expression:
two, regardless of how many waypoints there are.

One more detail that cost real debugging time. The pan's first point is clocked at `t0 + EASE`,
not at `t0`. The ease-in occupies `t0 .. t0 + EASE` and delivers its value at the end of that
window, so the pan chain has to take over at exactly that instant. Clocking the first point at
`t0` meant the first segment was already part-travelled by the time it became visible, and the
crop centre jumped 433 pixels in a single frame at the handover on the shipped demo.

## Thinning the path

Every retained waypoint costs roughly ninety bytes in each of the three generated expressions,
and the 29-second README demo alone produced 78 of them. Left unbounded, a five-minute take
builds a filter graph larger than the kernel will accept as a single argv element.

```
PATH_BUDGET       = 16    // per event
PATH_BUDGET_TOTAL = 192   // across the take
```

When the events collectively want more than the total, each event's budget is scaled down by the
same ratio, with a floor of 2.

The thinning itself is Ramer-Douglas-Peucker with a point budget: keep the first and last
waypoints, then repeatedly keep whichever remaining waypoint is furthest from the straight line
the pan would otherwise take through its neighbours, stopping when the worst remaining error is
under one pixel. Dropping every n-th point instead would flatten exactly the corners a viewer
notices. The shape of a pan survives decimation; its byte count is what has to stop growing.

## The tail

An event ends `HOLD_AFTER` past its interaction and is dropped entirely if that lands beyond the
last frame. That meant every script ending in a click and a `stop_recording` lost the zoom on the
very thing it was demonstrating.

Two mechanisms address it, and both are worth knowing because one of them is a compromise.

**Tail padding.** `tail_pad_for` looks at the last `click` or `type` mark and works out how much
longer the take would have to be for that interaction to get its full hold. The CFR pass then
holds the final captured frame for that long, up to `MAX_TAIL_PAD` (`HOLD_AFTER + EASE`, 2.8s).
This is honest: nothing happened after the last frame either way, so holding it is not inventing
footage.

**The refusal.** An event whose window comes out shorter than `2 * EASE + 0.15` (1.55s), after
being clamped to `duration - TAIL_MARGIN`, is dropped, because there is not enough time in it to
ease in and out. When that happens kaviri says so on stderr:

```
kaviri: the interaction at 28.4s is too close to the end of the 29.0s take to be zoomed;
add a trailing wait before stop_recording
```

Silence there is what made this expensive to diagnose: the only trace of a lost zoom used to be a
lower event count in a sidecar nobody reads.

**Write the wait into the script anyway.** Padding holds a frozen frame; a real
`{"op":"wait","ms":1600}` before `stop_recording` films the real page settling. All the bundled
examples end with one.

## From events to ffmpeg

`build_expr` turns the event list into one expression per axis. The events are sorted by start
time descending and each wraps the previous, so the earliest event ends up as the outermost
`if(between(it, t0, t1), …)`. If two events ever overlapped, the earlier one would win.

A single event with no path:

```
if(between(it,3.550,6.100),
   if(lt(it,4.250), <smoothstep from base to v over 3.550..4.250>,
   if(lt(it,5.400), <v>,
                    <smoothstep from v back to base over 5.400..6.100>)),
   <everything else>)
```

With a path, the constant middle section is replaced by a chain of `clip`-bounded linear
segments, innermost last.

The three expressions are substituted into one `zoompan`:

```
fps=30,zoompan=z='(Z)':
  x='clip((CX)-iw/(2*(Z)),0,iw-iw/(Z))':
  y='clip((CY)-ih/(2*(Z)),0,ih-ih/(Z))':
  d=1:fps=30:s=CWxCH
```

`cx`/`cy` are crop **centres**, which is why each is converted to an origin and clipped into the
frame here. `d=1` because every output frame is computed independently from its input frame;
`zoompan`'s own multi-frame mode would fight the expressions.

If the whole graph exceeds `GRAPH_ARG_LIMIT` (32 KiB) it is passed by filename with
`-filter_script:v` or `-filter_complex_script` instead of as an argument. Linux caps one argv
element at 128 KiB. Ordinary takes stay on the argument, because recent ffmpeg deprecates the
script forms. The graph is written to the render's temp directory either way, and on a pass-2
failure the error message names the file and the temp directory is retained.

## Why two passes

Pass 1 normalizes the variable-rate captured frames to constant-rate 30fps H.264. Pass 2 does the
zoom.

The order is not negotiable. Every number in this document is a time in seconds, and marks are
timestamped against a wall clock while frames arrive whenever the browser produced them. Doing
time-based maths against a variable-rate source means every zoom lands at the wrong moment.
Normalizing first makes frame index and time the same quantity.

Pass 1 is also where memory stays flat: frames are read off the spool one at a time and a static
stretch reuses the same JPEG for many ticks, so a five-minute take costs disk rather than RAM.

## Interactions with capture

Two things upstream of the planner change what the camera can do.

**`--scale`.** At `--scale 1` kaviri uses the DevTools screencast, whose frames are capped at the
CSS viewport size no matter what is asked for. A zoom into a 1x capture crops into upscaled
pixels and looks soft. Above 1, the screenshot pump supersamples and a zoom crops into pixels
that were really captured. This is the whole reason the pump is the default despite costing the
filmed app real CPU.

**The pointer is in the page.** kaviri injects an SVG cursor into the document rather than
compositing one at render time. That means the zoom transform carries it along for free: its
position cannot drift away from the click it belongs to, there is no second compositing path, and
it magnifies with the content the way a magnified screen recording does. It also means
`--cursor-scale` is measured in the viewport's own pixels, so on the narrow vertical presets the
pointer is a much larger share of the frame and `--cursor-scale 1.0` suits them better.

## Tuning it

There are no flags for any of the constants above, and that is on purpose: a camera with eleven
knobs is a camera nobody gets right. What you can change is the script, and the script is the
right place:

- **A zoom that is too tight** means the target is small. Target a wrapping container rather than
  the button inside it, and the ladder plus the fit bound will loosen.
- **A zoom that never happens** means no `click` or `type` op resolved a box. Check
  `zoom_events` in the `stop_recording` result, and check for the "too close to the end" warning.
- **Pumping between two nearby controls** means they are more than 1.3s apart. Tighten the
  `wait` between them and they merge into one pan.
- **A pan that feels rushed** means the typing is fast. `typewriter_ms` is the pacing control for
  the caret pan as well as for the text.
- **A move that goes somewhere you did not expect** is best diagnosed from the telemetry sidecar:
  `KAVIRI_KEEP_TEMP=1` writes `<out>.telemetry.json` with every mark, its box, and every computed
  zoom event including its path. Read it before guessing.
