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

The separately selected migration candidate builds all roles with:

```sh
./scripts/check.sh --reclaimed-softdevice
```

This adds the `reclaimed-softdevice` feature and selects application flash `0x1000..0x65000`. It replaces the resident S140 SoftDevice with RMK's current SDC/MPSL stack; transport roles stay the same. The default layout and UF2 guard remain conservative. This command only cross-builds: physical recovery, backup restoration and hardware acceptance remain required before an image can be approved. CI runs both layouts independently and retains the default left overflow as a failure.

To build the explicitly selected recovery-first keyboard variants:

```sh
./scripts/check.sh --reclaimed-softdevice --usb-recovery-first
```

The additional feature reserves the installed Adafruit bootloader's existing recovery marker at application base + `0x200`. It changes USB cold-start behavior and is not enabled by default. Binary fit does not verify that the vendor bootloader implements the convention. USB-only receiver variants are diagnostic: their normal cold-plug workflow is unsuitable with this marker. See [update foundation research](research/update-foundation.md).

The harness also builds a separate stage-zero USB diagnostic for the right half, always preserving S140 regardless of the selected keyboard layout. To build only that diagnostic:

```sh
cd firmware
cargo build --locked --release --bin recovery-probe --no-default-features \
  --features right,recovery-probe,usb-recovery-first
```

It identifies itself as `NocFree Recovery Probe Right`, sends a version greeting through USB CDC and requests the existing UF2 bootloader on a 1200-baud touch with DTR low. It performs no scanning, radio, ADC, battery or storage initialization. Compile guards reject migration and other roles. It is not keyboard firmware. A packaged diagnostic must pass `scripts/image_guard.py --image PATH --require-recovery-marker`. The controlled trial demonstrated update entry, cold USB recovery and factory restoration on the owner's right half; see [the observed results](recovery-probe.md). This does not establish recovery on the left or receiver, or for the separate S140-replacement layout.

Development omits web/Vial remapping, custom message transport, combos, forks, Morse actions and recorded macros. Profile switching and a small function layer are configured directly in Rust. These size choices do not establish factory feature parity.

The host scanner tests validate I²C input configuration, polarity, mapping uniqueness, full snapshots, partial-read failure isolation and repeated electrical transitions. They do not exercise physical wiring, RMK debounce timing, radio reliability or OS HID behavior. See [acceptance](acceptance.md) for those gates.

The harness also builds the separate right-only `input-probe` with features `right,input-probe,usb-recovery-first`. It adds RMK USB typing and the existing scanner while preserving the independent CDC updater, resident S140 and bootloader marker. See [its exact candidate and pending hardware trial](input-probe.md). When preparing a serial update, pass both its UF2 and ZIP to `scripts/image_guard.py --image PATH --serial-package ZIP`; this checks that the application-only legacy package matches the guarded payload and safe erase extent. Packaging and successful checks never authorize a device write.
