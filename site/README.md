# site/

The kaviri.dev landing page. One HTML file, one stylesheet, no framework, no build step and no
JavaScript. It is served as Cloudflare Workers static assets.

## Files

| file | what it is |
|---|---|
| `index.html` | the whole page |
| `style.css` | layout and type. Every colour goes through a `var()` |
| `favicon.svg` | the brand mark, copied from `brand/favicon.svg` |
| `worker.js` | the Worker in front of the asset store: caching, security headers, Range passthrough |
| `wrangler.toml` | Workers config, the `ASSETS` binding and the `kaviri.dev` custom domain |
| `.assetsignore` | keeps `worker.js` and `wrangler.toml` out of the public asset tree |
| `brand/tokens.css` | copied from `brand/tokens.css` in the repo root. Copied rather than linked because wrangler uploads this directory and nothing above it |

## What you have to copy in before deploying

One file is referenced by the page and is not in this directory, because it is made by hand.
Without it the page still renders, but link previews have no image.

| put it here | copy it from | notes |
|---|---|---|
| `site/og.png` | made by hand, 1200x630 | the Open Graph and Twitter card image |

## The demo video

`site/demo.mp4` and `site/demo-poster.jpg` are committed. The video is the motion hero,
"Screen Studio for your AI agent": `examples/motion/hero.jsonl`, kaviri's own takes of the
Parcel demo in five device frames, cut together with kaviri motion. To make it again, from the
repo root:

```sh
sh examples/motion/hero-takes.sh            # films the takes, renders hero.mp4
ffmpeg -i hero.mp4 -vf scale=1280:720:flags=lanczos -c:v libx264 -preset slow -crf 25 \
  -maxrate 1200k -bufsize 2400k -pix_fmt yuv420p -movflags +faststart \
  -c:a aac -b:a 96k site/demo.mp4
ffmpeg -ss 12 -i site/demo.mp4 -frames:v 1 -q:v 4 site/demo-poster.jpg
```

The page autoplays it muted and loops it, with controls so a visitor can turn the music on.
Keep it around 2.5MB. The `demo.mp4` path is deliberate: the page asks for `/demo.mp4`, so
whatever is at `site/demo.mp4` is what the world sees.

## Deploy

```sh
cd site
npx wrangler deploy
```

The first deploy asks to attach `kaviri.dev`. The zone is already on the account, so accept it
and Cloudflare provisions the certificate. To check the page before it is public:

```sh
npx wrangler dev
```

That serves the directory on `http://localhost:8787` with the same asset routing as production.

## Editing the copy

The positioning is settled and the hero line is not a draft:

> Your demo video is a build artifact. It changes when your product does, or the build fails.

House style applies to everything on the page. No em dashes. Plain, concrete, confident copy, no
marketing adjectives. The `.jsonl` script, the workflow YAML and the first-to-last frame check are
copied from the real repo, so if the syntax changes in `examples/demo.jsonl`, `action.yml` or
`.github/workflows/demo.yml`, change it here too rather than paraphrasing it.

## Keeping the tokens in step

`site/brand/tokens.css` is a copy, so it goes stale the moment `brand/tokens.css` changes. Re-copy
it as part of any brand change:

```sh
cp brand/tokens.css site/brand/tokens.css
cp brand/favicon.svg site/favicon.svg
```

`style.css` maps the brand's semantic names onto the ones this stylesheet was written against
(`--bg` to `--paper`, `--text` to `--ink`, `--border` to `--line`, `--text-muted` to `--muted`,
`--accent-contrast` to `--accent-ink`, and `--text` to `--code-ink`, which the brand does not
define). If the brand renames a token, that alias block at the top of `style.css` is the one place
to change.
