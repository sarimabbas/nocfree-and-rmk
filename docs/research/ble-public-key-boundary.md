# Review of the captured public-key boundary

Reviewed 2026-10-02 against the pinned published Trouble Host 0.8.0 source and local Cargo-resolved Cortex-M4 backend. No firmware or device state was changed by this review.

## Observations and source facts

The private Mac trace records a host SMP public key followed by no target public-key response before link timeout. An independent Python `cryptography` check of that 64-byte public point, decoding SMP's X and Y as little-endian integers, accepted it on SECP256R1. The point and trace remain private. This validates the host packet's mathematical point only; it does not prove the keyboard received or parsed it.

Trouble's pinned peripheral handler decodes the peer public key, clones the local secret, references its cached local public key, computes `dh_key`, and only then calls `send_public_key`. Therefore the missing response brackets reception/dispatch, point validation, synchronous ECDH and response queueing; the packet trace alone cannot select one. [Peripheral handler](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/security_manager/pairing/peripheral.rs#L277-L303).

The enabled Cortex-M4 path rejects the Bluetooth debug key and a peer key equal to the local key, validates the point through `from_untagged_bytes`, and calls the backend's synchronous `agree`. A mathematical on-curve check does not check equality against the keyboard's unobserved local key. The cached local key avoids another scalar multiplication at this stage. [Trouble crypto](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/security_manager/crypto.rs#L398-L443), [backend Rust source](https://docs.rs/crate/p256-cortex-m4/0.1.0-alpha.6/source/src/cortex_m4.rs).

The backend build scripts select the assembly/C implementation for targets beginning `thumbv7em` or `thumbv8m.main`; this firmware targets `thumbv7em-none-eabihf`. The local release build output explicitly contains `cargo:rustc-cfg=cortex_m4`. This is not the pure-Rust host fallback. [Backend build script](https://docs.rs/crate/p256-cortex-m4-sys/0.1.0/source/build.rs), [wrapper build script](https://docs.rs/crate/p256-cortex-m4/0.1.0-alpha.6/source/build.rs).

The backend author reports ECDH at 906k cycles / 14.2 ms and at most 2 KiB stack on an nRF52840 with instruction cache and GCC `-O2`. Our firmware uses Rust release `opt-level = "z"`; the retained C object's DWARF producer records GCC `-Os`, and its retained image has roughly 33 KiB nominal space between static allocations and initial stack top within the 96 KiB application RAM window. Neither the author's timing/stack estimate nor our link placement establishes this board's worst-case timing or dynamic stack safety. [Backend benchmark and testing conditions](https://github.com/Emill/P256-Cortex-M4#performance), [our build profile](../../firmware/Cargo.toml), [our RAM layout](../../firmware/memory-sdc.x).

The inspected security manager uses ordinary `RefCell` state and its events use `NoopRawMutex`; the inspected crypto wrappers and assembly do not explicitly mask interrupts. Synchronous execution can delay other tasks in the same executor, but this is not source evidence that radio interrupts are blocked for two seconds. An outer call context, interrupt failure, fault or reset still requires its own evidence. [Security manager](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/security_manager/mod.rs), [assembly implementation](https://docs.rs/crate/p256-cortex-m4-sys/0.1.0/source/P256-Cortex-M4/p256-cortex-m4-asm-gcc.S).

## Bounded next software experiment

Before proposing another device trial, independently build the exact optimized ARM backend against nonsecret published ECDH known-answer vectors and inspect its release symbols, call stack allocations and compiler flags. Compare a package-scoped `p256-cortex-m4-sys` optimization change to `2` against the retained `z` configuration, recording flash/RAM effects without changing any framework API, storage feature or transport behavior. This is a diagnostic comparison, not a justified fix yet. Host fallback vector success cannot validate the ARM assembly backend; even an ARM cross-build cannot measure execution timing or stack high-water use on the keyboard.

Possible causes remain unconfirmed: packet delivery/dispatch failure, backend execution fault, stack/data collision, blocked controller servicing, reset or radio/link loss. The capture stops before f5/f6 address encoding and later DHKey checks, so changing those downstream steps is not the first experiment supported by this trace. No global framework upgrade, security downgrade or extra logger is justified solely by this boundary.


## Offline experiment result

The [independent binary/build review](ble-crypto-build-review.md) now records an exact-retained-ELF experiment. Ten published ECDH vectors pass through the ARM backend and the retained Rust `dh_key` wrapper in functional CPU emulation. Five point-decoder checks also pass. A matching GNU compiler source rebuild under `-Os` and `-O2` passes the same vectors; the scoped optimization difference is only 105 emulated instructions and 8 bytes of stack. This does not support an optimization-only firmware trial.

The local repeatable private harness, frozen vectors and results remain under `.evidence/arm-crypto-experiment/`; generated executables, private image and downloaded compiler are excluded from Git. Before any new transport/scanner change, the root reran `./scripts/check.sh --reclaimed-softdevice --usb-recovery-first`: 42 Python checks, seven scanner tests and all production role/keymap and diagnostic cross-builds passed. These checks do not reproduce the hardware pairing fault.

Next hardware isolation requires no image change: attempt host pairing with the right half OFF and USB unplugged, using a fresh Mac packet capture. A different stopping point would implicate interaction with the split connection; the same stopping point would weaken that explanation. Actual handler/crypto/queue observations remain necessary if isolation does not resolve the failure. No firmware has been changed by this experiment.
