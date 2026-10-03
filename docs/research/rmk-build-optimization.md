# RMK build optimization: flash fit and live stack are separate

Primary-source research, 2026-10-02. The firmware pins RMK `9607aedf343b17dd6b27307583ae80c4f728fbbd` (0.9.0), Trouble 0.8.0, Rust 1.93.1 and `thumbv7em-none-eabihf`. Current upstream documentation may include later changes. No device operations or firmware source edits were performed for this note.

## What RMK officially recommends

RMK's [older 0.7.8 binary-size guide](https://rmk.rs/v0.7.8/guide/features/binary_size_optimization) recommends lowering logging, `panic-halt`, removing the RTT logger and optional features, and an optional nightly `build-std` immediate-abort configuration. Its reported savings came from an STM32 example: they are not a promised nRF52833 saving or stack reduction. The old example includes `col2row`, which is removed from current RMK; do not copy that manifest literally.

The [current upstream guide](https://rmk.rs/main/docs/features/binary_size_optimization) retains those strategies and adds Trouble memory settings. Its tiny-buffer example explicitly applies only to a single peripheral connection; it warns against applying those queue values to a split central or dongle. BLE features require persistent storage. Removing default optional configurator features remains supported, but the current NocFree manifest already disables RMK defaults and omits Vial/Rynk. A blanket storage or central-role removal would lose required pairing/profile/split functionality. Current guide retrieval is a moving snapshot, not the firmware pin.

The pinned [RMK manifest](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/Cargo.toml) explicitly makes dongle/split capacities larger and selects the Cortex-M4 security backend for nRF. Those are intentional functional dependencies, rather than accidental default Vial features. The baseline already sets `DEFMT_LOG = "off"`. Removing only the application's `defmt-logging` feature does not remove independently requested `defmt` features from SDC, MPSL, Embassy and the application logger/panic handler.

## Lowest-impact supported memory candidate

The pinned [Trouble build script](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/build.rs) defaults the packet MTU to 251, but defaults queued GATT notification capacity to 512 unless the packet MTU is explicitly selected through a feature or environment variable. With an explicit MTU it derives notification capacity as `min(MTU - 7, 512)`, accounting for L2CAP and ATT headers. Environment variables take precedence over numeric capacity features.

The local resolved configurations corroborate MTU 251, notification capacity 512, packet-pool size 32, notification queue 8, and RX/TX queues 16. Therefore the first configuration-only experiment should be:

```toml
[env]
TROUBLE_HOST_DEFAULT_PACKET_POOL_MTU = "251"
```

This retains the current packet MTU and every queue/pool count. It selects the already implemented derived notification capacity of 244, removing unreachable payload capacity rather than reducing packet throughput or queue depth. Eight entries imply 2,144 fewer payload bytes per notification queue before layout details, and compiler-generated temporaries can shrink too. The exact static and active-stack effects must be measured from the resulting ELF. Do not equate the arithmetic estimate with a hardware result. This is preferable to shrinking the pool to four packets or the TX queue to two, which the official guide reserves for a monolithic peripheral.

## Compiler alternatives and evidence so far

The [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html) permits `s`, `z`, standard speed levels, LTO and package overrides. Generic code may be optimized in its instantiating crate, so an `embassy-futures`-only override may miss the concrete join polls. A root-package speed override with size-optimized dependencies is a supported subsequent experiment; actual flash and stack must be compared.

Root's current isolated experiments report global optimization level 2 overflows the protected application flash by 9,888 bytes; removing the RMK defmt feature still overflows by 10,496 bytes. Global level 1 overflowed by 119,392 bytes. Global `s` with fat and thin LTO encountered a compiler `Undefined temporary symbol` failure; disabling debug information produced the same compiler failure. These are compile outcomes, not hardware trials. Inspect their bound private artifacts before treating later results as final. The global 2 result and defmt comparison use different feature selections; they are not proof that removing defmt generally increases code size.

The independent [crypto review](ble-crypto-build-review.md) established a 37,740-byte retained active-call subtotal versus 33,744 bytes nominal stack space. Scoped C `-O2` saved only eight stack bytes. Pinning only the outer BLE run future was subsequently tried in an independent isolated copy and left that compiled subtotal unchanged. It is a negative experiment; the earlier proposed outer-pin alternative is not a fix.

Changing `panic-probe` to official `panic-halt` can free flash without changing key behavior, radio or storage; it does lose panic reporting and changes fault-handler behavior. It may make a speed-optimized build fit, but does not itself prove a safe active stack. Nightly `build-std` can optimize core panic paths further, but changes the toolchain/core build and is a larger departure than the documented stable panic handler or explicit-MTU candidate. Debug symbols normally are non-loadable ELF data; stripping them is not an assumed flash recovery method. A successful debug-info-free compile can isolate a compiler emission problem independently of runtime optimization.

## Relevant upstream reports and examples

Rust [issue 99504](https://github.com/rust-lang/rust/issues/99504) remains open and documents unnecessary stack temporaries/memory copying when materializing large async futures. It provides a plausible compiler mechanism, not proof that this exact firmware has that issue or that a particular compiler release fixes it. There is no discovered official RMK pairing issue matching the NocFree captured failure.

Embassy [issue 4367](https://github.com/embassy-rs/embassy/issues/4367) explains that async locals held across awaits belong to the static task future, so their addresses do not measure MSP; the maintainer recommends inspecting static future size and `.bss` separately. That report ultimately involved an older SoftDevice/USB interrupt configuration, so its USB correction is not a justified replacement for the current SDC setup.

The pinned [Elytra nRF52833 central manifest](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf_dongle/central/Cargo.toml) uses `z` and fat LTO, matching the starting compiler policy. Its [memory map](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf_dongle/central/memory.x) supplies all 128 KiB RAM, while NocFree preserves a 32 KiB bootloader-sensitive region. It is the same-chip framework example, not proof of this board's safe stack, vendor recovery, or shipping acceptance. No discovered peer establishes a no-loss guarantee for the NocFree layout.

## Recommendation

First measure the explicit, unchanged 251-byte packet MTU candidate. Preserve all pool/queue counts, storage, two BLE roles and protected memory boundaries. If that alone leaves insufficient stack margin, compare stable `panic-halt` plus a fitting compiler profile or a root-package optimization override. Every candidate must pass the existing harness, all-role builds, image guard and independently reviewed compiled-stack budget before a device trial. None of these source/configuration findings establishes successful host pairing or measured no-loss/wake performance.

## First explicit-MTU build result

The unchanged `z`/fat-LTO left Mac build succeeds with the explicit 251-byte MTU. Its ELF reports 330,160 bytes text, 8,676 bytes initialized data and 53,736 bytes BSS. The completed independent call-chain audit below removes the identified overlap; section totals alone do not prove a safe stack or working pairing. The separate official `panic-halt` plus global level-2 comparison still overflows FLASH by 9,824 bytes, only 64 fewer than its matching 9,888-byte baseline. Neither a panic-handler nor optimization-profile change is being adopted.

## Independent review of the integrated explicit-MTU change

The tracked firmware diff is exactly the documented `TROUBLE_HOST_DEFAULT_PACKET_POOL_MTU = "251"` line and its explanatory comment. It leaves the compiler profile, hardware pins, RAM/flash map, storage feature, packet-pool count and RX/TX/notification queue counts unchanged. The resulting local Trouble build configuration verifies pool MTU 251, notification capacity 244, pool count 32, RX/TX queues 16 and notification queue 8.

The source bound is structural: Trouble advertises ATT MTU `P::MTU - 4`, and a Handle Value Notification/Indication has a one-byte opcode plus two-byte attribute handle. Thus the maximum value payload for the selected pool is `251 - 4 - 3 = 244`. The [notification handler](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/gatt.rs) copies the value into `Notification<NOTIF_MTU>`. Its old 512-byte container cannot enable a notification larger than the underlying packet/ATT limit. The [pinned split messages](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/mod.rs) enabled here contain bounded key positions/press state, pointing axes, LED/layer state, six-byte addresses, connection state and battery state; none needs a 244-byte value. Optional split firmware-chunk/custom-message features are not enabled in this image. This does not shrink available queue depth or introduce a lower packet MTU, and the standalone-peripheral tiny-buffer recommendations were not applied.

The experiment agent independently reports the resulting immutable candidate's known foreground pairing chain, including main/executor frames, at 29,068 bytes against 35,888 bytes nominal stack gap: 6,820 bytes remain before interrupts and other execution paths. The exact ELF SHA-256 is `91a8ffbd4d644495df026005169a2373bb5a17f35ff7cf64492f905b779f5468`; root retains its private bound audit. This configuration/source review corroborates the unchanged capacities and lawful payload bound; the separate binary audit owns the frame measurements. Cross-build/known-answer success does not establish hardware pairing or whole-firmware stack high-water safety.

Future RMK/Trouble upgrades must recheck the generated MTU/capacity values, actual selected packet-pool type, enabled optional message features and compiled frame chain. If the intended packet MTU is increased, the explicit environment value must be increased with it; it should not silently keep a later desired MTU at 251. Disconnect/release recovery, simultaneous split input, host pairing and wake/latency acceptance remain device tests, not implications of this source review.
