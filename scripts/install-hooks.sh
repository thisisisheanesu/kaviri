#!/usr/bin/env bash
# Make the checks run on every push, since GitHub will not run them.
#
#   scripts/install-hooks.sh
#
# Installs a pre-push hook that runs scripts/ci.sh. Skip it once with `git push --no-verify`,
# which is there for the day you need it and should feel like a decision.
#
# This is a workaround and it is worth being honest about what it does not give you. It runs
# on one machine, with whatever is installed on it, against whatever is in the working tree's
# toolchain. It cannot tell you the build is broken on a clean checkout, or on another
# platform, or with a different Rust version. It catches the thing that actually happens,
# which is pushing a change that does not compile or does not pass its own tests.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

hooks="$(git rev-parse --git-path hooks)"
mkdir -p "$hooks"

cat > "$hooks/pre-push" <<'HOOK'
#!/usr/bin/env bash
# Installed by scripts/install-hooks.sh. Runs the checks GitHub Actions cannot.
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
if [ ! -x "$root/scripts/ci.sh" ]; then
  echo "pre-push: scripts/ci.sh is missing or not executable; skipping" >&2
  exit 0
fi
echo "pre-push: running scripts/ci.sh (git push --no-verify to skip)"
"$root/scripts/ci.sh"
HOOK

chmod +x "$hooks/pre-push"
chmod +x "$root/scripts/ci.sh"

echo "installed $hooks/pre-push"
echo "it runs scripts/ci.sh before every push; --no-verify skips it"
