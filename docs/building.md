# Repeatable builds

Install Rust through rustup; the root `rust-toolchain.toml` selects Rust 1.93.1, rustfmt and `thumbv7em-none-eabihf`. Install an Arm GNU bare-metal toolchain **with newlib headers**, and put its `bin` directory on PATH. Apple's clang is insufficient for the P-256 dependency. The Homebrew bare GCC formula may lack the required C library headers; Arm's complete toolchain includes them.

Locally verified compiler: Arm GNU 15.2.rel1 for macOS arm64, downloaded from [Arm's distribution](https://developer.arm.com/-/media/Files/downloads/gnu/15.2.rel1/binrel/arm-gnu-toolchain-15.2.rel1-darwin-arm64-arm-none-eabi.tar.xz). Linux CI installs Ubuntu's `gcc-arm-none-eabi` and `libnewlib-arm-none-eabi`; this verifies portability but is not a byte-for-byte reproduction of the macOS compiler. Rust dependencies and upstream RMK are locked. RMK is pinned to an unreleased commit, not described as a released 0.9.0 build.

```sh
./scripts/check.sh
```

This runs the Python UF2 guard tests, Rust expander tests, formatting checks and release cross-builds for all three roles. It writes only build artifacts; it never accesses or flashes a device. A failing role makes the entire harness fail, even if other roles build.

For an individual role, run from `firmware/` so Cargo finds the target flags and RMK's compile-time keyboard configuration:

```sh
cd firmware
cargo build --locked --release --no-default-features --features right
```

Supported roles are `left`, `right`, and `receiver`; select exactly one. There is no Cargo flash runner. Builds use application flash `0x27000..0x65000`, settings `0x65000..0x6d000` and RAM `0x20008000..0x20020000`, preserving the inferred factory filesystem and bootloader plus the known warm-reset RAM location.

Development omits web/Vial remapping, custom message transport, combos, forks, Morse actions and recorded macros. Profile switching and a small function layer are configured directly in Rust. These size choices do not establish factory feature parity.

The host scanner tests validate I²C input configuration, polarity, mapping uniqueness, full snapshots, partial-read failure isolation and repeated electrical transitions. They do not exercise physical wiring, RMK debounce timing, radio reliability or OS HID behavior. See [acceptance](acceptance.md) for those gates.
