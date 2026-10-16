#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
app="${1:-$PWD/dist/PortKey-Rust.app}"
plutil -lint "$app/Contents/Info.plist"
test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = local.portkey.rust
test "$(plutil -extract LSUIElement raw -o - "$app/Contents/Info.plist")" = true
for binary in "$app/Contents/MacOS/PortKey" "$app/Contents/Helpers/m1ddc" "$app/Contents/Helpers/portkey-cli"; do
    test -x "$binary"
    otool -l "$binary" | awk '$1 == "minos" {found=1; if ($2 != "13.0") exit 1} END {if (!found) exit 1}'
done
otool -L "$app/Contents/MacOS/PortKey" | grep -q AppKit.framework
cmp ThirdParty/m1ddc/LICENSE "$app/Contents/Resources/m1ddc-LICENSE"
cmp ThirdParty/m1ddc/UPSTREAM.md "$app/Contents/Resources/m1ddc-UPSTREAM.md"
codesign --verify --deep --strict "$app"
printf 'Native AppKit bundle verified: %s\n' "$app"
