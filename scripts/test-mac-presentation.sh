#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
bin="$(mktemp -d "${TMPDIR:-/tmp}/portkey-mac-tests.XXXXXX")"
trap 'rm -rf "$bin"' EXIT
swiftc -swift-version 5 native/macos/Presentation.swift native/macos/tests/main.swift -o "$bin/tests"
"$bin/tests"
