# kaviri brand

The design system for kaviri.dev and the docs. Everything visible comes from
`tokens.css`. If a page needs a colour or a size that is not a token, the token
is missing, not the page.

## The name

**kaviri**, pronounced **ka-VEE-ree**. ChiShona for "twice, a second time".

Always lowercase, even at the start of a sentence, and never capitalised as
"Kaviri" in headings or nav. It is not an acronym, so no full caps.

The name is the thesis: a demo video goes stale the moment the UI moves, because
re-recording it means blocking out an afternoon. kaviri makes the video a build
artifact. The script lives next to the code it films, so the demo is made again
when the product changes. Not re-recorded. Remade.

## The one-liner

> Your demo video is a build artifact. It changes when your product does, or the
> build fails.

Lead with the failing build, never with the zoom. The zoom is a feature; the
failing build is the reason anyone cares. The second line, when a second line is
needed:

> kaviri records agents, not people. The camera follows the text caret, because
> there is no hand on a mouse.

## Colour

One accent, and it is **ember, `#c7361a`** on paper, `#ff6b3d` on near-black.

Why this one: it is the colour of a take that is running right now, it is not
the blue or violet that every developer tool defaults to, and at 5.3:1 on white
it is dark enough to set text rather than only decorate.

The accent marks the thing that changed. Use it for:

- the primary action, one per view
- a failing build, a diff marker, the second of two frames
- the mark's second square, and nothing else in the logo
- inline links in prose

Do not use it for: page backgrounds, large filled panels, headings, borders on
things that are merely present, gradients, hover states on everything, icon
fills, or "making a section feel important". If two accent elements are visible
at once and only one of them is the point, one of them is wrong.

Everything else is ink on paper. Warm neutrals, not blue-grey. Structure comes
from borders and whitespace, not from tinted boxes.

Success green (`--ok`) and failure (`--fail`) are status, not branding. They
appear in build output and nowhere else.

## Type

Inter for everything, JetBrains Mono for anything that is typed at or produced by
a machine: commands, JSON ops, file paths, env vars, log lines. Both are loaded
from the system first and fall back through the stacks in `tokens.css`.

- Display sizes take `--tracking-display`. Large type at zero tracking looks
  loose.
- The tracked-out `--text-2xs` caps label is for the eyebrow above a section
  heading. One per section, at most.
- Body copy holds to `--measure` (68 characters). Prose pages use
  `--measure-narrow`.
- Weights are 400, 500 and 600. There is no 700 and no italic in the UI.

## Shape and depth

Square by default. `--radius-1` on buttons and inputs, `--radius-2` on cards,
`--radius-3` on video and screenshots, and that is the whole vocabulary. Nothing
is a pill except a status chip.

Borders carry structure. Shadows are only for things that genuinely float above
the page: a menu, a dialog, a video that sits proud of its section. A card that
sits in the flow gets a border and no shadow.

No gradients as decoration. No stock illustration. No 3D mockups of a laptop. If
a section needs a picture, the picture is a frame of real output from kaviri.

## Files

| File | Use |
| --- | --- |
| `tokens.css` | The system. Import it first, before any page CSS. |
| `logo.svg` | Mark plus wordmark, for light backgrounds. |
| `logo-dark.svg` | The same lockup for dark backgrounds. |
| `logomark.svg` | Mark alone. Ink is `currentColor`, so it inherits. |
| `favicon.svg` | Tiled mark for the browser tab, inverts in dark mode. |
| `preview.html` | Open it to see the system. Not shipped. |

### The mark

Two takes of the same frame: a solid square, and the same square again, offset
and drawn in the accent. It is not a camera, not a play button and not a lens,
because every competitor already owns those and none of them mean "again".

Clear space on every side is the height of the solid square. Minimum size is
16px for the mark and 96px wide for the full lockup. Do not recolour the second
square, do not separate the two squares, do not set the lockup on a photograph,
and do not add a tagline inside the lockup.

### The .ico

`favicon.svg` covers every current browser. Ship a `favicon.ico` beside it for
old Safari, pinned tabs and feed readers, containing **16, 32 and 48px** frames
in one file. The 48px frame is what Windows shortcuts and some readers pick up.

```sh
# From the SVG, light variant, since an .ico cannot switch on theme.
for s in 16 32 48; do
  rsvg-convert -w $s -h $s favicon.svg -o /tmp/favicon-$s.png
done
magick /tmp/favicon-16.png /tmp/favicon-32.png /tmp/favicon-48.png favicon.ico
```

Check the 16px frame by eye before shipping it. If the two squares merge into a
smudge, thicken the second square's stroke in a 16px-only copy rather than
shrinking the geometry further. Apple touch icon is a 180px PNG of the same tile.

## Words

Use: record, take, script, op, build artifact, remade, caret, agent, headless,
frame, zoom, pan, ffmpeg, CDP. A run is a **take**. The JSON file is a
**script**. The thing that reads it is the **recorder**.

Do not use: seamless, effortless, revolutionise, magical, unleash, supercharge,
delightful, "AI-powered", "next-generation", "game changer". Do not call the
output "beautiful"; say what it is and let someone watch it.

No em dashes anywhere, in copy, code, comments or docs. Commas, full stops and
the occasional colon do the work.

Say what it costs. kaviri needs Chromium and ffmpeg on the machine, it is Linux
and macOS only today, and Windows is untested. That belongs on the page, not
buried in the docs.

## Licence and trademark

The recorder is Apache 2.0, unconditionally. Apache 2.0 grants copyright and
patent rights and withholds trademark rights, which is the shape wanted: anyone
can fork and ship the code, nobody can ship it under this name and mark. These
brand assets are not covered by that licence. A fork renames.
