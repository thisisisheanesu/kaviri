# Prompts for motion videos

Give a model the format and a brief, and it writes the script. These are prompts that work.
Each assumes the model has the format: either attach
[`motion-llm.md`](https://kaviri.dev/motion-llm.txt) (download it, or point the model at the
URL), or paste it above the prompt.

The loop is the same every time: the model writes the `.jsonl`, runs `--check`, looks at
`--still` frames of the moments that matter, fixes what it sees, and renders. A model with a
shell does all of that itself. A model without one writes the script and you run the commands.

## The easiest prompt: any model, any size

Give it [`motion-simple.txt`](https://kaviri.dev/motion-simple.txt) (attach it, paste it, or
give it the URL) and say:

```text
Using the attached kaviri motion simple format, write video.jsonl for <product> (<url>).
Brand colour <#hex>, <dark or light> theme. 6 beats: the problem, what people put up with,
the product (show the screenshot <file.png>), how it works, proof, and an end card.
Output only the file.
```

That is enough for a small model to make a scored, animated video: it only chooses words and,
per beat, one thing to show. Everything else is decided by kaviri. Use the prompts below with
the full format when you want more control.

## The one-line setup

For an agent with a shell (Claude Code, Codex, Cursor and the like):

```text
Read https://kaviri.dev/motion-llm.txt. It is the format for kaviri motion, which renders a
JSONL timeline to an MP4 with music. Use it for the video I ask for next. Work in a loop:
write the script, run `kaviri motion --script <file> --check`, render stills at the key
moments with `--still` and look at them, fix what is wrong, then render the MP4.
```

For a chat model without a shell, attach `motion-llm.txt` and say:

```text
The attached file is the complete format for kaviri motion. Write one .jsonl script for the
video below. Output only the script, in one code block, and nothing after it.
```

## A launch showreel

```text
Make a dynamic 30-second motion graphics video introducing <product> (<url>): a showreel a
senior motion designer would be proud of. Real production, not a demo.

- 16:9, 1080p, 120 bpm, 16 bars. Music: intro 2 bars, build 2, drop 8, break 2, outro 2.
- One idea per scene, one or two bars each, every cut on a bar line, a different transition
  into each scene. The drop's first downbeat is the biggest moment: flash, shake, shockwave.
- Do not show screenshots. Rebuild the product from the ui components (window, input, list,
  code, cards, chips, cursor) so each part animates on its own: text types, toggles flip on
  the beat, cards stream in, a cursor clicks and the camera zooms to where it clicks.
- Big kinetic type with the key words in [accent], entering per letter (wave, converge, mask)
  and leaving with scatter or zoom. Logos orbit with trails and collapse into the brand mark.
- Nothing ever holds still: push the camera, float the chips, pulse the hero on the beat.
- Brand: <colours, font, logo file or SVG>. End on the mark, the name, one line, and the URL.
```

## A 15-second feed ad

```text
Write a 15-second 4:5 (1080x1350) ad for <product>, light theme, 124 bpm, 8 bars:
intro 1, build half a bar, drop 4.5, outro 2. Five beats of story, one headline each, pinned
at the top with the brand chip above it:
1. <the problem or the starting point>, shown in a phone mockup
2. <what the product does>, shown happening (a scan line, a before and after, a list filling in)
3. <proof of scale>, a tilted wall of results drifting upward
4. <proof of quality>, a carousel that snaps card to card on the beat, a rating line
5. a dark call to action: headline, a "no sign-up" chip, a button a cursor clicks, confetti.
Images: <paths to your images, relative to the script>.
```

## Rebuilding a screen as components

```text
Here is a screenshot of <screen>. Rebuild it in kaviri motion as ui components (a window with
the same sidebar items, the same inputs, lists, buttons and cards) rather than using the image.
Then animate it being used for 4 bars at 120 bpm: the panels fly in with a slight 3D tilt that
eases flat, the cursor moves to <control> and clicks on beat 5, <field> types "<text>", <toggle>
flips on beat 7, and the camera zooms to 1.5x on each interaction and back out.
```

## A changelog or release clip

```text
Make a 20-second release video for <product> <version>. 10 bars at 120 bpm: build 2, drop 6,
outro 2. One scene per headline feature (<feature 1>, <feature 2>, <feature 3>), each showing
the feature working with ui components and a two-to-four word headline. Open with the version
number scrambling in, close with "Update now" and the install command typing in a code block.
```

## Kinetic type only

```text
Make a 12-second kinetic typography piece of this line: "<line>". 128 bpm, 6 bars, minimal
style music. Break it into four beats of two to four words each, a different text effect for
each (wave, converge, mask, scramble), one accent colour, a background that moves slowly, and
a strike-through on "<word to cross out>" landing on a downbeat.
```

## When the result is not right yet

These follow-ups work well:

- "Render stills at 1b of every scene and at every drop. What is off-balance, overlapping or
  too small? Fix it."
- "Scene 3 is static. Add depth: tilt the panels, push the camera, float the chips, and give
  the hero a beat pulse."
- "The text is too long. One idea per scene, four words at most, key words in accent."
- "The cut into the drop does not hit hard enough. Use a flash transition, a shake, a shockwave
  and a burst, all on the downbeat."
- "Make it feel more expensive: fewer things at once, bigger type, slower camera, more space."

## Why a model can do this well

Every name in the format is checked. A misspelled effect, a missing scene, a parent declared
after its child or an act on a layer that does not exist is an error naming the line and the
real options, so a model corrects itself in one round instead of producing a video with a
hole in it. `--still` gives it eyes on any frame in a couple of seconds, and the musical
time units mean it never has to do arithmetic to hit a beat.
