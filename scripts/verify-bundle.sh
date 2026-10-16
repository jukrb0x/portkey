#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
app="${1:-$PWD/dist/Portkey.app}"
plutil -lint "$app/Contents/Info.plist"
test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = local.usbfollowdisplay.app
test "$(plutil -extract CFBundleDisplayName raw -o - "$app/Contents/Info.plist")" = Portkey
test "$(plutil -extract CFBundleExecutable raw -o - "$app/Contents/Info.plist")" = Portkey
for executable in "$app/Contents/MacOS/Portkey" "$app/Contents/Helpers/m1ddc"; do
    test -x "$executable"
    otool -l "$executable" | awk '$1 == "minos" {found=1; if ($2 != "13.0") {print "Wrong deployment target: " $2; exit 1}} END {if (!found) exit 1}'
done
for language in en zh-Hans; do
    plutil -lint "$app/Contents/Resources/$language.lproj/Localizable.strings"
    cmp "Sources/FollowCore/Resources/$language.lproj/Localizable.strings" "$app/Contents/Resources/$language.lproj/Localizable.strings"
done
cmp ThirdParty/m1ddc/LICENSE "$app/Contents/Resources/m1ddc-LICENSE"
codesign --verify --deep --strict "$app"
printf 'Bundle verified: %s\n' "$app"
