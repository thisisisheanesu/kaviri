#!/usr/bin/env bash
# CI, run here, because GitHub will not run it.
#
# Every push to this repository comes back `startup_failure` with zero jobs, and the reason is
# not the workflow: the GitHub account has no payment method, which locks billing, which
# disables Actions on private repositories. `.github/workflows/ci.yml` is correct and will run
# the day that is fixed or the day the repository goes public, whichever comes first.
#
# Until then the checks still have to happen on every change, or "the tests pass" quietly
# becomes "the tests passed when I last remembered". This script runs exactly what ci.yml
# runs, in the same order, and scripts/install-hooks.sh wires it to a pre-push hook so it is
# not a thing to remember.
#
#   scripts/ci.sh          the same checks ci.yml runs
#   scripts/ci.sh --demo   and film the example, which is what demo.yml does
#
# Exit code is the verdict, so it works in a hook and it will work in Actions unchanged.

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# A git hook does not get your interactive PATH. It gets a short one, without ~/.cargo/bin,
# so the first run of this from pre-push failed with "cargo: command not found" and reported
# it as a formatting failure, which is a confusing way to learn about a PATH. Find the
# toolchain rather than assume the caller has it.
for dir in "$HOME/.cargo/bin" "$HOME/.local/bin" /usr/local/bin; do
  case ":$PATH:" in
    *":$dir:"*) ;;
    *) [ -d "$dir" ] && PATH="$dir:$PATH" ;;
  esac
done
export PATH
command -v cargo >/dev/null || {
  echo "ci: cargo is not on PATH and was not in ~/.cargo/bin either." >&2
  exit 1
}

# The frame spool and ffmpeg's scratch go here rather than /tmp, which on this machine has a
# per-user quota small enough that a render fills it and fails in a way that reads like an
# ffmpeg bug.
export TMPDIR="${TMPDIR:-$PWD/target/tmp}"
mkdir -p "$TMPDIR"

demo=0
for arg in "$@"; do
  case "$arg" in
    --demo) demo=1 ;;
    *) echo "ci: unknown argument $arg" >&2; exit 2 ;;
  esac
done

step() { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
fail() { printf '\033[31mci: %s\033[0m\n' "$1" >&2; exit 1; }

step "cargo fmt --check"
cargo fmt --check || fail "formatting. Run cargo fmt."

step "cargo clippy --all-targets -- -D warnings"
cargo clippy --all-targets -- -D warnings

step "cargo test --release"
cargo test --release

# Not a lint anybody else runs, and worth its two seconds: the em dash is a house style rule
# and it is much easier to catch here than in review.
step "house style"
if git grep -nP '\xe2\x80\x94' -- '*.rs' '*.md' '*.html' '*.css' '*.js' '*.toml' | grep -v '^docs/.*CHANGELOG'; then
  fail "em dashes, which this project does not use."
fi
echo "  no em dashes"

# The playground runs a second copy of the camera model. A test inside the crate checks the
# constants match, and it only runs if the file is where it expects.
step "the browser camera is the same camera"
test -f site/play/camera.js || fail "site/play/camera.js is missing, so the preview cannot be checked against the crate."
cargo test --release the_preview_constants_match -- --exact --nocapture >/dev/null
echo "  constants agree"

if [ "$demo" -eq 1 ]; then
  step "film the example, which is what demo.yml does"
  command -v ffmpeg >/dev/null || fail "ffmpeg is not on PATH."
  port=8099
  python3 -m http.server "$port" --bind 127.0.0.1 --directory examples >/dev/null 2>&1 &
  srv=$!
  # Kill by the pid we own. A pkill pattern here would also match the shell running this
  # script, which is a mistake this project has already made more than once.
  trap 'kill "$srv" 2>/dev/null || true' EXIT
  for _ in $(seq 1 40); do
    curl -sf "http://127.0.0.1:$port/" >/dev/null && break
    sleep 0.25
  done
  out="$TMPDIR/ci-demo.mp4"
  cargo run --release -- record --script examples/demo.jsonl --out "$out" --preset readme --smooth off
  frames=$(ffprobe -v error -select_streams v:0 -count_frames \
           -show_entries stream=nb_read_frames -of csv=p=0 "$out")
  [ "${frames:-0}" -gt 100 ] || fail "the take came out with only ${frames:-0} frames."
  echo "  filmed $out, $frames frames"
fi

printf '\n\033[32mci: everything passed\033[0m\n'
