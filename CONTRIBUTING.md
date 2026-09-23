# Contributing to kaviri

kaviri is one Rust binary that drives a headless browser over the Chrome
DevTools Protocol and renders what happened into an MP4. Patches are welcome.
This file is what you need before you send one.

## Build and test

```sh
cargo build                 # debug build
cargo test                  # the full suite
cargo build --release       # what actually gets shipped
./target/release/kaviri record --script examples/form.jsonl --out demo.mp4
```

You need:

- Rust 1.75 or newer. The floor is `rust-version` in `Cargo.toml`, and it is
  deliberately low so that a distribution's rustc can build kaviri. Do not
  raise it by reaching for a new `std` method.
- A Chromium or Chrome binary on `PATH`, or `$KAVIRI_CHROMIUM`.
- `ffmpeg` built with libx264, on `PATH` or `$KAVIRI_FFMPEG`.

`kaviri doctor` prints what both of those resolve to on your machine, which is
the fastest way to find out why a test is failing.

Two tests shell out to a real ffmpeg, to check the generated filter graph and
the hand-rolled PNG writer against the actual encoder rather than against an
assumption. If you do not have ffmpeg and want the rest of the suite anyway,
set `KAVIRI_SKIP_FFMPEG_TESTS=1`. CI leaves it unset on purpose, so a missing
encoder is a red build there instead of two quietly skipped tests.

Before you open a pull request, run what CI runs:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

Clippy warnings are errors. `--locked` means `Cargo.lock` is part of the
change: if your patch adds a dependency, commit the updated lock file, and
add the crate to `THIRD-PARTY.md` with its licence in the same commit. A
dependency that arrives without its notice is a licensing bug, and it is much
cheaper to catch in review than in a customer's legal review.

Environment variables are read through `src/env.rs`, never through
`std::env::var` directly, because every variable has to accept both the
`KAVIRI_` prefix and the older `LENSA_` one. Use `env::var("CHROMIUM")` and
pass the suffix only.

## House style

Non negotiable, because consistency here is cheaper than taste:

- **No em dashes.** Not in code, comments, copy, docs, commit messages or
  error strings. A comma, a full stop or a colon does the job.
- **Comments explain why, in full sentences.** A comment that restates the
  line below it is noise that has to be maintained. A comment that records
  the constraint, the bug, or the platform quirk that forced the code into
  its shape is the most valuable thing in the file. Look at the comments in
  `Cargo.toml` and `.github/workflows/ci.yml` for the register.
- **Copy is plain, concrete and confident.** No marketing adjectives. Nothing
  is seamless, effortless or revolutionary. State what the thing does and what
  it costs you. This applies to `--help` output, error messages and the README
  equally, because all three are read by someone who is already annoyed.
- **Match the surrounding style of whatever file you are in.** If a module
  spells something a particular way, spell it that way too, and change both or
  neither.

Error messages get a specific rule: say what failed, what kaviri was trying to
do, and what the reader can do next. `ffmpeg not found` is a bad message.
`kaviri: no ffmpeg on PATH; set $KAVIRI_FFMPEG or install one, then run kaviri
doctor` is the standard.

## What makes a good pull request

- One change per pull request. A rename and a bug fix in the same diff cost
  the reviewer twice.
- A test for anything that is a behaviour. The suite is fast and it is the
  only reason this codebase can be refactored at speed.
- A note in `CHANGELOG.md` under `[Unreleased]` for anything a user would
  notice: a flag, an op, a default, an environment variable, an output path.
- If you changed what a take looks like, say so in the pull request and
  include a frame. Zoom and pan are judged by eye and the tests cannot do it.

Open an issue first for anything large, a new op, a new output format, a new
dependency, so that the design conversation happens before you have written
it.

## The repository boundary

kaviri is three codebases, and code flows in one direction only:

```
recorder  ->  cloud  ->  billing
```

- **recorder** is this repository. Apache-2.0, public, no account, no network
  call home. It runs entirely on your machine and knows nothing about the
  hosted service.
- **cloud** is the hosted service. It depends on the recorder, runs takes on
  its own machines and stores the output.
- **billing** depends on the cloud. Nothing depends on billing.

Never backwards. The recorder must not import, call, assume or check anything
from the cloud or from billing, and it must not acquire a licence check, a
telemetry requirement or a feature that only works when signed in. That rule
is what makes "Apache-2.0, unconditionally" a statement someone can rely on
when they put kaviri into their own build pipeline. A patch that crosses the
boundary the wrong way will be rejected however good it is, so raise it as an
issue first and it can be solved on the correct side.

## Licence and the CLA

The recorder is Apache-2.0. Contributions are accepted under that licence,
plus a contributor licence agreement: see [CLA.md](CLA.md). Sign it once, and
it covers everything you send afterwards.

kaviri takes a CLA rather than a DCO, and the honest reason is a commercial
one: the project sells commercial licences and a hosted service built over
this code, and a DCO certifies the origin of a contribution without granting
the rights needed to do either. The CLA is deliberately short, it leaves you
owning your copyright, and it is written to be read rather than clicked past.

By contributing you agree that your contribution is licensed under
Apache-2.0 and under the terms of the CLA.
