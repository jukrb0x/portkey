#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec "$PWD/scripts/build-rust-app.sh" "$@"
