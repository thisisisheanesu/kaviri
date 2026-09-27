#!/bin/sh
# Films the takes the kaviri.dev hero is cut from: the Parcel demo in examples/ recorded by kaviri
# itself in five frames, and the showreel it shows as an example of kaviri motion. Then renders
# the hero. Run from the repository root:
#
#   sh examples/motion/hero-takes.sh
#
# The takes land in examples/motion/takes/ (MP4s are not committed), the hero in hero.mp4.
set -eu
K=${KAVIRI:-./target/release/kaviri}
T=examples/motion/takes
mkdir -p "$T"

# The demo app, served the way CI serves it.
python3 -m http.server 8099 --directory examples >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null' EXIT
for _ in $(seq 1 40); do curl -sf http://127.0.0.1:8099/ >/dev/null && break; sleep 0.25; done

# --slowmo 4: the page's clock runs four times slower while filming, so a software-rendered
# browser still gives every frame of a smooth take.
S=examples/demo.jsonl
$K record --script $S --slowmo 4 --frame macos --desktop on --dock dev --preset landscape --out "$T/macos.mp4"
$K record --script $S --slowmo 4 --frame ios --out "$T/ios.mp4"
$K record --script $S --slowmo 4 --frame android --out "$T/android.mp4"
$K record --script $S --slowmo 4 --frame ios --frame-style recording --out "$T/ios-rec.mp4"
$K record --script $S --slowmo 4 --frame windows --preset landscape --out "$T/windows.mp4"

# kaviri motion, shown inside the hero as what it makes.
$K motion --script examples/motion/showreel.jsonl --out "$T/showreel.mp4"
$K motion --script examples/motion/announcement.jsonl --out "$T/announcement.mp4"

$K motion --script examples/motion/hero.jsonl --out hero.mp4
