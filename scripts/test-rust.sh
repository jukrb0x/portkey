#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
. ./scripts/rust-env.sh
cargo test --locked
