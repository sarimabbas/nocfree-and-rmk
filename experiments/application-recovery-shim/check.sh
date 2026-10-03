#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
rustc --test --edition=2021 firmware/src/startup_recovery/gate.rs -o "$work/tests"
"$work/tests"
rustc --crate-type lib --edition=2021 --target thumbv7em-none-eabihf \
    -C opt-level=s --emit=obj -o "$work/shim.o" experiments/application-recovery-shim/lib.rs
