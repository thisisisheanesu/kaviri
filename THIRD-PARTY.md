# Third-party notices

kaviri compiles a small set of Rust crates into its binary. This file lists
them, the licence each is used under, and the copyright notices those licences
require to travel with the software.

It also states what kaviri does **not** bundle, which is the half that decides
whether a build can be sold or shipped inside a closed product. See
[Not bundled: ffmpeg and Chromium](#not-bundled-ffmpeg-and-chromium).

kaviri's own licence is a separate question and is settled: the recorder is
Apache-2.0, unconditionally. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

## How to read this

Every crate below is available under a permissive licence. Where a crate is
offered under a choice, the "used under" column names the one kaviri elects.
kaviri elects **MIT** wherever MIT is on offer, because MIT is the shortest
obligation to discharge: reproduce the notice.

Nothing here is copyleft. No crate in the tree is GPL, LGPL, MPL, AGPL or
SSPL, and none carries a field-of-use or non-commercial restriction. A binary
built from this tree may be sold, and may be embedded in a product that is not
itself open source.

## What you owe, in practice

Shipping a kaviri binary to anyone outside your own organisation means
shipping this file with it, or an equivalent notices document, because:

- **MIT** requires the copyright notice and the permission notice to
  accompany every copy of the software. A binary-only copy is still a copy.
  This is the obligation people most often miss, because nothing in the build
  reminds them.
- **Apache-2.0** section 4(d) requires that any NOTICE file carried by a
  dependency be reproduced in the distribution. None of the crates below ships
  a NOTICE file today, so the obligation reduces to attribution plus a copy of
  the licence text. Recheck that whenever a dependency is added or upgraded.
- **Unlicense** (byteorder, memchr, both also offered under MIT) imposes
  nothing. kaviri elects MIT for them anyway, so there is one obligation to
  discharge rather than two shapes of one.
- **Unicode-3.0** applies to the data tables inside `unicode-ident` and needs
  its own notice, reproduced in full below.

The practical shape of that: put this file next to the binary in the release
archive, and link it from the download page.

## Direct dependencies

The crates named in `Cargo.toml`.

| Crate | Version | Used under | Available under |
|---|---|---|---|
| `tungstenite` | 0.24.0 | MIT | MIT OR Apache-2.0 |
| `serde` | 1.0.229 | MIT | MIT OR Apache-2.0 |
| `serde_json` | 1.0.151 | MIT | MIT OR Apache-2.0 |
| `base64` | 0.22.1 | MIT | MIT OR Apache-2.0 |
| `signal-hook` | 0.3.18 | MIT | Apache-2.0 OR MIT |
| `libc` | 0.2.189 | MIT | MIT OR Apache-2.0 |

## Transitive dependencies, linked into the binary

Pulled in by the crates above, mostly by `tungstenite`'s handshake feature
(the websocket key is a SHA-1 of a random nonce, which is where `sha1`, `rand`
and `digest` come from).

| Crate | Version | Used under | Available under |
|---|---|---|---|
| `block-buffer` | 0.10.4 | MIT | MIT OR Apache-2.0 |
| `byteorder` | 1.5.0 | MIT | Unlicense OR MIT |
| `bytes` | 1.12.1 | MIT | MIT |
| `cfg-if` | 1.0.4 | MIT | MIT OR Apache-2.0 |
| `cpufeatures` | 0.2.17 | MIT | MIT OR Apache-2.0 |
| `crypto-common` | 0.1.7 | MIT | MIT OR Apache-2.0 |
| `data-encoding` | 2.11.1 | MIT | MIT |
| `digest` | 0.10.7 | MIT | MIT OR Apache-2.0 |
| `errno` | 0.3.14 | MIT | MIT OR Apache-2.0 |
| `generic-array` | 0.14.7 | MIT | MIT |
| `getrandom` | 0.2.17 | MIT | MIT OR Apache-2.0 |
| `http` | 1.5.0 | MIT | MIT OR Apache-2.0 |
| `httparse` | 1.10.1 | MIT | MIT OR Apache-2.0 |
| `itoa` | 1.0.18 | MIT | MIT OR Apache-2.0 |
| `log` | 0.4.34 | MIT | MIT OR Apache-2.0 |
| `memchr` | 2.8.3 | MIT | Unlicense OR MIT |
| `ppv-lite86` | 0.2.21 | MIT | MIT OR Apache-2.0 |
| `rand` | 0.8.8 | MIT | MIT OR Apache-2.0 |
| `rand_chacha` | 0.3.1 | MIT | MIT OR Apache-2.0 |
| `rand_core` | 0.6.4 | MIT | MIT OR Apache-2.0 |
| `serde_core` | 1.0.229 | MIT | MIT OR Apache-2.0 |
| `sha1` | 0.10.7 | MIT | MIT OR Apache-2.0 |
| `signal-hook-registry` | 1.4.8 | MIT | MIT OR Apache-2.0 |
| `thiserror` | 1.0.69 | MIT | MIT OR Apache-2.0 |
| `typenum` | 1.20.1 | MIT | MIT OR Apache-2.0 |
| `utf-8` | 0.7.6 | MIT | MIT OR Apache-2.0 |
| `zerocopy` | 0.8.56 | MIT | BSD-2-Clause OR Apache-2.0 OR MIT |
| `zmij` | 1.0.23 | MIT | MIT |

Three crates in `Cargo.lock` are gated to targets kaviri does not currently
support, so they are resolved but never compiled into a Linux or macOS build.
They are listed for completeness, and their notices become due the day a
Windows or WASI build ships.

| Crate | Version | Used under | Available under | Target |
|---|---|---|---|---|
| `wasi` | 0.11.1 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | wasm32-wasi |
| `windows-link` | 0.2.1 | MIT | MIT OR Apache-2.0 | windows |
| `windows-sys` | 0.61.2 | MIT | MIT OR Apache-2.0 | windows |

## Build-time only

These run inside the compiler as proc macros or build scripts and produce
code. None of them is linked into the shipped binary, so a binary-only
distribution does not owe their notices. A source distribution does, and
`cargo vendor` output does, so they are listed.

| Crate | Version | Used under | Available under |
|---|---|---|---|
| `proc-macro2` | 1.0.107 | MIT | MIT OR Apache-2.0 |
| `quote` | 1.0.47 | MIT | MIT OR Apache-2.0 |
| `serde_derive` | 1.0.229 | MIT | MIT OR Apache-2.0 |
| `syn` | 2.0.119 | MIT | MIT OR Apache-2.0 |
| `syn` | 3.0.4 | MIT | MIT OR Apache-2.0 |
| `thiserror-impl` | 1.0.69 | MIT | MIT OR Apache-2.0 |
| `unicode-ident` | 1.0.24 | MIT and Unicode-3.0 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `version_check` | 0.9.5 | MIT | MIT OR Apache-2.0 |
| `zerocopy-derive` | 0.8.56 | MIT | BSD-2-Clause OR Apache-2.0 OR MIT |

`unicode-ident` is the one crate whose licence is an AND rather than an OR.
The Rust code is MIT or Apache-2.0 at your election, but the Unicode character
tables it embeds are covered by Unicode-3.0 regardless of that election, so
both notices are owed.

## Copyright notices

Reproduced from each crate's own licence file. Crates whose licence file
carries no copyright line (the dtolnay crates, among others) are attributed to
their authors as published on crates.io.

```
base64                 Copyright (c) 2015 Alice Maz
block-buffer           Copyright (c) 2018-2019 The RustCrypto Project Developers
byteorder              Copyright (c) 2015 Andrew Gallant
bytes                  Copyright (c) 2018 Carl Lerche
cfg-if                 Copyright (c) 2014 Alex Crichton
cpufeatures            Copyright (c) 2020-2025 The RustCrypto Project Developers
crypto-common          Copyright (c) 2021 RustCrypto Developers
data-encoding          Copyright (c) 2015-2020 Julien Cretin
digest                 Copyright (c) 2017 Artyom Pavlov
errno                  Copyright (c) 2014 Chris Wong
generic-array          Copyright (c) 2015 Bartlomiej Kaminski
getrandom              Copyright (c) 2018-2024 The rust-random Project Developers
http                   Copyright (c) 2017 http-rs authors
httparse               Copyright (c) 2015-2025 Sean McArthur
itoa                   Copyright (c) David Tolnay
libc                   Copyright (c) The Rust Project Developers
log                    Copyright (c) 2014 The Rust Project Developers
memchr                 Copyright (c) 2015 Andrew Gallant
ppv-lite86             Copyright (c) 2019 The CryptoCorrosion Contributors
proc-macro2            Copyright (c) David Tolnay and Alex Crichton
quote                  Copyright (c) David Tolnay
rand                   Copyright (c) The Rand Project Developers and contributors
rand_chacha            Copyright (c) The Rand Project Developers and contributors
rand_core              Copyright (c) The Rand Project Developers and contributors
serde                  Copyright (c) Erick Tryzelaar and David Tolnay
serde_core             Copyright (c) Erick Tryzelaar and David Tolnay
serde_derive           Copyright (c) Erick Tryzelaar and David Tolnay
serde_json             Copyright (c) Erick Tryzelaar and David Tolnay
sha1                   Copyright (c) 2006-2009 Graydon Hoare
                       Copyright (c) 2016 Artyom Pavlov and RustCrypto Developers
signal-hook            Copyright (c) 2017 tokio-jsonrpc developers
signal-hook-registry   Copyright (c) 2017 tokio-jsonrpc developers
syn                    Copyright (c) David Tolnay
thiserror              Copyright (c) David Tolnay
thiserror-impl         Copyright (c) David Tolnay
tungstenite            Copyright (c) 2017 Alexey Galakhov
                       Copyright (c) 2016 Jason Housley
typenum                Copyright (c) 2014 Paho Lurie-Gregg
                       Copyright (c) 2014 Andre Bogus
unicode-ident          Copyright (c) David Tolnay
                       Copyright (c) 1991-2023 Unicode, Inc.
utf-8                  Copyright (c) Simon Sapin
version_check          Copyright (c) 2017-2018 Sergio Benitez
wasi                   Copyright (c) The Cranelift Project Developers
windows-link           Copyright (c) Microsoft Corporation
windows-sys            Copyright (c) Microsoft Corporation
zerocopy               Copyright 2023 The Fuchsia Authors
zerocopy-derive        Copyright 2023 The Fuchsia Authors
zmij                   Copyright (c) David Tolnay
```

## The MIT licence

Every MIT-licensed crate above is used under these terms, with the copyright
notice as listed.

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## The Unicode licence, version 3

Applies to the character tables inside `unicode-ident`.

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright (c) 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```

## Not bundled: ffmpeg and Chromium

kaviri does not contain, link against, vendor, redistribute or download
ffmpeg or Chromium. It discovers both on the host at run time and invokes
them as separate processes:

- **ffmpeg** is located by `$KAVIRI_FFMPEG`, then by `ffmpeg` on `PATH`, then
  by a short list of conventional install locations. kaviri spawns it as a
  child process and pipes frames to its stdin. No ffmpeg code is linked in, no
  ffmpeg headers are used, and no ffmpeg binary ships in the release archive.
- **Chromium** (or Chrome) is located by `$KAVIRI_CHROMIUM`, then by
  `chromium` or `google-chrome` on `PATH`, then by conventional locations.
  kaviri spawns it and speaks the Chrome DevTools Protocol to it over a
  websocket. The protocol is a wire format; kaviri contains no Chromium code.

### Why this matters

This is the design decision that keeps a kaviri build sellable, and it has to
stay true.

ffmpeg is LGPL-2.1-or-later by default and GPL-2.0-or-later when built with
`--enable-gpl`, which is what almost every distribution package and static
build ships, because `--enable-gpl` is how libx264 gets in. In other words,
the ffmpeg builds that can actually write H.264, which is what kaviri asks for,
are the GPL ones. libx264 itself is GPL and separately patent-encumbered.

If kaviri linked ffmpeg's libraries, or shipped an ffmpeg binary in the same
distribution unit, those obligations would attach to kaviri's own
distribution: at minimum an LGPL relinking obligation, and under a GPL build a
real argument that kaviri is a derivative work that must itself be GPL. That
is incompatible with selling a closed build, and it would make kaviri
unusable inside most commercial products.

Shelling out to a binary the user installed is a different legal position.
Spawning a separate process and talking to it over a pipe is the recognised
arm's-length arrangement: the user supplies their own ffmpeg, under whatever
terms they obtained it, and kaviri is a program that runs it, in the same way
a shell script does. The same argument covers Chromium, which is BSD-3-Clause
at its core but carries an LGPL component set and a long tail of other
licences that nobody wants to inherit.

So:

- **Do not** add an ffmpeg or Chromium binary to the release archive, the
  installer, the container image, or a Homebrew formula that vendors either.
  Depending on them as separate packages is fine; that is what a package
  manager dependency is for.
- **Do not** replace the subprocess with an `ffmpeg-sys` or `ffmpeg-next`
  binding, however much simpler the frame pipeline would be. That one change
  converts kaviri into a derivative work of ffmpeg.
- **Do** keep the README's statement that both are discovered at run time and
  remain the user's own software under their own licences.
- **Do** tell users of H.264 output that libx264 and AVC carry patent
  considerations in some jurisdictions, and that the encoder and the
  distribution decision are theirs, not kaviri's.

## Keeping this file honest

This file was assembled by hand from `Cargo.toml` and `Cargo.lock` at the
versions listed, cross-checked against the licence files inside the vendored
crates. Two things should be automated before the first public release,
because a hand-written notices file goes stale on the first `cargo update`:

1. **Generate it.** `cargo about generate` produces this document from the
   real resolved graph and the real `LICENSE-*` files inside each `.crate`,
   including the exact copyright lines, which is what MIT actually asks for.
   A generator does not misremember a name.
2. **Enforce it.** `cargo deny check licenses`, with an allowlist of
   MIT, Apache-2.0, Unlicense, BSD-2-Clause, BSD-3-Clause and Unicode-3.0,
   wired into CI, so that a future dependency carrying a copyleft or
   source-available licence fails the build rather than being found by a
   customer's lawyer.
