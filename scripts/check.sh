#!/bin/sh
# One repeatable local/CI entrypoint; never writes to a device.
set -eu
layout_features=""
for option in "$@"; do
    case "$option" in
        --reclaimed-softdevice) layout_features="$layout_features,reclaimed-softdevice" ;;
        --usb-recovery-first) layout_features="$layout_features,usb-recovery-first" ;;
        *) echo "Usage: $0 [--reclaimed-softdevice] [--usb-recovery-first]" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
python3 -m unittest discover -s tests -v
cargo test --locked --manifest-path crates/nocfree-input/Cargo.toml
cargo fmt --manifest-path crates/nocfree-input/Cargo.toml -- --check
cargo fmt --manifest-path firmware/Cargo.toml -- --check
(
    cd firmware
    failed=0
    for role in left right receiver; do
        cargo build --locked --release --bin nocfree-rmk --target thumbv7em-none-eabihf \
            --no-default-features --features "$role$layout_features" || failed=1
    done
    cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf \
        --no-default-features --features right,recovery-probe,usb-recovery-first || failed=1
    exit "$failed"
)
