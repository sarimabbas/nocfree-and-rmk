#!/bin/sh
# One repeatable local/CI entrypoint; never writes to a device.
set -eu
layout_features=""
backlight_features=""
runtime_features=""
mode=all
physical_layouts=ansi
# With no options, check the supported production layout and verified polarity.
if [ "$#" -eq 0 ]; then
    layout_features=",reclaimed-softdevice"
    backlight_features=",backlight-active-high"
    runtime_features=",runtime-recovery"
fi
while [ "$#" -gt 0 ]; do
    option=$1
    shift
    case "$option" in
        --layout)
            if [ "$#" -eq 0 ]; then echo "--layout needs ansi, iso, jis or kr" >&2; exit 2; fi
            case "$1" in ansi|iso|jis|kr) physical_layouts=$1 ;; *) echo "Unknown layout: $1" >&2; exit 2 ;; esac
            shift ;;
        --all-layouts) physical_layouts="ansi iso jis kr" ;;
        --host-only) mode=host ;;
        --build-only) mode=build ;;
        --reclaimed-softdevice) layout_features="$layout_features,reclaimed-softdevice" ;;
        --backlight-active-low) backlight_features="$backlight_features,backlight-active-low" ;;
        --backlight-active-high) backlight_features="$backlight_features,backlight-active-high" ;;
        --runtime-recovery) runtime_features=",runtime-recovery" ;;
        *) echo "Usage: $0 [--reclaimed-softdevice] [--backlight-active-low|--backlight-active-high] [--runtime-recovery] [--layout ansi|iso|jis|kr|--all-layouts] [--host-only|--build-only]" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
python3 scripts/prepare_migration.py
if [ "$mode" != build ]; then
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT HUP INT TERM
    rustc --test --edition=2024 tests/watchdog_recovery.rs -o "$work/watchdog-tests"
    "$work/watchdog-tests"
    python3 -m unittest discover -s tests -v
    for physical_layout in $physical_layouts; do
        if [ "$physical_layout" = ansi ]; then
            cargo test --locked --manifest-path crates/nocfree-input/Cargo.toml
        else
            cargo test --locked --manifest-path crates/nocfree-input/Cargo.toml --features "layout-$physical_layout"
        fi
    done
    cargo fmt --manifest-path crates/nocfree-input/Cargo.toml -- --check
    cargo fmt --manifest-path firmware/Cargo.toml -- --check
    python3 firmware/presets/test_presets.py
    python3 -m unittest discover -s scripts -p "test_companion_*.py" -v
fi
if [ "$mode" = host ]; then exit 0; fi
(
    cd firmware
    failed=0
    for physical_layout in $physical_layouts; do
        for keymap_features in "" ",mac-keymap"; do
            for role in left right receiver; do
                physical_features=""
                if [ "$role" != receiver ] && [ "$physical_layout" != ansi ]; then physical_features=",layout-$physical_layout"; fi
                role_keymap_features="$keymap_features"
                if [ "$role" = receiver ]; then role_keymap_features=""; fi
                role_backlight_features="$backlight_features"
                role_layout_features="$layout_features"
                # Receiver has no keyboard backlight and needs no lighting feature/schema change.
                if [ "$role" = receiver ]; then role_backlight_features=""; fi
                # Production runtime recovery retains the receiver's resident S140.
                if [ "$role" = receiver ] && [ -n "$runtime_features" ]; then role_layout_features=""; fi
                cargo build --locked --release --bin nocfree-rmk --target thumbv7em-none-eabihf \
                    --no-default-features --features "defmt-logging,$role$role_layout_features$role_keymap_features$role_backlight_features$runtime_features$physical_features" || failed=1
            done
        done
    done
    exit "$failed"
)
