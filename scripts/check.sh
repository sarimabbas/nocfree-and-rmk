#!/bin/sh
# One repeatable local/CI entrypoint; never writes to a device.
set -eu
layout_features=""
backlight_features=""
shim_features=""
rescue_features=""
for option in "$@"; do
    case "$option" in
        --reclaimed-softdevice) layout_features="$layout_features,reclaimed-softdevice" ;;
        --usb-recovery-first) layout_features="$layout_features,usb-recovery-first" ;;
        --backlight-active-low) backlight_features="$backlight_features,backlight-active-low" ;;
        --backlight-active-high) backlight_features="$backlight_features,backlight-active-high" ;;
        --application-recovery-shim) shim_features=",application-recovery-shim" ;;
        --usb-rescue-startup) rescue_features=",usb-rescue-startup" ;;
        *) echo "Usage: $0 [--reclaimed-softdevice] [--usb-recovery-first] [--backlight-active-low|--backlight-active-high] [--application-recovery-shim] [--usb-rescue-startup]" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
./experiments/held-key-recovery/check.sh
./experiments/application-recovery-shim/check.sh
cargo test --locked --manifest-path experiments/usb-startup-recovery/Cargo.toml
python3 -m unittest discover -s tests -v
cargo test --locked --manifest-path crates/nocfree-input/Cargo.toml
cargo fmt --manifest-path crates/nocfree-input/Cargo.toml -- --check
cargo fmt --manifest-path firmware/Cargo.toml -- --check
(
    cd firmware
    failed=0
    for keymap_features in "" ",mac-keymap"; do
        for role in left right receiver; do
            role_backlight_features="$backlight_features"
            role_shim_features="$shim_features"
            # Receiver has no keyboard backlight and needs no lighting feature/schema change.
            if [ "$role" = receiver ]; then role_backlight_features=""; role_shim_features=""; fi
            cargo build --locked --release --bin nocfree-rmk --target thumbv7em-none-eabihf \
                --no-default-features --features "defmt-logging,$role$layout_features$keymap_features$role_backlight_features$role_shim_features$rescue_features" || failed=1
        done
    done
    cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf \
        --no-default-features --features defmt-logging,right,recovery-probe,usb-recovery-first || failed=1
    for inspect_role in left right; do
        cargo build --locked --release --bin bootloader-inspect --target thumbv7em-none-eabihf \
            --no-default-features --features "defmt-logging,$inspect_role,bootloader-inspect" || failed=1
    done
    for probe_role in left right; do
        for keymap_features in "" ",mac-keymap"; do
            cargo build --locked --release --bin input-probe --target thumbv7em-none-eabihf \
                --no-default-features --features "defmt-logging,$probe_role,input-probe,usb-recovery-first$keymap_features" || failed=1
        done
    done
    cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf \
        --no-default-features --features defmt-logging,left,migration-probe || failed=1
    cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf \
        --no-default-features --features defmt-logging,left,migration-entry-probe || failed=1
    for stage in runtime hal hal-serial usb-build-serial hal-neutral usb-enabled-serial usb-configured-serial usb-reset-serial usb-addressed-serial; do
        cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf \
            --no-default-features --features "defmt-logging,left,migration-$stage-probe" || failed=1
    done
    exit "$failed"
)
