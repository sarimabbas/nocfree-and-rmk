#!/bin/sh
# One repeatable local/CI entrypoint; never writes to a device.
set -eu
cd "$(dirname "$0")/.."
python3 -m unittest discover -s tests -v
cargo test --locked --manifest-path crates/nocfree-input/Cargo.toml
cargo fmt --manifest-path crates/nocfree-input/Cargo.toml -- --check
cargo fmt --manifest-path firmware/Cargo.toml -- --check
(
    cd firmware
    for role in left right receiver; do
        cargo build --locked --release --target thumbv7em-none-eabihf \
            --no-default-features --features "$role"
    done
)
