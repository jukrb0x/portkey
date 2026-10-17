#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/rust-env.sh
cargo test --locked
if [[ "$(uname -s)" == Darwin ]]; then
    scripts/test-ddc.sh
    scripts/test-mac-presentation.sh
    cargo build --locked --release --lib
    scripts/test-mac-state.sh
fi
