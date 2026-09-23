# Security policy

## Reporting a vulnerability

Report privately. Do not open a public issue, and do not post a proof of
concept anywhere public, until a fix has shipped.

- Preferred: GitHub's private vulnerability reporting, under the repository's
  **Security** tab → **Report a vulnerability**. It creates a private thread
  attached to the repo and gets a CVE if one is warranted.
- By email: **ishe@vambo.ai**, subject line starting `kaviri security`.

Please include the kaviri version (`kaviri --version`), the OS, the Chromium and
ffmpeg that `kaviri doctor` reports, the smallest op script or command line that
reproduces the problem, and what an attacker gains. A reproduction that runs in
under a minute is worth more than a long write-up.

What to expect: an acknowledgement within three working days, an assessment
within ten, and a fix or a dated plan after that. Credit in the changelog if
you want it, and no credit if you do not. There is no bug bounty.

Supported versions: the latest release only. kaviri is pre-1.0 and fixes land
on the tip rather than being backported.

## The trust boundary, stated honestly

kaviri is a tool for driving a browser with an automation protocol and turning
what happens into a video. Several of the things it does are indistinguishable
from the things an attacker would want it to do. The line between "working as
designed" and "vulnerability" therefore has to be written down rather than
assumed.

### `kaviri serve --port` is a remote control socket

This is the one that matters most, so it comes first.

`kaviri serve --port N` listens on `127.0.0.1:N` and executes the op protocol
for whoever connects. The op protocol can navigate the browser anywhere,
click and type into the page, run script in the page, and write an MP4 and a
telemetry sidecar to a path on disk. **A process that can talk to that port
can do all of that.** Treat the port exactly as you would treat an open SSH
session to the machine, not as a debugging convenience.

Two specific things people get wrong:

- **Binding to loopback is not a boundary against a web browser.** Any web
  page the user visits while kaviri is serving can issue a cross-origin
  `fetch()` at `http://127.0.0.1:N` with `mode: 'no-cors'`. The page cannot
  read the response, but it does not need to: the ops have already run. This
  is why the protocol requires a token handshake as the first line of every
  connection and drops any connection whose first line is not valid JSON, so
  a browser-issued HTTP request disconnects rather than partially executing.
  The token is printed on stderr at startup and is not guessable by a page.
- **Every local user and every local process is in scope.** Loopback is
  reachable by every account on the machine and by every container sharing
  the host network namespace. On a shared box or a multi-tenant runner, the
  token is the only thing standing between another tenant and your browser
  session.

Therefore: prefer `kaviri serve` on stdin, which is the default and has no
socket at all. Use `--port` only when the driving agent genuinely cannot share
a pipe, on a machine you control, and treat the token as a credential. Never
expose the port beyond loopback, never forward it, and never put it behind a
reverse proxy.

### What kaviri trusts, by design

These are not vulnerabilities. They are the tool's contract, and a report
about one of them will be closed as working as intended.

- **The op script.** A `.jsonl` script is a program. It can navigate to
  `file://` URLs, run arbitrary JavaScript in the page through the ops that
  evaluate, and choose the output path. Running an untrusted script is
  equivalent to running an untrusted shell script, and should be treated the
  same way. Do not feed kaviri a script from a pull request, an issue body, or
  any other place a stranger can write.
- **The environment.** `KAVIRI_CHROMIUM`, `KAVIRI_CHROMIUM_ARGS`, `KAVIRI_FFMPEG`,
  `KAVIRI_SPOOL_DIR`, `TMPDIR` and `PATH` all steer which binaries kaviri
  executes and where it writes. An attacker who can set your environment or
  prepend to your `PATH` has already won; kaviri does not try to defend against
  that.
- **The browser profile.** kaviri launches Chromium with a fresh throwaway
  profile in a private per-run directory. It does not read your real profile,
  your cookies or your saved passwords, and it is not designed to record a
  session that is authenticated as you. If a take needs a login, the script
  performs the login, and the credentials are then in the script.
- **The page being recorded.** kaviri points a browser at a URL. That page runs
  its own JavaScript in a normal Chromium sandbox. kaviri adds no isolation
  beyond what the browser provides, and it is not a malware analysis sandbox.
  Recording a hostile site is recording a hostile site.

### What kaviri does try to get right

A report about one of these is in scope:

- The `--port` token handshake being bypassable, guessable, or skippable.
- An op that escapes the intended write location. In record mode the output
  path comes from `--out` and a script-supplied `path` is ignored; in serve
  mode the output path is constrained rather than taken verbatim.
- Argument injection into the processes kaviri spawns. Output and input paths
  reach ffmpeg's argv, and a path beginning with `-` must be treated as a
  filename rather than an option.
- Temp file handling. The frame spool and the browser profile live in one
  per-run directory created mode 0700 with a name carrying real entropy, and
  the spool file is created `O_EXCL` mode 0600, so neither path is guessable
  or pre-creatable by another local user.
- Anything that causes kaviri to execute a binary other than the one the user
  resolved, or to write outside the directories it announced.

### Two things worth knowing that are not bugs

- **The telemetry sidecar contains every URL the take visited.** It is off
  unless `--keep-temp` or `KAVIRI_TELEMETRY` asks for it, and it then records
  the marks, where a `navigate` mark's label is the fully resolved URL,
  including query strings, tokens in query strings, and canonicalised local
  `file://` paths. Under `--keep-temp` it sits next to the MP4 as
  `<out>.telemetry.json`. If you publish the video, do not publish the sidecar
  without reading it first.
- **The captured frames sit on disk for the length of the take.** The JPEG
  spool holds everything the browser displayed, including whatever was on
  screen during a login. It lives in the per-run 0700 directory and is removed
  when the take is rendered or the process exits, but a hard kill (SIGKILL,
  a power loss) leaves it behind. `--keep-temp` leaves it behind on purpose.

## Hardening a deployment

- Run takes as a dedicated unprivileged user with its own `TMPDIR`.
- Use `kaviri serve` on stdin. If `--port` is unavoidable, bind it on a host
  where you control every local account, and rotate the process rather than
  leaving a server up between takes.
- Keep untrusted pages and authenticated sessions in different runs. There is
  one browser profile per run, and that is the isolation.
- In CI, treat the recorded artifacts as potentially containing secrets from
  the app under test, because they do: a video of a logged-in app is a video
  of a logged-in app.
