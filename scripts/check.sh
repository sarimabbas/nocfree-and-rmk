#!/bin/sh
# Repeatable software checks; never writes to a device.
set -eu
mode=all
physical_layouts=ansi
polarity=high
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
        --backlight-active-low) polarity=low ;;
        --backlight-active-high) polarity=high ;;
        *) echo "Usage: $0 [--layout ansi|iso|jis|kr|--all-layouts] [--backlight-active-low|--backlight-active-high] [--host-only|--build-only]" >&2; exit 2 ;;
    esac
done
cd "$(dirname "$0")/.."
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
    cargo fmt --manifest-path crates/backlight-tests/Cargo.toml -- --check
    # RMK event channels are global. Give each scenario a fresh process.
    export NEXTEST=1
    export KEYBOARD_TOML_PATH="$PWD/crates/backlight-tests/keyboard.toml"
    cargo test --locked --manifest-path crates/backlight-tests/Cargo.toml --lib -- --list > "$work/tests"
    sed -n 's/: test$//p' "$work/tests" > "$work/names"
    test -s "$work/names"
    while IFS= read -r name; do
        cargo test --locked --manifest-path crates/backlight-tests/Cargo.toml --lib "$name" -- --exact
    done < "$work/names"
    unset KEYBOARD_TOML_PATH NEXTEST
    python3 -m unittest discover -s scripts -p "test_companion_*.py" -v
fi
if [ "$mode" = host ]; then exit 0; fi
cd firmware
for physical_layout in $physical_layouts; do
    for keymap in "" ",mac-keymap"; do
        for role in left right receiver; do
            features="defmt-logging,$role,runtime-recovery,startup-watchdog"
            if [ "$role" != receiver ]; then
                features="$features$keymap,reclaimed-softdevice,backlight-active-$polarity,async-scanner"
                if [ "$physical_layout" != ansi ]; then features="$features,layout-$physical_layout"; fi
            fi
            if [ "$role" = left ]; then features="$features,status-led"; fi
            cargo build --locked --release --bin nocfree-rmk --target thumbv7em-none-eabihf \
                --no-default-features --features "$features"
        done
    done
done
