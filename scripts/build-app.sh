#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
swift build -c release --product USBFollowDisplay
MACOSX_DEPLOYMENT_TARGET=13.0 make -B -C ThirdParty/m1ddc binary
app="$PWD/dist/USB Follow Display.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Helpers" "$app/Contents/Resources"
cp .build/release/USBFollowDisplay "$app/Contents/MacOS/USBFollowDisplay"
cp ThirdParty/m1ddc/m1ddc "$app/Contents/Helpers/m1ddc"
cp ThirdParty/m1ddc/LICENSE "$app/Contents/Resources/m1ddc-LICENSE"
cp ThirdParty/m1ddc/UPSTREAM.md "$app/Contents/Resources/m1ddc-UPSTREAM.md"
cp Resources/Info.plist "$app/Contents/Info.plist"
chmod +x "$app/Contents/MacOS/USBFollowDisplay" "$app/Contents/Helpers/m1ddc"
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app/Contents/Helpers/m1ddc"
codesign --force --sign - "$app"
scripts/verify-bundle.sh "$app"
printf 'Built: %s\n' "$app"
