# Motion graphics

`kaviri record` films a web app that already exists. `kaviri motion` films one it builds
for the purpose: a launch video, a showreel, a feed ad, a changelog clip. The whole video is a
`.jsonl` file of scenes, layers, animations, micro-interactions and sounds, and it renders to
an MP4 with a soundtrack whose every beat the picture was cut to.

```sh
kaviri motion --script examples/motion/showreel.jsonl --out showreel.mp4
```

It is the same idea as the recorder, applied to motion design. The video is a build artifact:
the script lives in the repo, and when the product changes you change a line and render again
instead of reopening an editor. And because the script is plain JSON lines, a model can write
one.

- **The simple way**: [`motion-simple.md`](motion-simple.md). A brand, one line per beat and
  an end card; kaviri picks the motion, transitions and music. Published as
  [kaviri.dev/motion-simple.txt](https://kaviri.dev/motion-simple.txt), short enough for any
  model to follow.
- **For a model**: [`motion-llm.md`](motion-llm.md) is the complete format in one file, written
  to be pasted into a prompt or attached to a chat. It is published as
  [kaviri.dev/motion-llm.txt](https://kaviri.dev/motion-llm.txt).
- **Prompts that work**: [`motion-prompts.md`](motion-prompts.md).
- **Examples**: `examples/motion/showreel.jsonl` (a 32 second 16:9 showreel, dark),
  `examples/motion/portraits.jsonl` (a 15 second 4:5 feed ad, light) and
  `examples/motion/kaviri-dev.jsonl` (kaviri.dev as a launch video: the site's copy, real
  screenshots of its sections in `examples/motion/site/`, and its Parcel demo rebuilt as an
  `html` layer whose input types and whose button is pressed).

## The simple way

```jsonl
{"op":"brand","name":"kaviri","accent":"#c7361a","theme":"light","url":"kaviri.dev","logo":"../../brand/logomark.svg"}
{"op":"beat","text":"Demos go [stale.]","sub":"The UI moves. The video does not."}
{"op":"beat","text":"Nobody [re-records] it.","show":{"strike":["Block out an afternoon.","Find a quiet room."]}}
{"op":"beat","text":"Your demo video is a [build artifact.]","show":{"image":"site/hero.png"}}
{"op":"beat","text":"The recorder is [free.]","show":{"chips":["Apache 2.0","No account"]}}
{"op":"end","tagline":"Your demo video is a [build artifact.]"}
```

That is a finished video with music. Each `beat` is a two-bar scene: a headline sized to fit,
and optionally one `show` (a screenshot in a browser window, a chat box that types, a code
block that types, a checklist that ticks, stats that count, an orbit of icons, chips, lines
that get crossed out). Entrances and transitions rotate so no two beats move alike. The third
beat is the drop, with a flash, a shake and a shockwave, and the beat before it builds with
rays. The music is written to fit: intro, build, drop, a break before the end if there are
five beats or more, and an outro under the end card. `examples/motion/simple.jsonl` is the
whole of kaviri.dev this way, in eight lines.

`brand`, `beat` and `end` expand into the ops described below, so anything in the full format
can be added on extra lines when the simple way is not enough.

## The shape of a script

```jsonl
{"op":"video","size":[1920,1080],"fps":30,"bpm":120}
{"op":"music","key":"F#m","sections":[{"bars":2,"part":"build"},{"bars":4,"part":"drop"}]}
{"op":"background","kind":"nebula"}
{"op":"scene","id":"hook","dur":"2bar"}
{"op":"text","scene":"hook","text":"Every tool. [One place.]","size":110,"in":{"fx":"wave","at":"1b"}}
{"op":"scene","id":"product","dur":"4bar","transition":"flash"}
{"op":"ui","id":"box","scene":"product","kind":"input","w":900,"in":"rise"}
{"op":"act","target":"box","do":"type","at":"1b","text":"Summarise this week's tickets"}
```

Four ideas carry the whole format.

**Time is musical.** Any time can be written in seconds, beats (`"2b"`) or bars (`"1bar"`), and
summed (`"4bar+2b"`). Write in beats and the picture lands on the music without anyone
counting milliseconds. The build above is two bars; the product scene starts on the drop.

**Scenes are laid end to end** and each layer's times are relative to its scene. A transition
is centred on the cut, so a half-beat `flash` whites out the last quarter beat of one scene
and the first quarter beat of the next.

**Build from components, not screenshots.** A screenshot can only slide and scale. The `ui`
kit rebuilds the pieces (a window, a phone, an input that types, a list whose toggles flip,
a code block that types itself, cards, chips, stats that count, a cursor that clicks) so each
part can move on its own, which is what makes a video look designed rather than assembled.

**Motion is layered.** A layer has base properties, `keys` (keyframes), an `in` entrance and
an `out` exit, `loop`s that run while it is up, `anim` ops that add motion to it or to elements
inside it, and `act`s, the micro-interactions. Text entrances run per letter or per word.

The complete list of every op, field, effect and ease is in [`motion-llm.md`](motion-llm.md).
It is one page on purpose.

## The soundtrack

The `music` op writes a score from nothing but arithmetic: a key, a chord progression and a
list of sections.

```json
{"op":"music","key":"F#m","progression":["i","VI","III","VII"],
 "sections":[{"bars":2,"part":"intro"},{"bars":2,"part":"build"},{"bars":8,"part":"drop"},
             {"bars":2,"part":"break"},{"bars":2,"part":"outro"}]}
```

Each part is an arrangement. An `intro` is a pad and a filtered arpeggio. A `build` opens the
filter, rolls a snare that doubles into its last bar, runs a riser, and leaves the half beat
before a drop empty. A `drop` brings in the kick on every beat, the clap, hats, a sidechained
bass, stabs and an impact on its first downbeat. A `break` takes the kick out, and an `outro`
lands an impact and holds the chord. `style` re-voices it all: `pulse` (electronic),
`cinematic` (toms, drones, braams) or `minimal` (soft keys).

Because the score and the picture are written on one grid, everything lines up for free:
`pulse` loops and the global frame pulse hit on the kick, a `build` riser ends on the cut into
the drop, and the drop's impact is the frame the logo lands on. On top of the score kaviri adds
sound effects the picture implies: a whoosh on every transition, an impact on a flash, typing
under a `type` act, a click under a `click`, pops under checks. `sfx` ops place any of twelve
effects yourself.

Bring your own track with `{"op":"music","src":"track.mp3","bpm":128,"offset":"0.1s"}`, where
`offset` is where the first downbeat falls in the file. Everything still snaps to its grid,
and the automatic effects are mixed on top.

The score is deterministic: the same script makes the same WAV, sample for sample. Nothing is
sampled and nothing is downloaded.

## Working on a video

```sh
kaviri motion --script reel.jsonl --check
```

Validates the script and prints the timeline: each scene's start in seconds and as a bar and
beat, the length, and how many layers and sound cues there are. Every name is checked, so a
misspelled effect is an error with the line number and the list of real names, not a layer
that silently does nothing.

```sh
kaviri motion --script reel.jsonl --still 1b,2bar,3bar+2b,14.5 --out look.png
```

Writes a PNG of each moment in a couple of seconds. This is the loop to work in, for a person
or a model: change the script, look at the frames that matter, repeat. Times on the command
line may be musical too.

```sh
kaviri motion --script reel.jsonl --preview player/
```

Writes `player/index.html` and `player/music.wav`: the video playing live in any browser, with
sound, a scrubber and space to pause. It is the same runtime the renderer photographs, so what
plays is what renders.

```sh
kaviri motion --script reel.jsonl --out reel.mp4
```

Renders it. Several browsers photograph frames in parallel (`--jobs`, one per CPU by default);
the frames and the soundtrack are then encoded with libx264 and AAC. `--from` and `--to`
render part of it, `--fps` overrides the frame rate, `--crf` sets quality (16 by default), and
`--audio-out` writes the soundtrack as a WAV too.

## How it renders, and why it is exact

The page kaviri builds is a pure function of time. There is no animation clock running in
the browser: CSS transitions are switched off, CSS animations are paused and set to the frame's
time, particles are drawn from a seed, and every random choice comes from a hash of the layer's
id. kaviri calls `KV.seek(t)` for each frame and photographs the result. So frame 900 is the
same whether it is rendered first, last, or by the third of four browsers, and a still at
`--still 14.5` is exactly frame 435 of the video.

That is also why it is not a screen recording of an animation. A browser under software
rendering paints a busy page at a handful of frames a second; photographing seeked frames
gives every frame of a 60fps video, however slow each one is to paint.

On a four-core machine a 1080p showreel renders at about ten frames a second, so a 30 second
video takes a minute or two. Heavy blur on large layers is the most expensive thing on the
page.

## What it needs

The same as the recorder: a Chromium and an ffmpeg with libx264 (`kaviri doctor` checks both).
Fonts come from the machine unless a `font` op embeds one; the default stack starts with
Inter, so install Inter or embed it for the intended look.
