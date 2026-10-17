#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/rust-env.sh
export MACOSX_DEPLOYMENT_TARGET=13.0
cargo build --locked --release
make -B -C ThirdParty/m1ddc binary
app="${1:-$PWD/dist/PortKey.app}"
[[ "$app" = /* ]] || app="$PWD/$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Helpers" "$app/Contents/Resources"
swiftc -O -swift-version 5 -target "$(uname -m)-apple-macosx13.0" \
    -import-objc-header native/portkey.h \
    -framework AppKit -framework SwiftUI -framework CoreFoundation -framework CoreGraphics -framework ColorSync -framework IOKit \
    native/macos/*.swift target/release/libportkey.a -o "$app/Contents/MacOS/PortKey"
cp target/release/portkey "$app/Contents/Helpers/portkey-cli"
cp ThirdParty/m1ddc/m1ddc "$app/Contents/Helpers/m1ddc"
cp ThirdParty/m1ddc/LICENSE "$app/Contents/Resources/m1ddc-LICENSE"
cp ThirdParty/m1ddc/UPSTREAM.md "$app/Contents/Resources/m1ddc-UPSTREAM.md"
cp Resources/Rust-Info.plist "$app/Contents/Info.plist"
icon_work="$(mktemp -d "${TMPDIR:-/tmp}/portkey-icon.XXXXXX")"
trap 'rm -rf "$icon_work"' EXIT
swift scripts/generate-mac-icon.swift "$icon_work/PortKey.iconset"
iconutil -c icns "$icon_work/PortKey.iconset" -o "$app/Contents/Resources/PortKey.icns"
/usr/libexec/PlistBuddy -c 'Add :CFBundleIconFile string PortKey' "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
codesign --force --sign - "$app/Contents/Helpers/m1ddc"
codesign --force --sign - "$app/Contents/Helpers/portkey-cli"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
scripts/verify-native-bundle.sh "$app"
printf 'Built: %s\n' "$app"
