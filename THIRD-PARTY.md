# Third-party notices

lensa links a small set of Rust crates into its binary. This file lists them,
the licence each is used under, and the copyright notices those licences
require to travel with the software.

It also states what lensa does **not** bundle, which is the more important
half for anyone selling or redistributing a build. See
[Not bundled: ffmpeg and Chromium](#not-bundled-ffmpeg-and-chromium).

lensa's own licence is a separate question and is not settled yet. See
[LICENSE](LICENSE).

## How to read this

Every crate below is available under a permissive licence. Where a crate is
offered under a choice of licences, the "used under" column names the one
lensa elects. lensa elects **MIT** wherever MIT is on offer, because MIT is
the shortest obligation to discharge: reproduce the notice. Where MIT is not
on offer, lensa uses Apache-2.0 and carries its NOTICE obligation.

Nothing here is copyleft. No crate in the tree is GPL, LGPL, MPL, AGPL, or
SSPL, and none carries a field-of-use or non-commercial restriction. A binary
built from this tree may be sold.

## Obligations, in practice

Shipping a lensa binary to anyone outside the organisation means shipping
this file with it, or an equivalent notices document, because:

- **MIT** requires that the copyright notice and permission notice accompany
  every copy of the software, including a binary-only copy.
- **Apache-2.0** section 4(d) requires that any NOTICE file carried by a
  dependency be reproduced in the distribution. None of the dependencies
  below ships a NOTICE file today, so the obligation reduces to attribution
  plus a copy of the licence text, but this must be rechecked whenever a
  dependency is added or upgraded.
- **Unlicense** (byteorder, memchr, offered alongside MIT) imposes nothing.
  Both are also available under MIT and lensa elects MIT for consistency.
- **Unicode-3.0** applies to the Unicode data tables inside `unicode-ident`
  and requires its own notice, reproduced in full at the end of this file.

The practical shape of that: put this file next to the binary in the release
archive, and link it from the download page.

## Direct dependencies

These are the crates named in `Cargo.toml`.

| Crate | Version | Used under | Available under |
|---|---|---|---|
| `tungstenite` | 0.24 | MIT | MIT OR Apache-2.0 |
| `serde` | 1 | MIT | MIT OR Apache-2.0 |
| `serde_json` | 1 | MIT | MIT OR Apache-2.0 |
| `base64` | 0.22 | MIT | MIT OR Apache-2.0 |
| `signal-hook` | 0.3 | MIT | Apache-2.0 OR MIT |
| `libc` | 0.2 | MIT | MIT OR Apache-2.0 |

## Transitive dependencies

Pulled in by the crates above. Build-time-only crates (the proc-macro chain
behind `serde_derive` and `thiserror`) are listed too, because their code
does influence the shipped binary even though the crates themselves are not
linked into it.

| Crate | Version | Used under | Available under |
|---|---|---|---|
| `block-buffer` | 0.10 | MIT | MIT OR Apache-2.0 |
| `byteorder` | 1.5 | MIT | Unlicense OR MIT |
| `bytes` | 1.12 | MIT | MIT |
| `cfg-if` | 1.0 | MIT | MIT OR Apache-2.0 |
| `cpufeatures` | 0.2 | MIT | MIT OR Apache-2.0 |
| `crypto-common` | 0.1 | MIT | MIT OR Apache-2.0 |
| `data-encoding` | 2.11 | MIT | MIT |
| `digest` | 0.10 | MIT | MIT OR Apache-2.0 |
| `generic-array` | 0.14 | MIT | MIT |
| `getrandom` | 0.2 | MIT | MIT OR Apache-2.0 |
| `http` | 1.5 | MIT | MIT OR Apache-2.0 |
| `httparse` | 1.10 | MIT | MIT OR Apache-2.0 |
| `itoa` | 1.0 | MIT | MIT OR Apache-2.0 |
| `log` | 0.4 | MIT | MIT OR Apache-2.0 |
| `memchr` | 2.8 | MIT | Unlicense OR MIT |
| `ppv-lite86` | 0.2 | MIT | MIT OR Apache-2.0 |
| `proc-macro2` | 1.0 | MIT | MIT OR Apache-2.0 |
| `quote` | 1.0 | MIT | MIT OR Apache-2.0 |
| `rand` | 0.8 | MIT | MIT OR Apache-2.0 |
| `rand_chacha` | 0.3 | MIT | MIT OR Apache-2.0 |
| `rand_core` | 0.6 | MIT | MIT OR Apache-2.0 |
| `serde_core` | 1.0 | MIT | MIT OR Apache-2.0 |
| `serde_derive` | 1.0 | MIT | MIT OR Apache-2.0 |
| `sha1` | 0.10 | MIT | MIT OR Apache-2.0 |
| `signal-hook-registry` | 1.4 | MIT | Apache-2.0 OR MIT |
| `syn` | 2.0, 3.0 | MIT | MIT OR Apache-2.0 |
| `thiserror` | 1.0 | MIT | MIT OR Apache-2.0 |
| `thiserror-impl` | 1.0 | MIT | MIT OR Apache-2.0 |
| `typenum` | 1.20 | MIT | MIT OR Apache-2.0 |
| `unicode-ident` | 1.0 | MIT, and Unicode-3.0 for the data tables | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `utf-8` | 0.7 | MIT | MIT OR Apache-2.0 |
| `version_check` | 0.9 | MIT | MIT OR Apache-2.0 |
| `wasi` | 0.11 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| `zerocopy` | 0.8 | MIT | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zerocopy-derive` | 0.8 | MIT | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zmij` | 1.0 | MIT | MIT OR Apache-2.0 |

`wasi` is reached only through `getrandom` on the `wasm32-wasi` target, which
lensa does not build for; it is listed for completeness because it appears in
`Cargo.lock`.

## Copyright notices

MIT requires these to accompany the software. They are reproduced from the
`LICENSE-MIT` file each crate publishes.

- `base64`: Copyright (c) 2015 Alice Maz
- `block-buffer`, `cpufeatures`, `crypto-common`, `digest`, `sha1`:
  Copyright (c) 2017-2024 The RustCrypto Project Developers
- `byteorder`, `memchr`: Copyright (c) 2015 Andrew Gallant
- `bytes`: Copyright (c) 2018 Carl Lerche
- `cfg-if`: Copyright (c) 2014 Alex Crichton
- `data-encoding`: Copyright (c) 2015-2020 Julien Cretin,
  Copyright (c) 2017-2020 Google Inc.
- `generic-array`: Copyright (c) 2015 Bartłomiej Kamiński
- `getrandom`, `rand`, `rand_chacha`, `rand_core`:
  Copyright (c) 2018 Developers of the Rand project,
  Copyright (c) 2014 The Rust Project Developers
- `http`: Copyright (c) 2017 http-rs authors
- `httparse`: Copyright (c) 2015-2024 Sean McArthur
- `itoa`, `proc-macro2`, `quote`, `syn`, `thiserror`, `thiserror-impl`,
  `unicode-ident`, `zmij`: Copyright (c) David Tolnay
  (`proc-macro2` also Copyright (c) 2014 Alex Crichton)
- `libc`, `log`: Copyright (c) 2014-2020 The Rust Project Developers
- `ppv-lite86`: Copyright (c) 2019 The CryptoCorrosion Contributors
- `serde`, `serde_core`, `serde_derive`, `serde_json`:
  Copyright (c) 2014 Erick Tryzelaar, Copyright (c) 2018 David Tolnay
- `signal-hook`, `signal-hook-registry`:
  Copyright (c) 2017 tokio-rs and signal-hook developers (Michal 'vorner' Vaner)
- `tungstenite`: Copyright (c) 2017 Alexey Galakhov,
  Copyright (c) 2016 Jason Housley
- `typenum`: Copyright (c) 2014 Paho Lurie-Gregg
- `utf-8`: Copyright (c) 2014 Simon Sapin
- `version_check`: Copyright (c) 2017-2018 Sergio Benitez
- `wasi`: Copyright (c) 2019 The Cranelift Project Developers
- `zerocopy`, `zerocopy-derive`: Copyright (c) 2023 The Fuchsia Authors

Each of those notices accompanies the standard MIT permission text. Rather
than reproduce twenty-odd byte-identical copies of the MIT licence here, the
release archive carries one copy of the MIT text; the notices above name the
holders it applies to. Where a crate's own `LICENSE-MIT` differs from the
canonical text, that crate's file governs.

### Unicode-3.0 notice, for `unicode-ident`

`unicode-ident` embeds tables derived from the Unicode Character Database,
which carry the Unicode licence in addition to the crate's own MIT or
Apache-2.0 terms:

> Copyright © 1991-2023 Unicode, Inc. All rights reserved.
>
> Permission is hereby granted, free of charge, to any person obtaining a
> copy of data files and any associated documentation (the "Data Files") or
> software and any associated documentation (the "Software") to deal in the
> Data Files or Software without restriction, including without limitation
> the rights to use, copy, modify, merge, publish, distribute, and/or sell
> copies of the Data Files or Software, and to permit persons to whom the
> Data Files or Software are furnished to do so, provided that either (a)
> this copyright and permission notice appear with all copies of the Data
> Files or Software, or (b) this copyright and permission notice appear in
> associated Documentation.
>
> THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
> KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
> MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
> THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS
> INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT
> OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF
> USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
> OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
> PERFORMANCE OF THE DATA FILES OR SOFTWARE.

## Not bundled: ffmpeg and Chromium

lensa does not contain, link against, vendor, redistribute or download
ffmpeg or Chromium. It discovers both on the host at run time and invokes
them as separate processes:

- **ffmpeg** is located by `$LENSA_FFMPEG`, then by `ffmpeg` on `PATH`, then
  by a short list of conventional install locations. lensa spawns it as a
  child process and pipes frames to its stdin. No ffmpeg code is linked in,
  no ffmpeg headers are used, and no ffmpeg binary ships in the release
  archive.
- **Chromium** (or Chrome) is located by `$LENSA_CHROMIUM`, then by
  `chromium` / `google-chrome` on `PATH`, then by conventional locations.
  lensa spawns it and speaks the Chrome DevTools Protocol to it over a
  websocket. The protocol is a wire format; lensa contains no Chromium code.

### Why this matters for licensing

This is the single design decision that keeps lensa sellable without a
lawyer, and it must stay true.

ffmpeg is LGPL-2.1-or-later by default, and GPL-2.0-or-later when built with
`--enable-gpl`, which is what almost every distribution and static build ships
because that is how libx264 gets in. libx264 itself is GPL and separately
patent-encumbered. If lensa linked ffmpeg's libraries, or shipped an ffmpeg
binary in the same distribution unit, the copyleft obligations would attach to
lensa's own distribution: at minimum an LGPL relinking obligation, and under a
GPL build an argument that lensa is a derivative work that must itself be
GPL. That is incompatible with selling a proprietary binary.

Spawning a separate process and communicating over a pipe is the recognised
arm's-length arrangement. The user supplies their own ffmpeg, under whatever
terms they obtained it, and lensa is a program that happens to run it in the
same way a shell script does. The same argument covers Chromium, which is
BSD-3-Clause at its core but carries an LGPL component set and a bundle of
other licences that nobody wants to inherit.

Consequently:

- **Do not** add an ffmpeg or Chromium binary to the release archive, the
  installer, the container image, or a Homebrew formula that vendors either.
  Depending on them as separate packages is fine; that is what a package
  manager dependency is for.
- **Do not** replace the subprocess with an `ffmpeg-sys` / `ffmpeg-next`
  binding, however much simpler the frame pipeline would be. That single
  change converts lensa into a derivative work of ffmpeg.
- **Do** keep the README's statement that both are discovered at run time and
  are the user's own software under their own licences.
- **Do** tell users of H.264 output that libx264 and AVC carry patent
  considerations in some jurisdictions, and that this is their encoder and
  their distribution decision, not lensa's.

## Keeping this file honest

This file was assembled by hand from `Cargo.toml` and `Cargo.lock`. Two
things must be automated before release, because a hand-written notices file
goes stale on the first `cargo update`:

1. Generate it. `cargo about generate` produces this document from the real
   resolved graph and the real `LICENSE-*` files inside each `.crate`,
   including the exact copyright lines, which is what MIT actually asks for.
   The notices above should be replaced by that output; they are reproduced
   from the crates' published licence files and are correct to the best of a
   manual check, but a generator does not misremember.
2. Enforce it. `cargo deny check licenses` with an allowlist of
   MIT / Apache-2.0 / Unlicense / BSD-2-Clause / BSD-3-Clause / Unicode-3.0,
   wired into CI, so a future dependency carrying a copyleft or
   source-available licence fails the build rather than being discovered by a
   customer's legal review.

Note also that `Cargo.lock` in the tree predates the addition of
`signal-hook` and `libc` as direct dependencies, so it does not yet list
`signal-hook` or `signal-hook-registry`. Regenerate the lock and re-check this
file once the crate builds.
