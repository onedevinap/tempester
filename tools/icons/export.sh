#!/bin/bash
# Exports an app icon (tools/icons/draw.mjs) to PNG sizes, macOS .icns, Windows .ico and Linux PNGs.
#
# usage: tools/icons/export.sh <instrument> <output folder>     (instrument: tempest, pro2, tb303, bs2)
# Needs node, Google Chrome (headless rendering of the SVG, with a throw-away profile: the user's profile is never
# used), macOS sips and iconutil. 16 and 32 px use the simplified drawing ("small"), every other size the full one.
set -euo pipefail
P="$(cd "$(dirname "$0")/../.." && pwd)"
INST=${1:?instrument expected}
mkdir -p "${2:?output folder expected}"
OUT="$(cd "$2" && pwd)"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$OUT/png" "$T/profile" "$T/$INST.iconset"

node "$P/tools/icons/draw.mjs" "$INST" big > "$OUT/$INST.svg"
node "$P/tools/icons/draw.mjs" "$INST" small > "$OUT/$INST-small.svg"
grep -q "<svg" "$OUT/$INST.svg" && grep -q "<svg" "$OUT/$INST-small.svg" || { echo "empty SVG"; exit 1; }

render() {  # <svg> <png 1024>   (Chrome refuses tiny windows: render at 1024, then reduce with sips)
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --user-data-dir="$T/profile" \
    --default-background-color=00000000 --window-size=1024,1024 --screenshot="$2" "file://$1" > /dev/null 2>&1 || true
  [ -s "$2" ] || { echo "rendering $1 failed"; exit 1; }
}
render "$OUT/$INST.svg" "$T/big.png"
render "$OUT/$INST-small.svg" "$T/small.png"

for s in 16 32 48 64 128 256 512 1024; do
  src="$T/big.png"; [ "$s" -le 32 ] && src="$T/small.png"
  sips -z "$s" "$s" "$src" --out "$OUT/png/${INST}_$s.png" > /dev/null
done

I="$T/$INST.iconset"
cp "$OUT/png/${INST}_16.png" "$I/icon_16x16.png";      cp "$OUT/png/${INST}_32.png" "$I/icon_16x16@2x.png"
cp "$OUT/png/${INST}_32.png" "$I/icon_32x32.png";      cp "$OUT/png/${INST}_64.png" "$I/icon_32x32@2x.png"
cp "$OUT/png/${INST}_128.png" "$I/icon_128x128.png";   cp "$OUT/png/${INST}_256.png" "$I/icon_128x128@2x.png"
cp "$OUT/png/${INST}_256.png" "$I/icon_256x256.png";   cp "$OUT/png/${INST}_512.png" "$I/icon_256x256@2x.png"
cp "$OUT/png/${INST}_512.png" "$I/icon_512x512.png";   cp "$OUT/png/${INST}_1024.png" "$I/icon_512x512@2x.png"
iconutil -c icns "$I" -o "$OUT/$INST.icns"

python3 -I "$P/tools/icons/make_ico.py" "$OUT/$INST.ico" \
  "$OUT/png/${INST}_16.png" "$OUT/png/${INST}_32.png" "$OUT/png/${INST}_48.png" "$OUT/png/${INST}_256.png"
python3 -I "$P/tools/icons/png_to_rgba.py" "$OUT/png/${INST}_64.png" "$OUT/${INST}_64.rgba" > /dev/null
echo "$OUT : $INST.svg, $INST-small.svg, $INST.icns, $INST.ico, png/ (16-1024)"
