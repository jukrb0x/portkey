#!/bin/sh
# Optional repository-local toolchain; installed system Cargo also works.
if [ -x "$PWD/.tools/cargo/bin/cargo" ]; then
    export CARGO_HOME="$PWD/.tools/cargo"
    export RUSTUP_HOME="$PWD/.tools/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
