#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ! -f target/release/libportkey.a ]]; then
    source scripts/rust-env.sh
    cargo build --locked --release --lib
fi
bin="$(mktemp -d "${TMPDIR:-/tmp}/portkey-state-tests.XXXXXX")"
trap 'rm -rf "$bin"' EXIT
swiftc -swift-version 5 -import-objc-header native/portkey.h \
    -framework AppKit -framework SwiftUI -framework CoreFoundation -framework CoreGraphics -framework ColorSync -framework IOKit \
    native/macos/CoreSession.swift native/macos/Presentation.swift native/macos/PortKeyStore.swift native/macos/USBNotifications.swift native/macos/StatusMenu.swift \
    native/macos/state-tests/main.swift target/release/libportkey.a -o "$bin/tests"
"$bin/tests"
