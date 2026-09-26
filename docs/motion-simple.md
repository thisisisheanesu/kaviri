# kaviri motion, the simple way

Write a file called `video.jsonl`. It has three kinds of lines, one JSON object per line.
kaviri does the animation, the transitions, the camera and the music. You only write words.

## The template

Copy this and change the words. Keep every line on one line.

```jsonl
{"op":"brand","name":"Acme","accent":"#5b8cff","theme":"dark","url":"acme.dev","music":"energetic"}
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
| `theme` | `"dark"` or `"light"` | `"dark"` |
| `url` | the website, shown at the end | |
| `logo` | a logo image file, shown at the end | a letter tile |
| `music` | `"energetic"`, `"cinematic"`, `"calm"` or `"none"` | `"energetic"` |

**`beat`**, one per idea, in order. Each beat is 4 seconds.

| field | what to put |
|---|---|
| `text` | the headline, **2 to 8 words**. Put the key words in square brackets to colour them: `"Every model. [One app.]"` |
| `sub` | optional: a smaller second line under the headline |
| `show` | optional: **one** thing to show under the headline, from the list below |

**`end`**, once, last. `tagline` is one short line. It shows the logo, the name, the
tagline and the url.

## What a beat can show

Pick one per beat. Leave `show` out for a big headline on its own.

| `show` | what it looks like |
|---|---|
| `{"image":"shot.png"}` | a screenshot in a browser window that flies in. Add `"frame":"none"` for no window |
| `{"type":"text to type"}` | a chat box that types the text and presses Send. Add `"chips":["GPT","Claude"]` for model tags |
| `{"code":"line one\nline two","title":"file.js"}` | a code editor that types the code out |
| `{"list":["Step one","Step two","Step three"]}` | a checklist that ticks each line |
| `{"stats":[["20,641+","customers"],["4.9","rating"]]}` | big numbers that count up. Two to four of them |
| `{"icons":["chat","code","ai","mail","terminal"]}` | icons orbiting in 3D with light trails. Names: `files browser mail chat music photos calendar notes settings terminal code camera video maps store ai game wallet device`; any other word becomes a letter tile |
| `{"chips":["Fast","Private","Free"]}` | labels that pop in one by one |
| `{"strike":["Old way one.","Old way two."]}` | lines that get crossed out |

## Rules that make it good

1. **5 to 7 beats.** The third beat is the big moment: the music drops there with a flash.
   Put the product reveal (usually an `image`) on beat 3.
2. **Short headlines.** One idea per beat. Colour one to three words with `[ ]`.
3. **Tell a story**: the problem, what people put up with, the reveal, how it works, proof, then
   the end card.
4. **Different shows on different beats.** Do not use the same `show` twice in a row.
5. File paths (`image`, `logo`) are relative to the `.jsonl` file.
6. Use `"theme":"light"` for a bright, paper look. Use `"size":"vertical"` for phones by adding
   `{"op":"video","size":"vertical"}` as the first line.

## Optional

- `"bars":1` on a beat makes it 2 seconds; `"bars":4` makes it 8.
- A full example is `examples/motion/simple.jsonl`.
- To check a file without rendering: `kaviri motion --script video.jsonl --check`.
- To look at a moment: `kaviri motion --script video.jsonl --still 9 --out look.png` (9 seconds in).
- Everything in the full format (`motion-llm.txt`) can be mixed in on extra lines.
