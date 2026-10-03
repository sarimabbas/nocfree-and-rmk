# Independent review of the retained ARM crypto build

Reviewed 2026-10-02. This review changes documentation only. It examines the retained hardware-tested left image in `dist/full-left-ble-name/candidate.elf`, SHA-256 `5a5816a04041e8674fe50910df833b6f55238b47ab72ea8cfcf4b822c4aecaee`, and the Cargo-resolved `p256-cortex-m4` 0.1.0-alpha.6 / `p256-cortex-m4-sys` 0.1.0 sources. The disassembly is private under `.evidence/crypto-independent-review/`.

## What the binary establishes

The actual `SecretKey::dh_key` implementation calls `p256_octet_string_to_point` at `0x33eec`, then `p256_ecdh_calc_shared_secret` at `0x33f0e`. This agrees with the [pinned Trouble source](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/security_manager/crypto.rs) and confirms that this image uses the ARM C/assembly implementation, rather than the host fallback. The [backend build script](https://docs.rs/crate/p256-cortex-m4-sys/0.1.0/source/build.rs) selects that implementation for `thumbv7em` and forces `-march=armv7e-m`; the application target is `thumbv7em-none-eabihf`.

The retained assembly contains `UMAAL` and floating-point register moves/loads using `s0`–`s15`. The [backend configuration](https://docs.rs/crate/p256-cortex-m4-sys/0.1.0/source/P256-Cortex-M4/p256-cortex-m4-config.h) enables those FPU instructions when the compiler defines `__ARM_FP`. The image's reset entry enables CP10/CP11 in CPACR at `0x1238`, then executes DSB and ISB before entering `main`. This rules out an image that simply forgot to enable the FPU at startup; it does not establish that later code preserves that configuration or that every interrupt path preserves live state correctly. The inspected crypto range contains no `CPSID`, `CPSIE`, or PRIMASK write.

The [Arm procedure-call standard](https://github.com/ARM-software/abi-aa/blob/main/aapcs32/aapcs32.rst) permits caller-clobbered `s0`–`s15`, requires 4-byte stack alignment universally and 8-byte alignment at public interfaces. Inspected Rust/C public-call frame deltas preserve 8-byte alignment. Private assembly helpers sometimes use temporary 4-byte stack alignment; this alone is not a demonstrated ABI defect. The [backend API](https://github.com/Emill/P256-Cortex-M4#api) requires 4-byte alignment for its `uint32_t` arrays. Rust's wrapper stores scalar and point coordinates in `[u32; 8]`; the retained caller places its point buffers at 4-byte-aligned stack offsets. No concrete alignment fault was found.

## Stack evidence and limits

Local frame sizes, including saved core registers, read from the retained disassembly:

| Function | Bytes retained while its callees run |
| --- | ---: |
| `Pairing::handle_input` | 2,536 |
| `SecretKey::dh_key` | 296 |
| `p256_ecdh_calc_shared_secret` | 88 |
| `p256_scalarmult_generic_no_scalar_check` | 40 |
| `scalarmult_variable_base` | 1,104 |

That active chain accounts for 4,064 bytes before its nested assembly helper frames, outer executor frames and interrupt/exception frames. It is not a full worst-case stack bound. `__sheap`/`_stack_end` is `0x20017c30`, initial stack top is `0x20020000`: the nominal static-to-stack gap is 33,744 bytes. The backend author's [2 KiB stack / nRF52840 benchmark](https://github.com/Emill/P256-Cortex-M4#performance) describes the crypto implementation, not the entire Trouble call chain, and cannot be substituted for this firmware's dynamic high-water measurement.

The source's `opt-level = "z"` propagates to the C build through `cc`; the assembly algorithm is not reoptimized by that C flag. C scalar multiplication includes finite fixed-count loops and calls the handwritten field arithmetic. A scoped `-O2` comparison can quantify compiler-generated C instruction changes and flash/frame differences. Source alone does not justify a two-second compute-time claim, an optimization fix, or a claim that radio interrupts were masked. The installed keyboard's timing remains unmeasured.

## Interpretation

No specific target-instruction, FPU-startup, public-call alignment or static RAM-overlap bug was identified. The missing public-key response still brackets peer-packet reception/dispatch, public-point checks, synchronous ECDH and response queueing. A passing exact-ARM known-answer experiment would weaken an arithmetic/backend-codegen explanation for its tested inputs; it would not validate the board's interrupts, radio scheduling, runtime stack, or the Mac packet's delivery to the handler. Hardware acceptance still requires another captured pairing attempt after a narrowly justified change.

## Dispatch and response-queue review

The inspected call path is `Host::handle_acl` → `ConnectionManager::handle_security_channel` → `SecurityManager::handle_l2cap_command` → peripheral state handler. The connection list and security-manager state use ordinary `RefCell` borrows, and outbound/security channels use `NoopRawMutex`. No explicit critical section surrounds ECDH in these functions. A synchronous computation still occupies the current executor poll; that is distinct from blocking radio interrupts. [Host dispatch](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/host.rs), [connection manager](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/connection_manager.rs), [security manager](https://github.com/embassy-rs/trouble/blob/42e3e04de00db3951b126741e1f3602bfdf5df47/host/src/security_manager/mod.rs).

The response path allocates a packet and uses nonblocking `try_outbound`. Exhausted packet storage or a full outbound queue returns `OutOfMemory`; the error path then attempts a Pairing Failed packet through the same resources. Therefore resource exhaustion can prevent both the public-key reply and the failure reply. This is a source-supported possibility, not an observed failure. If exact retained ARM backend and Rust-wrapper known-answer execution passes, the next useful observation is handler entry, ECDH return and response-enqueue result rather than a downstream f5/f6 change or an unexplained compiler optimization switch.

## Independent replay of the exact ARM experiment

The reviewer inspected and independently reran the experiment agent's private harness against the retained ELF. Ten selected valid [Wycheproof SECP256R1 ECDH vectors](https://github.com/C2SP/wycheproof/blob/master/testvectors_v1/ecdh_secp256r1_test.json) passed on Unicorn 2.1.4's Cortex-M4 functional emulator with the FPU enabled. The frozen vector file SHA-256 was `cdb8bd5d1206fddb6618c69ffa18f303b4752caba321d348d4aacae3f20cbec4`. The harness converts the public coordinates and private scalar to the backend's native little-endian words and compares the returned big-endian shared secret to the published expectation. It requires the return sentinel to be reached, rejects an empty vector selection, and verifies preservation of core registers `r4`–`r11` and FPU registers `s16`–`s31`.

Every selected execution returned correctly, preserved those registers, ran 663,513 emulated instructions and descended 1,524 bytes from its starting SP. This measures only the directly invoked ECDH routine and its callees. Adding the retained `handle_input` and `dh_key` frames accounts for 4,356 bytes before other outer and interrupt frames; this is still not a hardware stack bound. Instruction counts are not CPU cycles or board timing. No interrupts, controller peripheral, radio or executor were modeled. The private Mac public point and the keyboard's ephemeral secret were not inputs to these public known-answer tests.

The reviewer also independently replayed the exact retained Rust `SecretKey::dh_key` wrapper, whose calling convention was checked against its disassembly: result pointer in `r0`, native scalar pointer in `r1`, remote big-endian coordinate bytes in `r2`, cached local point in `r3`. The harness derives the matching local point by executing the retained `p256_keygen`, supplies separate result/input buffers, and flushes Unicorn's translation cache after registering the measurement hook and before wrapper execution. Ten vectors passed, with return and callee-saved register checks, at 665,840 instructions and 1,820 bytes SP descent. That includes the 296-byte wrapper frame and 1,524-byte ECDH chain; adding the retained pairing-handler frame gives the same 4,356-byte subtotal. This tests the wrapper's point checking and byte-order conversion as well as ECDH, but not live SMP dispatch or interrupt preemption.

The separate source-build experiment uses Arm GNU 15.2.Rel1, hard-float Cortex-M4 instructions and identical sources/flags except C optimization. The cached object's producer establishes that Cargo's `z` became GCC `-Os`, rather than a literal GCC `-Oz`. The reviewer inspected the recorded build commands and result reports: both `-Os` and `-O2` passed ten vectors. Their respective counts were 658,827 / 658,722 instructions and 1,524 / 1,516 bytes SP descent. The reduction is only 105 instructions (about 0.016%) and 8 stack bytes. Those rebuilt images link newlib helpers, so compare the pair with each other rather than treating their totals as exact retained-firmware counts. This experiment provides no support for changing the firmware optimization to solve the observed two-second link timeout. The reviewer independently replayed the retained binary and wrapper; the rebuild comparison was reviewed from the experiment agent's bound commands and result reports.

## Expanded active-stack finding after the pairing stall

The preceding arithmetic/ABI review covered the crypto boundary and explicitly excluded outer executor frames. A subsequent owner-observed loss of USB typing after the failed host pairing prompted a review of those frames. This expanded binary review identifies a concrete static stack-budget problem that the crypto-only subtotal missed.

The retained image has the following direct `BL` call chain. Caller frame allocation remains in place at each listed call; inspected stack-restoration branches lead to return epilogues, rather than continuing into the callee.

| Retained caller | Callee call instruction | Caller frame bytes |
| --- | --- | ---: |
| Main `TaskStorage::poll` (`0x16bf8`) | `0x18836` → `Join::poll` | 2,904 |
| `Join::poll` (`0x2852c`) | `0x2bdc2` → `Join3::poll` | 13,800 |
| `Join3::poll` (`0x158c`) | `0x2f3a` → RMK BLE task | 14,720 |
| RMK BLE task (`0x1b908`) | `0x1bc02` → Trouble RX runner | 920 |
| Trouble RX runner (`0x1197c`) | `0x141d4` → pairing L2CAP handler | 952 |
| Pairing L2CAP handler (`0xf058`) | `0xf076` → `Pairing::handle_input` | 88 |
| `Pairing::handle_input` (`0xcc34`) | `0xd0b8` / `0xd4b2` → `dh_key` | 2,536 |
| `dh_key` plus its ECDH callees | Independently emulated retained wrapper | 1,820 |

The subtotal is **37,740 bytes**, exceeding the entire 33,744-byte nominal static-to-stack gap by **3,996 bytes**, before the executor's own outer frame or any interrupt frames. Even assuming entry at the RAM ceiling `0x20020000`, that chain reaches `0x20016c94`, below `__ebss = 0x20017830` and the uninitialized RTT buffer ending at `0x20017c30`. Large temporary allocations in the two nested asynchronous join polls dominate the budget. This is a retained-binary call-chain calculation, not a device stack-pointer measurement. It establishes that the reviewed chain cannot fit above statically allocated data; it does not identify the exact overwritten object, prove which instructions the failed hardware attempt reached, or establish its precise fault mechanism.

This supersedes the earlier absence of a demonstrated static overlap in the narrower crypto-only review. Passing standalone ECDH and `dh_key` functional emulation does not address this outer-stack problem. The scoped P256 `-O2` experiment saves only 8 bytes and cannot eliminate a deficit of thousands. The next software experiment should reduce or isolate the nested poll-frame pressure using existing framework execution boundaries, then compare the actual compiled call-chain budget before any device trial.

The reset entry enables the FPU but does not visibly configure FPCCR. The current source does not explicitly set ASPEN/LSPEN or verify the bootloader's inherited lazy-stacking configuration. No runtime FPCCR or CONTROL value was read. Therefore interrupt FPU-state preservation remains unverified; the stack finding requires no assumption that lazy stacking is broken. Interrupt frames can only increase the required space beyond this subtotal, and functional emulation performed without interrupts cannot validate them.

## Minimal compiler/configuration and framework-boundary alternatives

The [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html) supports global `s` size optimization and `thin` LTO as ordinary configuration experiments. Package overrides require care: generic code can be generated in the crate that instantiates it, so optimizing only `embassy-futures` does not reliably target this application's concrete join polls. A root-package optimization override with size-optimized dependencies is another compile-only comparison if global speed optimization does not fit flash. Each result needs its actual flash usage and active compiled stack chain checked; no setting promises improvement.

If profile changes cannot produce a fitting image, the pinned RMK already exposes the needed execution boundary without a framework fork. Its [run_all macro](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/input_device/mod.rs#L81) only imports `Runnable` and passes each `.run()` future to [join_all](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/helper_macro.rs). A bounded board-wiring experiment can pin `ble.run()` in place with `core::pin::pin!` and pass its `Pin<&mut Future>` to the same `rmk::join_all!`, with every other runnable in exactly its existing order. This retains RMK's own run futures, join implementation and polling order; it introduces no allocator, static-lifetime conversion or custom USB/BLE implementation. The hypothesis is that the outer join's `MaybeDone` then holds a small pinned reference rather than a large owned BLE future, avoiding its large move/drop temporaries. It must be cross-built and disassembled before being called a fix. Internal RMK joins remain unchanged.

## Compile-only profile results

Using the current pinned source, Rust 1.93.1 / LLVM 21.1.8, the left Mac feature set and unchanged application/RAM/storage limits:

| Override | Result |
| --- | --- |
| Global `opt-level = 2` | Link rejected: final loadable section exceeds FLASH by 9,888 bytes |
| Global `opt-level = "s"`, existing fat LTO | Compiler rejected: `Undefined temporary symbol` |
| Global `opt-level = "s"`, thin LTO | Same compiler rejection |
| Global `opt-level = "s"`, debug information disabled | Same compiler rejection; disabling DWARF did not isolate it |
| Root-package-only `opt-level = 2`, dependencies at existing `z` | Link rejected: final loadable section exceeds FLASH by 49,888 bytes |

These isolated build overrides changed no tracked firmware source and produced no candidate eligible for a device trial. The flash boundary was not relaxed, RAM origin was not lowered, and no firmware was copied. The next experiment is the pinned-reference RMK execution boundary described above.

## Supported MTU configuration removes the identified overlap

The independent isolated application-level BLE pin experiment compiled but retained the same 37,740-byte pairing subtotal. The proposed pin alternative above is therefore not a fix. DWARF maps the 13,800-byte outer join frame to inlined `BleTransport::run` initialization, and the 14,720-byte inner join to RMK's BLE host/split-session/scan join. Pinning the application-level BLE future does not remove those internal construction temporaries.

The supported `TROUBLE_HOST_DEFAULT_PACKET_POOL_MTU = "251"` setting leaves the existing packet MTU, packet-pool size, RX/TX depths and split notification count unchanged. The pinned Trouble build script then derives the notification payload buffer as 244 bytes instead of its implicit 512-byte fallback. No optimization profile, panic handler, RAM origin, flash boundary or transport implementation changes are adopted. See the [primary-source build research](rmk-build-optimization.md).

The independent audit of the resulting left Mac ELF found the affected frame sequence `2904 + 9512 + 10312 + 920 + 952 + 88 + 2536 + 1820 = 29,044` bytes. The verified outer main/executor frames add 24 bytes, for **29,068 bytes** of known foreground stack. Static allocation ends at `0x200173d0`, leaving **35,888 bytes** below the initial stack pointer; the affected path therefore has **6,820 bytes of remaining headroom before interrupt overhead**. Its lowest calculated foreground SP is `0x20018e74`. Flash fits, and exact-binary crypto-wrapper replay passes ten published vectors with ABI preservation. The repeated host harness and all supported role/keymap cross-builds pass with the configuration applied.

This removes the demonstrated overlap in the reviewed compiled pairing path. It does not establish interrupt high-water usage, safety of every async branch, successful host pairing, no-loss input, or hardware latency. Following a battery-first restart, the owner confirmed left USB `qwert` typing again on the retained earlier image; the explicit-MTU image has not been installed.
