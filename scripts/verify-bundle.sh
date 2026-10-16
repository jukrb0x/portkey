#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
app="${1:-$PWD/dist/USB Follow Display.app}"
plutil -lint "$app/Contents/Info.plist"
test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = local.usbfollowdisplay.app
for executable in "$app/Contents/MacOS/USBFollowDisplay" "$app/Contents/Helpers/m1ddc"; do
    test -x "$executable"
    otool -l "$executable" | awk '$1 == "minos" {found=1; if ($2 != "13.0") {print "Wrong deployment target: " $2; exit 1}} END {if (!found) exit 1}'
done
cmp ThirdParty/m1ddc/LICENSE "$app/Contents/Resources/m1ddc-LICENSE"
codesign --verify --deep --strict "$app"
printf 'Bundle verified: %s\n' "$app"
