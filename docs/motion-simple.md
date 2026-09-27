# kaviri motion, the simple way

Write a file called `video.jsonl`. It has three kinds of lines, one JSON object per line.
kaviri does the animation, the transitions, the camera and the music. You only write words.

## The template

Copy this and change the words. Keep every line on one line.

```jsonl
{"op":"brand","name":"Acme","accent":"#5b8cff","style":"bold","url":"acme.dev"}
{"op":"beat","text":"The problem, in [four words.]"}
{"op":"beat","text":"What people put up with.","show":{"strike":["The first chore.","The second chore.","The third chore."]}}
{"op":"beat","text":"Meet [Acme.]","show":{"image":"screenshot.png"}}
{"op":"beat","text":"Ask for [anything.]","show":{"type":"Summarise this week's tickets"}}
{"op":"beat","text":"Loved by [teams.]","show":{"stats":[["20,641+","customers"],["4.9","rating"],["30s","to set up"]]}}
{"op":"end","tagline":"One line about [Acme.]"}
```

Then run:

```sh
kaviri motion --script video.jsonl --out video.mp4
```

## The three lines

**`brand`**, once, first.

| field | what to put | default |
|---|---|---|
| `name` | the product name | |
| `accent` | one brand colour, as `"#rrggbb"` | `"#5b8cff"` |
| `style` | the look, from the table below | `"bold"` |
| `theme` | `"dark"` or `"light"`, to override the style's | the style's |
| `url` | the website, shown at the end | |
| `logo` | a logo image file, shown at the end | a letter tile |
| `accent2` | a second colour, where a style uses a gradient | a violet |
| `intro` | `"logo"` to open on the logo alone for two seconds | none |
| `music` | `"energetic"`, `"cinematic"`, `"calm"` or `"none"` | the style's |
| `background` | `"nebula"`, `"mesh"`, `"grid"`, `"aurora"`, `"solid"`, `"gradient"` or `"flat"` | the style's |

**`beat`**, one per idea, in order. Each beat is 4 seconds.

| field | what to put |
|---|---|
| `text` | the headline, **2 to 8 words**. Put the key words in square brackets to colour them: `"Every model. [One app.]"` |
| `sub` | optional: a smaller second line under the headline |
| `show` | optional: **one** thing to show under the headline, from the list below, or a list of **two** to show side by side |
| `style` | optional: a different style for this beat only |
| `theme` | optional: `"light"` or `"dark"` for this beat only |

**`end`**, once, last. `tagline` is one short line. It shows the logo, the name, the
tagline and the url. `links` adds a row of words above, like a site's navigation:
`"links":["Docs","Pricing","Blog"]`.

## Styles

Pick the one that fits the product. Everything else stays the same.

| `style` | looks like | theme | music |
|---|---|---|---|
| `"bold"` | big kinetic type, starfield, flash and shockwave on the drop | dark | energetic |
| `"minimal"` | quiet fades and slides on a clean light background, no effects | light | calm |
| `"neon"` | glitch and scramble text, a glowing synthwave grid | dark | energetic |
| `"editorial"` | serif type, slow reveals, magazine calm, an iris on the drop | light | cinematic |
| `"playful"` | bouncy pops, soft colour background, confetti | light | energetic |
| `"cinematic"` | wide capitals, aurora light, letterbox bars, slow blur cuts | dark | cinematic |
| `"brutalist"` | huge black capitals on pure white, hard cuts, no decoration | light | energetic |
| `"luxury"` | spaced serif capitals, slow dissolves, a soft glow; use a gold accent | dark | cinematic |
| `"terminal"` | monospace text that types and scrambles in, glitch cuts; use a green accent | dark | energetic |
| `"hype"` | italic capitals that slam in, whip pans, a hard-hitting beat | dark | energetic |
| `"corporate"` | clean and trustworthy: gentle rises, tidy pushes | light | calm |
| `"retro"` | warm cream paper, heavy film grain, bouncing serif, iris wipes | light | calm |
| `"announcement"` | cream and forest-green scenes in turn, italic serif accent words, huge numbers; use a mint accent | both | energetic |
| `"launch"` | pure black, thin type, accent words in a gradient, odometer numbers, sparkles | dark | energetic |
| `"gallery"` | soft pastel light, words among floating screenshots, a camera gliding over the product | light | calm |

`kaviri motion --styles` prints this list.

## What a beat can show

Pick one per beat. Leave `show` out for a big headline on its own.

| `show` | what it looks like |
|---|---|
| `{"image":"shot.png"}` | a screenshot in a browser window that flies in. Add `"frame":"none"` for no window |
| `{"type":"text to type"}` | a chat box that types the text and presses Send. Add `"chips":["GPT","Claude"]` for model tags |
| `{"code":"line one\nline two","title":"file.js"}` | a code editor that types the code out |
| `{"list":["Step one","Step two","Step three"]}` | a checklist that ticks each line |
| `{"stats":[["20,641+","customers"],["4.9","rating"]]}` | big numbers that count up. Two to four of them |
| `{"icons":["chat","code","ai","mail","terminal"]}` | icons orbiting in 3D with light trails. Names: `files browser mail chat music photos calendar notes settings terminal code camera video maps store ai game wallet device`; any other word becomes a tile of its first letter, and an emoji (`"🌙"`, `"🎧"`) becomes a tile of that emoji, so any product can have icons |
| `{"chips":["Fast","Private","Free"]}` | labels that pop in one by one |
| `{"strike":["Old way one.","Old way two."]}` | lines that get crossed out |
| `{"number":"$400M","label":"Series F"}` | one huge number counting up to its value |
| `{"roll":["Seed","Series A","Series F"]}` | a list that rolls up like a slot machine and stops on the last line |
| `{"photo":"landscape.jpg"}` | a photo filling the whole frame, the headline in white on it |
| `{"stack":["a.jpg","b.jpg","c.jpg"]}` | photo cards fanned out, rising one by one |
| `{"clip":"take.mp4","label":"iOS"}` | a video (a kaviri recording, or any video) played across the beat |
| `{"devices":["ios.mp4","android.mp4","mac.mp4"],"labels":["iOS","Android","macOS"]}` | several videos side by side |
| `{"cycle":["developer.","designer.","founder."]}` | the headline as a lead-in ("To every"), then a big word that changes on the beat. Add `"around":["a.png","b.png"]` for pictures drifting at the edges |
| `{"wall":["a.png","b.png","c.png"]}` | a tilted wall of pictures behind the headline. Add `"stats":[["197","resources"]]` for big numbers over it |
| `{"float":["a.png","b.png","c.png"]}` | the headline in the middle with pictures floating in around it |

Options: `number` takes `"pad":true` (an odometer with dim zeros) and `"graph":true` (a line
climbing under it); `photo` takes a list for a slideshow; `image` takes `"pan":true` for a
camera that leans in and travels across the picture.

## Mix and match

- **Two things side by side:** make `show` a list of two, `"show":[{"stack":[…]},{"number":"$400M"}]`.
  Good pairs: `stack` + `number`, `roll` + `photo`, `type` + `stats`, `code` + `list`.
- **A different look for one beat:** add `"style":"terminal"` (any style) to that beat.
- **The other ground for one beat:** add `"theme":"light"` or `"theme":"dark"` to that beat.

Walkthroughs with pictures of every step: `docs/motion-walkthroughs.md`
(https://kaviri.dev/docs/motion-walkthroughs/).

## Rules that make it good

1. **5 to 7 beats.** The third `beat` line (not counting `brand`) is the big moment: the
   music drops there with a flash. Put the product reveal on it: an `image` of the product if
   you have one; if not, `type` (the product being used) or `icons` (what it works with), or a
   headline on its own like `"Meet [Acme.]"`.
2. **Short headlines.** One idea per beat. Colour one to three words with `[ ]`.
3. **Tell a story**: the problem, what people put up with, the reveal, how it works, proof, then
   the end card.
4. **Different shows on different beats.** Do not use the same `show` twice in a row.
5. File paths (`image`, `logo`) are relative to the `.jsonl` file.
6. Use `"theme":"light"` for a bright, paper look. For TikTok, Reels or Shorts add
   `{"op":"video","size":"tiktok"}` as the first line (a 1080x1920 video; `"bpm":140` in the
   same line makes it quicker). Long headlines wrap by themselves; keep code lines under
   about 30 characters so they read on a phone. `examples/motion/tiktok.jsonl` is one.

## Checking your file

`--check` prints one JSON line. `ok` is `true` when the file is valid; otherwise the error names
the line and lists the allowed values, so fix that line and run it again. `duration` is the
video length in seconds and `scenes` lists each beat as `beat1`, `beat2`… with its start time,
then `end`.

Fields you set on `brand` (`theme`, `background`, `music`) always win over the style's choice.

## Variations and templates

- **Random choices:** any value can be `{"$pick":["one","two","three"]}`; the video uses one of
  them. `"$weights":[3,1,1]` makes the first more likely. Whole `show`s and styles can be picked.
- **Optional lines:** `"$maybe":0.5` on a line keeps it half the time.
- **Which variant:** `--seed 3` picks variant 3, and the same seed always gives the same video.
  `--variants 4` renders variants 1 to 4 (`out-v1.mp4` …) to compare.
- **Templates:** `{"op":"vars","product":"Relay"}` sets defaults, `{{product}}` uses them in any
  text, and `--var product=Beacon` changes them from the command line. One file, many launches.

## Optional

- `"bars":1` on a beat makes it 2 seconds; `"bars":4` makes it 8.
- A full example is `examples/motion/simple.jsonl`.
- To check a file without rendering: `kaviri motion --script video.jsonl --check`.
- To look at moments: `kaviri motion --script video.jsonl --still 2,10,18 --out look.png` writes
  one PNG per time (seconds; `look-00-2.00s.png`, …). Each beat starts 4 seconds after the
  last, so beat 3 is around 9 seconds in. Stills take a few seconds.
- Rendering the video takes a few minutes: roughly 2 to 10 frames a second depending on the
  machine, so a 30 second video is about 1 to 7 minutes. It prints progress while it works,
  and the file only appears when it is complete.
- To see a style before choosing, change `"style"` and render one still.
- Everything in the full format (`motion-llm.txt`) can be mixed in on extra lines.
