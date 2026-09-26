# kaviri motion: the whole format, for a model

You are writing a `.jsonl` file that `kaviri motion` turns into a finished MP4 with music.
This file is everything you need. Read it once, then write the script.

```sh
kaviri motion --script reel.jsonl --check                 # validate, print the timeline
kaviri motion --script reel.jsonl --still 2b,1bar+2b,9.5 --out look.png   # look at frames
kaviri motion --script reel.jsonl --out reel.mp4          # render the video with sound
kaviri motion --script reel.jsonl --preview player/       # a page that plays it live, with sound
```

## How to think about it

- **One JSON object per line.** Lines starting with `#` or `//` and blank lines are ignored.
- **Time is musical.** Every time field takes seconds (`1.5`, `"1.5s"`, `"300ms"`), beats
  (`"2b"`), bars (`"1bar"`) or a sum (`"4bar+2b"`, `"8b-0.5b"`). Write times in beats and bars,
  and every cut, entrance and hit lands on the music. At 120 bpm a beat is 0.5s and a bar 2s.
- **Scenes are laid end to end.** Each `scene` has a `dur`; the next one starts where it ends.
  Times on a layer are relative to its scene's start. A transition is centred on the cut.
- **Layers are built from components, not screenshots.** Rebuild the product out of `ui`
  components, `text`, `icon`, `shape` and `image` so every part can move on its own.
- **Motion is layered**: base props, then `keys` (keyframes), then the `in` entrance, the
  `out` exit, `loop`s (ambient motion), `anim` ops and `act`s (micro-interactions).
- **The soundtrack is generated from the same grid**: `music` synthesizes a score whose
  sections you choose, and transitions, clicks, typing, checks and flashes add their own
  sound effects automatically.
- **Positions**: `x`, `y` are pixels or percentages of the parent (`"50%"`, `"50%+120"`). A
  layer is centred on its point (`anchor` `[0.5,0.5]`). With no `x`/`y` a layer sits in the
  centre of its parent; inside a `group` it sits at the group's point (0, 0).
- **Everything is validated.** A misspelled name is an error naming the line and listing the
  real names. Run `--check` after writing, `--still` to look, then render.

## A complete small example

```jsonl
{"op":"video","size":[1920,1080],"fps":30,"bpm":120}
{"op":"theme","accent":"#5b8cff"}
{"op":"music","key":"F#m","sections":[{"bars":1,"part":"intro"},{"bars":1,"part":"build"},{"bars":4,"part":"drop"},{"bars":2,"part":"outro"}]}
{"op":"background","kind":"nebula"}
{"op":"scene","id":"hook","dur":"2bar"}
{"op":"text","scene":"hook","text":"Every tool. [One place.]","size":110,"in":{"fx":"wave","at":"1b","dur":"1b"},"out":{"fx":"scatter","at":"7b"}}
{"op":"particles","scene":"hook","kind":"rays","at":"5b","dur":"3b"}
{"op":"scene","id":"ui","dur":"4bar","transition":"flash"}
{"op":"ui","id":"box","scene":"ui","kind":"input","w":900,"chips":["Fast","Private"],"in":{"fx":"rise","at":0}}
{"op":"act","target":"box","do":"type","at":"1b","text":"Summarise this week's tickets"}
{"op":"act","target":"box","do":"click","sel":".kv-send","at":"5b"}
{"op":"scene","id":"end","dur":"2bar","transition":"zoom"}
{"op":"text","scene":"end","text":"acme","size":160,"in":{"fx":"rise","at":0}}
{"op":"ui","scene":"end","kind":"chip","label":"acme.dev","y":"65%","in":{"fx":"pop","at":"2b","sfx":"pop"}}
```

## Top-level ops (at most one of each)

**`video`**: `size` `[w,h]` or `"1080p"` `"720p"` `"4k"` `"vertical"` (1080x1920) `"square"`
(default 1920x1080; even numbers). `fps` (30). `bpm` (120). `beats_per_bar` (4). `duration`
(default: where the scenes end, or the music). `pulse` (0.012: the whole frame kicks on each
beat, scaled by how hard the music plays; 0 turns it off). `grain` (0.06). `vignette` (0.55).
`perspective` (1600, px, for 3D). `letterbox` (px or %). `seed` (varies every random
placement). `bg` (fallback background colour).

**`theme`**: colours and type the components use. `bg` `bg2` `surface` `surface2` `border`
`border_strong` `hover` `text` `text2` `muted` `faint` `accent` `accent2` `ok` `track` `radius`
(px) `font` `mono` `shadow`, and `token_k` `token_s` `token_n` `token_c` `token_f` `token_t`
`token_p` to override code colours (keywords, strings, numbers, comments, functions, types,
punctuation; they follow the theme's lightness by default). Defaults are a dark UI with a blue accent. For a light look set
`bg`, `surface`, `surface2`, `text`, `muted`, `border` and `shadow` together.

**`font`** (may repeat): `{"op":"font","family":"Inter","src":"fonts/Inter.woff2","weight":"100 900"}`.
The file is embedded. Without one, fonts come from the machine; `Inter` then system sans.

**`music`**, synthesized:
- `key`: `"Am"`, `"F#m"`, `"Eb"`, `"D major"` (default `Am`).
- `progression`: roman numerals, one chord per `chord_bars` (1): `["i","VI","III","VII"]`
  (default for minor), `["I","V","vi","IV"]` (major). Add `7` for sevenths: `"iv7"`.
- `sections`: `[{"bars":2,"part":"intro"}, …]`. Parts: `intro` (pad, soft arp), `build`
  (rising filter, snare roll that doubles, a riser, a half-beat gap before a drop), `drop`
  (kick on every beat, clap, hats, rolling bass, arp, stabs, and an impact on its first
  downbeat), `break` (pad and arp, no kick), `outro` (impact, sustained chord),
  `silence`. `bars` may be fractional. **Make the sections cover the scenes**, and put a
  `drop` where the picture's big moment is.
- `style`: `pulse` (electronic, default), `cinematic` (toms, drones, braams), `minimal` (soft
  keys, light beat).
- `gain` (1), `seed`, `fade_out` (1.5s), `auto_sfx` (true).

**`music`** from a file: `{"op":"music","src":"track.mp3","bpm":128,"offset":"0.12s","start":0}`.
`offset` is where the first downbeat is in the file (everything on the grid shifts by it),
`start` skips into the file. `auto_sfx` still adds effects on top.

**`background`**: `kind` one of
- `nebula`: drifting colour blobs and stars. `colors` (3 hex), `base`, `stars` (140),
  `star_color`, `drift`, `intensity` (0.85), `speed`.
- `gradient`: `colors`, `angle`, `spin` (deg/s).
- `grid`: a perspective floor. `base`, `color`, `cell`, `glow`, `speed`, `stars`.
- `solid`: `color`.
- `aurora`: `colors`, `base`, `stars`.
- `mesh`: moving soft colour fields, good for light themes. `colors`, `base`, `speed`.

## Scenes and the camera

**`scene`**: `id` (required), `dur` (required), `at` (to place it rather than follow the
last one), `transition` into this scene: a name or `{"kind":…,"dur":…}` (default length one
beat, 0.25 to 0.8s). `push` (0.04: slow zoom-in across the scene; 0 for none). `drift` `[vx,vy]`
px/s. `camera` `{"keys":[…]}` with `zoom`, `x`, `y` (the point to centre, in px from the
middle), `rotate`, `rx`, `ry`. `bg` (any CSS background, covers the global one).

Transitions: `cut` `dissolve` `zoom` (through the frame) `whip` (horizontal motion blur)
`slide` (vertical) `push` `flash` (white hit) `blur` `iris` `glitch` `spin`.

**`camera`**: keys for the whole video, or for one scene with `"scene":"id"`:
`{"op":"camera","scene":"s2","keys":[{"t":"1b","zoom":1},{"t":"2b","zoom":1.5,"x":-200,"y":-80,"ease":"inOutExpo"}]}`.
A camera zoom crops everything in the scene; give a layer `"fixed":true` to keep it out of the
camera (a headline over a zooming UI).

**`shake`**: `at`, `dur` (0.5), `amp` (14 px), `freq` (22), `scene`.
**`flash`**: `at`, `dur` (0.35), `color` (white), `peak` (1), `scene`. Adds an impact sound.
**`sfx`**: `kind`, `at`, `scene`, `gain`, `pan` (-1..1), `pitch` (multiplier), `dur`.
Kinds: `whoosh` `impact` `riser` (ends at `at`+`dur`) `pop` `click` `tick` `swell` `glitch`
`shimmer` `boom` `type` `chime`.

Sounds added for you when there is music: a whoosh on every transition (an impact on `flash`,
a glitch on `glitch`), an impact on `flash` ops, typing on `type`, a click on `click`, a tick
on `toggle` and `select`, a pop on `check`, and whatever `in.sfx` names on a layer. Give an
act `"sfx":false` to silence it.

## Layers

Layer ops: `text`, `image`, `svg`, `icon`, `shape`, `ui`, `html`, `group`, `particles`.

Fields every layer takes:

| field | meaning |
|---|---|
| `id` | name it to target it with `act`/`anim`, or to use it as a `parent` |
| `scene` | the scene it belongs to (children inherit their parent's). Without one it is global: absolute times, over every scene |
| `parent` | a `group`, `window`, `phone`, `html`, `image` or other box declared on an earlier line; the child moves with it and its x/y are inside it |
| `x` `y` `z` | position (px, `"50%"`, `"50%-200"`); z in px towards the camera |
| `anchor` | `[0.5,0.5]` is the centre; `[0,0.5]` pins the left edge |
| `w` `h` | size, where the kind uses one |
| `scale` `sx` `sy` `rotate` `rx` `ry` | transform; `rx`/`ry` tilt in 3D |
| `opacity` `blur` `bright` | |
| `glow` `glow_color` | a soft light around the layer (px) |
| `layer` | z-index among siblings |
| `fixed` | outside the scene camera |
| `from` `until` | seconds (scene time) the layer exists between |
| `style` `class` | raw CSS on the layer, a class for your own `html` CSS |
| `in` `out` `loop` `keys` `trail` `shine` | motion, below |

### `text`

`text` with markup: `[accent words]`, `{muted words}`, `~struck words~` (the line draws on a
`strike` act), `\n` for a new line, `\[` for a literal bracket. `size` (72) `weight` (700)
`color` `accent` `font` (`"mono"` or a CSS family) `tracking` (em, default -0.02) `leading`
`align` (`center`, `left`, `right`) `italic` `upper` `shadow` (true or CSS) `gradient` (a list of
colours, fills the letters) `gradient_angle` `caret` (show a text caret) `caret_color`
`caret_hold` `strike_color` `split`.

Text entrances run per character by default, staggered: `wave`, `rise`, `pop`, `flip`,
`typewriter`, `scramble`, `converge`, `mask` and the rest. `split`: `char`, `word`, `line` or
`none` (whole block); it may sit on the layer or inside `in`/`out`. With `split:"word"` the
stagger is per word. Keep `stagger` × units short enough to finish inside the scene.

### `image`

`src` (png, jpg, webp, gif, avif; svg files are inlined so `draw` works on their strokes),
`w` `h` (default w 400), `fit` (`contain`/`cover`), `radius`, `shadow` (true or CSS), `border`
(true or CSS). Paths are relative to the script. Images can hold children (a badge on a photo).

### `svg`

`svg` (markup) or `src` (a file), `w` (200) `h`, `color` (for `currentColor`). Strokes can be
drawn on with `"in":"draw"` or keys on `draw` (0..1).

### `icon`

kaviri's illustrated app tiles: `name` one of `files browser mail chat music photos calendar
notes settings terminal code camera video maps store ai game wallet device`. `size` (96),
`label` (a caption under it).

### `shape`

`kind`:
- `rect`: `w` `h` `radius` (16) `fill` (colour, or a list for a gradient) `angle` `stroke`
  `stroke_width` `shadow`
- `circle`: `d`, `fill` (a list makes a lit sphere), `stroke`
- `ring`: `d`, `stroke`, `stroke_width`, `dash` (draws on with `in:"draw"`)
- `line`: `to` `[dx,dy]`, `stroke`, `stroke_width` (draws on)
- `glow`: `d`, `color`, `intensity`, `blend` (a soft light bloom; also a dark scrim with a
  dark colour and `"blend":"normal"`)
- `path`: `d`, `viewBox`, `fill`, `stroke`, `stroke_width` (draws on)
- `star`: `d`, `points` (5), `inner`, `fill`, `stroke`

### `group`

A point that holds children. `layout`:
- `{"kind":"orbit","rx":360,"ry":120,"period":8,"depth":0.35,"tilt":0,"phase":0}`: children
  circle in 3D, the back ones smaller and dimmer. Animate `orbit_rx`, `orbit_ry`,
  `orbit_angle` (degrees, extra spin) and `orbit_depth` in the group's `keys` to converge,
  expand or whip it round.
- `{"kind":"row","gap":24}`, `{"kind":"column","gap":24}`: centred on the group point.
- `{"kind":"grid","cols":5,"gap":24,"cell":[w,h]}`
- `{"kind":"ring","r":300,"squash":1,"phase":0}`, `{"kind":"scatter","w":…,"h":…}`

`cascade`: an entrance handed to each child in order, `{"fx":"pop","at":"1b","stagger":"0.25b"}`
(a child with its own `in` keeps it). `trail` on an orbit group draws light trails behind
every child. Rotate or tilt the group (`rotate`, `rx`) for a 3D wall.

### `particles`

`kind`: `stars` (`count`, `vx`, `vy`, `size`), `dust` (`count`), `bokeh` (`count`, `size`),
`burst` (sparks from a point: `at`, `count`, `speed`, `life`, `size`, `gravity`),
`rays` (hyperspace streaks: `at`, `dur`, `count`, `speed`), `shockwave` (expanding rings: `at`,
`dur`, `rings`, `width`, `radius`), `confetti` (`at`, `count`, `speed`, `life`). All take
`colors`, `ox` `oy` (the origin, default the centre), `w` `h` (default the frame), `blend`.

### `ui`: the component kit

Rebuild the product from these. Every one is a real DOM component styled by `theme`.
Children with `"parent"` set to a `window`, `phone` or `card` go inside it.

| `kind` | fields | acts it answers |
|---|---|---|
| `window` | `title` or `url` (an address bar), `sidebar` (strings, `"# Heading"`, or `{label,icon,color,active}`), `sidebar_width`, `chrome:false`, `w` (1200) `h` (720) | children go in the content area |
| `phone` | `title`, `action`, `clock`, `status:false`, `frame`, `screen`, `w` (390) `h` (800) | children go on the screen |
| `card` | `title`, `subtitle`, `icon` (a glyph, or a built-in icon name), `icon_src`, `icon_bg`, `badge`, `body` (`**bold**`, `` `code` ``), `lines` (placeholder lines), `button`, `check`, `w` (360) | `stream`, `check`, `highlight`, `click` |
| `input` | `placeholder`, `text`, `chips` (`["GPT"]` or `{label,icon,color}`), `tools` (5), `send:false`, `send_label`, `w` (760) | `type`, `click` with `"sel":".kv-send"` |
| `button` | `label`, `icon`, `variant` (`primary`, `secondary`, `ghost`), `size`, `bg` | `click` |
| `toggle` | `label`, `on` | `toggle` |
| `check` | `label`, `checked` | `check` |
| `chip` | `label`, `icon`, `color`, `size` | |
| `list` | `title` or `search` (+`search_text`), `items` (`{label,icon,color,badge,meta,toggle:false,check:false}`), `selected`, `w` (380) | `select`, `toggle`/`check` with `index`, `type` (into the search) |
| `code` | `code`, `lang` (`js`, `py`, `sh`, `rust`, `json`…), `title` (false to hide), `size` (font px), `typed:false` | `type` (types the code out, highlighted) |
| `message` | `name`, `tag`, `text` (`**bold**`), `avatar`, `avatar_src`, `avatar_bg`, `w` (620) | `stream` |
| `field` | `label`, `value`, `placeholder`, `mask` (dots), `w` (420) | `type`, `check` (a green tick) |
| `cursor` | `style` (`arrow`, `hand`), `size` (34). Move it with `keys` | `click` (press and ripple) |
| `stat` | `value`, `prefix`, `suffix`, `label`, `decimals`, `size`, `color` | `count` |
| `rating` | `title`, `value`, `value_label`, `stars` (0..5), `source`, `laurel:false` | |
| `progress` | `value` (0..1), `color`, `w` | `progress` |
| `skeleton` | `lines` | |
| `kbd` | `label`, `size` | `click` |
| `tile` | a logo tile: `glyph` (a letter or emoji), `icon` (a built-in icon name), `src` (an image), `svg`, `bg` (colour or `[c1,c2]`), `color`, `size` (88), `radius`, `shape:"circle"`, `ring`, `label` | |

Panels (`window`, `card`, `input`, `list`, `code`, `message`) also take `radius`, `bg`,
`border:false`, `shadow:false` and `glass:true`.

### `html`

`html` (markup) or `src`, plus `css` (a stylesheet added to the page) and `w`/`h`. The escape
hatch for anything the kit lacks. Its elements can be animated with `anim` + `sel` and
driven with `act` + `sel`. CSS transitions are switched off, because the page is photographed
frame by frame and only kaviri's clock moves; CSS `@keyframes` animations do work, and run on
kaviri's clock (their time zero is the start of the video).

## Motion

### `in` and `out`

A name, or an object:
`{"fx":"wave","at":"1b","dur":"1b","ease":"outBack","stagger":"0.05b","order":"center","delay":0,"split":"char","sfx":"pop"}`.
`at` is scene time. Defaults: `in` at 0 for 0.7s, `out` for 0.5s. `order`: `forward`,
`reverse`, `center`, `edges`, `random`. `spread` spreads the whole stagger over a time
instead. A layer is invisible before its `in` starts and after its `out` ends.

Entrances: `fade` `rise` `drop` `left` `right` `pop` `zoom` (from big and blurred) `blur`
`wave` (per letter, the showreel favourite) `flip` (3D) `spin` `swing` `converge` (letters fly
in from everywhere) `typewriter` (with a caret) `scramble` (random glyphs resolve) `mask`
(letters rise from behind a line) `wipe` `wipe-up` `wipe-down` `iris` (circle reveal) `draw`
(strokes draw on) `stretch` `fly` (from deep in z) `glitch` `elastic` `bounce` `none`.

Exits: `fade` `fall` `rise` `left` `right` `pop` `zoom` (through the camera) `blur` `wave`
`flip` `scatter` (letters explode) `shrink` `wipe` `iris` `fly` `glitch` `spin` `none`.

Custom: `"in":{"from":{"y":80,"opacity":0,"blur":12,"rotate":-8,"scale":0.8},"at":"1b","dur":"1b"}`
and `"out":{"to":{…},"at":…}`. Keys: `x y z scale sx sy rotate rx ry opacity blur`.

### `keys`

Keyframes on any layer: `[{"t":"2b","x":400,"scale":1.2,"ease":"inOutCubic"}, …]`. `t` is when
the value is reached; the tween starts at the previous key for that property (or the scene
start). Use `"at"` and `"dur"` instead to say when a tween starts and how long it runs. Before
its first key a property holds its base value. Properties: `x y z scale sx sy rotate rx ry
opacity blur glow bright draw w h color fill`, and on an orbit group `orbit_rx orbit_ry
orbit_angle orbit_depth`. Colours interpolate.

Eases: `linear` `step` `inQuad` `outQuad` `inOutQuad` `inCubic` `outCubic` `inOutCubic`
`inQuart` `outQuart` `inOutQuart` `inQuint` `outQuint` `inOutQuint` `inExpo` `outExpo`
`inOutExpo` `inCirc` `outCirc` `inOutCirc` `inBack` `outBack` `inOutBack` `outElastic`
`outBounce` `spring` `snap` (fast then a long settle), or `[x1,y1,x2,y2]` cubic-bezier.

### `loop`

One or a list, each a name or `{"fx":…, …}`, optionally limited with `at`/`dur`:
`float` (`amp` 10px, `period` 3s, `phase`), `sway` (`amp` 3°), `spin` (`period` 8s, `dir`:-1),
`pulse` (**on the beat**: `amp` 0.06, `every` "1b", `sharp`, `energy:false` to ignore the
music's intensity), `breathe`, `wiggle` (`amp`, `freq`), `flicker`, `glow` (**on the beat**:
`amp` 24px), `shine` (a light sweep across the layer), `drift` (`vx`,
`vy` px/s, `vr` °/s, `vs`).

### `trail`

`"trail":true` or `{"len":0.45,"width":6,"color":"#hex","opacity":0.9}`: a fading ribbon of
where the layer was over the last `len` seconds. On an orbit group it trails every child.

### `anim`

More motion on an existing layer, or on elements inside it:
`{"op":"anim","target":"list1","sel":".kv-li","fx":"rise","at":"1b","stagger":"0.1b"}`,
`{"op":"anim","target":"card","at":"2b","dur":"1b","to":{"rotate":0,"scale":1.1}}`,
`{"op":"anim","target":"logo","keys":[…]}`, or `in`/`out`/`loop` fields like a layer's.

### `act`: micro-interactions

`{"op":"act","target":"id","do":…,"at":…}` plus:

| `do` | fields |
|---|---|
| `type` | `text`, `cps` (28) or `dur`, `sel`, `append`, `hold` (caret blink after). On a `code`: types its code |
| `stream` | `dur` or `wps` (14): words appear with a blur (message, card body) |
| `click` | `sel` (press an element inside), `dur`, `depth`, `color`, `size` of the ripple |
| `toggle` / `check` | `index` (list rows), `value:false` to turn off, `dur` |
| `select` | `index`: the list highlight slides to that row |
| `count` | `to`, `from`, `dur` (1.2s), `ease`: a stat ticks up |
| `highlight` | `sel`, `color`, `hold`, `until`: a glowing ring |
| `strike` | `index`, `dur`: draws the line through `~struck~` text |
| `progress` | `to`, `dur` |
| `class` | `name`, `sel`, `until`: adds your own class |
| `text` | `text`, `sel`: swaps the content |

## Making it look professional

1. **Cut on the grid.** Scenes of 1 or 2 bars, entrances on beats, the big reveal on a
   `drop`'s first downbeat with a `flash` transition, a `shake` and a `shockwave`.
2. **Break the product apart.** A chat app is an `input` that types, a `list` whose toggles flip
   on the beat, `message` cards that stream in, a `code` block that types itself. Stagger them.
3. **Never hold still.** Scenes `push` by default; add `float` to floating chips, `pulse` or
   `glow` on the hero, `drift` on walls, `orbit` for logo clouds, `trail` on things that move.
4. **Big type, few words.** One idea per scene, 70 to 140px, the key words in `[accent]`.
   Enter with `wave`, `converge` or `mask`; leave with `scatter`, `zoom` or `blur`.
5. **Depth.** Tilt panels with `ry`/`rx` and ease them flat; `fly` things in from z; tilt a
   whole `group` grid into a 3D wall.
6. **Build to a drop.** `intro` (1-2 bars) and a `build` (1-2 bars) under the opening, `drop`
   under the product, `break` for the quiet line, `outro` under the logo.
7. **End on the brand**: mark, wordmark, one line, a URL chip, on the `outro`'s impact.
8. **Look before you render**: `--still` at the moments that matter, fix, then render.

## Mistakes the checker catches, and ones it cannot

Caught: unknown ops, effects, eases, kinds and sounds; a `parent` declared after its child; an
`act` on a layer that does not exist; a scene with no `dur`; bad times. Not caught: text
larger than the frame, layers on top of each other, a stagger longer than its scene, music
that ends before the picture (kaviri warns), a camera zoom that crops a headline (use
`fixed`). That is what `--still` is for.
