#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec "$PWD/scripts/verify-native-bundle.sh" "${1:-$PWD/dist/PortKey.app}"
