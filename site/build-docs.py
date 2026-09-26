#!/usr/bin/env python3
"""Turn the repository's Markdown docs into pages on kaviri.dev.

The docs live next to the code in `docs/`, because that is where they get updated when the
code changes and a separate docs repository is a docs repository that goes stale. This script
is the one place that knows how to put them on the web, and it runs before a deploy rather
than at request time: a Worker that renders Markdown per request is a rendering cost on every
page view for content that changes when someone commits.

    python3 site/build-docs.py      # writes site/docs/

Everything it writes is generated, so `site/docs/` is not in the repository.
"""

import html
import os
import re
import sys

try:
    import markdown
except ImportError:  # pragma: no cover - the message is the whole value
    sys.exit("build-docs: needs the markdown package (pip install --user markdown)")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "site", "docs")

# Order matters: this is the order they appear in the index and in the footer of each page,
# and it is the order somebody reading all of them should read them in.
PAGES = [
    ("index", "README.md", "kaviri", "What it is, what a script looks like, and how to run one."),
    ("script-protocol", "docs/script-protocol.md", "Script protocol",
     "Every op, every field, and what each one does to the camera."),
    ("camera", "docs/camera.md", "The camera",
     "How a zoom and a pan are derived from interactions you never describe."),
    ("ci", "docs/ci.md", "In CI",
     "The GitHub Action, and filming an app the workflow starts itself."),
    ("troubleshooting", "docs/troubleshooting.md", "Troubleshooting",
     "What each error means, starting with the ones that produce a video rather than a failure."),
    ("motion", "docs/motion.md", "Motion graphics",
     "kaviri motion: a JSONL timeline rendered to a video with a soundtrack on its beat grid."),
    ("motion-simple", "docs/motion-simple.md", "Motion, the simple way",
     "A brand, one line per beat and an end card: kaviri picks the motion and the music."),
    ("motion-prompts", "docs/motion-prompts.md", "Motion prompts",
     "Prompts that get a model to write a good motion script, and the file to give it."),
    ("motion-llm", "docs/motion-llm.md", "Motion format",
     "The whole motion format in one page, written to be handed to a model."),
    ("agents", "AGENTS.md", "For agents",
     "The whole protocol for something that has been asked for a video and has a shell."),
]

SHELL = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} &middot; kaviri docs</title>
<meta name="description" content="{blurb}">
<link rel="icon" href="/favicon.svg">
<link rel="stylesheet" href="/brand/tokens.css">
<link rel="stylesheet" href="/style.css">
<link rel="stylesheet" href="/docs.css">
</head>
<body>
<header class="site-head wrap">
  <a class="wordmark" href="/">kaviri<span class="dot">.</span>dev</a>
  <nav>
    <a href="/docs/">Docs</a>
    <a href="/play/">Playground</a>
    <a href="https://github.com/thisisisheanesu/kaviri">GitHub</a>
  </nav>
</header>
<div class="wrap docs-shell">
  <nav class="docs-side" aria-label="Documentation">
    <p class="eyebrow">Docs</p>
    <ul>{side}</ul>
    <p class="eyebrow" style="margin-top:var(--k-space-3)">Plain text</p>
    <ul>
      <li><a href="/llms.txt">llms.txt</a></li>
      <li><a href="/llms-full.txt">llms-full.txt</a></li>
      <li><a href="/motion-simple.txt" download>motion-simple.txt</a></li>
      <li><a href="/motion-llm.txt" download>motion-llm.txt</a></li>
    </ul>
  </nav>
  <main class="docs-main">
{body}
    <hr class="rule">
    <p class="muted docs-foot">
      This page is generated from <code>{source}</code> in the repository, so it cannot drift
      from the code it documents without somebody noticing.
    </p>
  </main>
</div>
<footer class="site-foot wrap">
  <p class="pron">kaviri &middot; ka-VEE-ree</p>
  <p>ChiShona for twice, a second time. The demo is not re-recorded. It is made again.</p>
  <div class="foot-links">
    <a href="/">Home</a>
    <a href="/docs/">Docs</a>
    <a href="/play/">Playground</a>
    <a href="https://github.com/thisisisheanesu/kaviri">GitHub</a>
    <a href="mailto:hello@kaviri.dev">hello@kaviri.dev</a>
  </div>
</footer>
</body>
</html>
"""


def rewrite_links(body: str) -> str:
    """Point the repository's relative links at the published pages.

    A doc that says `docs/camera.md` is correct on GitHub and a 404 here. The mapping is the
    same list the index is built from, so a new page is wired up by adding one line to PAGES
    rather than by remembering to also edit a regex.
    """
    for slug, path, _, _ in PAGES:
        target = "/docs/" if slug == "index" else f"/docs/{slug}/"
        for spelling in (path, os.path.basename(path), "./" + path):
            body = body.replace(f'href="{spelling}"', f'href="{target}"')
    return body


def build():
    os.makedirs(OUT, exist_ok=True)
    md = markdown.Markdown(
        extensions=["fenced_code", "tables", "toc", "attr_list", "sane_lists"],
        output_format="html5",
    )

    written = []
    for slug, path, title, blurb in PAGES:
        src = os.path.join(ROOT, path)
        if not os.path.exists(src):
            print(f"build-docs: skipping {path}, which does not exist")
            continue
        text = open(src, encoding="utf-8").read()
        md.reset()
        body = rewrite_links(md.convert(text))

        side = "".join(
            f'<li{" class=here" if s == slug else ""}><a href="{"/docs/" if s == "index" else "/docs/" + s + "/"}">{html.escape(t)}</a></li>'
            for s, _, t, _ in PAGES
        )

        page = SHELL.format(
            title=html.escape(title),
            blurb=html.escape(blurb),
            side=side,
            body=body,
            source=html.escape(path),
        )
        # index.md becomes /docs/, everything else becomes /docs/<slug>, which the asset
        # store serves from <slug>/index.html with its trailing-slash handling.
        dest = os.path.join(OUT, "index.html") if slug == "index" else os.path.join(OUT, slug, "index.html")
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        open(dest, "w", encoding="utf-8").write(page)
        written.append((slug, len(page)))

    # The motion format as plain text, to download or hand a model by URL. Copied rather than
    # rendered: a model reads Markdown better than it reads our HTML.
    for doc, txt in (("motion-llm.md", "motion-llm.txt"), ("motion-simple.md", "motion-simple.txt")):
        src = os.path.join(ROOT, "docs", doc)
        if os.path.exists(src):
            dest = os.path.join(ROOT, "site", txt)
            open(dest, "w", encoding="utf-8").write(open(src, encoding="utf-8").read())
            print(f"  /{txt}  {os.path.getsize(dest):6} bytes")

    for slug, n in written:
        print(f"  /docs/{'' if slug == 'index' else slug + '/'}  {n:6} bytes")
    print(f"build-docs: {len(written)} pages into site/docs/")


if __name__ == "__main__":
    build()
