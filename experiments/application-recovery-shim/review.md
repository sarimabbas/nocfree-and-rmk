# Independent application shim review

2026-10-03. Status: passed offline prototype review after correction. No device
commands, firmware writes, or hardware acceptance were performed by this reviewer.

## Corrected blocker

The initial STOP-timeout branch disabled TWIM and used DSB before returning with
stack-owned DMA buffers. That was unsupported: the nRF52833 specification treats
STOPPED as DMA completion and requires it before disabling after STOP. A CPU
barrier does not establish peripheral completion. The corrected branch calls a
non-returning factory recovery/reset function while the buffers remain alive.
Ordinary NACK/timeout with subsequently confirmed STOPPED still falls back to RMK.
A permanently stuck bus therefore enters recovery rather than normal typing; this
is an explicit exceptional behavior, now accurately documented.

## Verified source properties

- Pre-init adapter owns stack TX/RX arrays, contains no initialized statics, heap,
  HAL startup, NVMC, flash, UICR or radio accesses. Transmit/receive pointers remain
  valid through STOPPED or the non-returning reset path. Both lists are disabled.
- TWIM0 base40003000; TIMER1 base40009000; GPIO P1 base50000300 match pinned
  nrf-pac0.4.0 nRF52833 definitions. P0.11/P1.09 PIN_CNF offsets, S0D1 drive6,
  pull-up3, enabled value6, DMA pointers/counters, 100kHz frequency, repeated-start
  LASTTX_STARTRX7/LASTRX_STOP12 and transmit LASTTX_STOP9 match definitions and
  Nordic's register specification. USBREGSTATUS438 and GPREGRET51c are POWER
  offsets; AIRCR reset uses VECTKEY and retains only PRIGROUP.
- GPIO CNF values are saved/restored. TWIM is disabled, PSEL disconnected, DMA
  pointers/counters and events cleared before normal return. TIMER1 is stopped
  and cleared; relevant NVIC interrupts are disabled and pending bits cleared.
  This is clean teardown, not restoration of arbitrary prior peripheral state.
  Exclusive TIMER1/TWIM0 ownership and already-masked interrupts are prerequisites.
- PCA9555 configuration6/7=ff and polarity4/5=0 match the working scanner's input
  assumptions; input0/1 reads combine active-low bits into the role-local chords.
- Policy rejects noUSB, first scan without chord, incomplete/NACK scans, release,
  USB loss and time/iteration cap. Hold timing uses wrapping subtraction. Extra
  simultaneous held keys are intentionally allowed. USB absence avoids setup.

## Repeatable verification

Reviewer independently ran7 injected host policy tests and built the current
no_std ARM object for thumbv7em-none-eabihf with opt-level=s; both passed. These
checks do not exercise real registers, DMA electrical behavior, or recovery.

Integration must still verify __pre_init placement before BSS/data initialization,
actual linked control flow and generated helper calls, per-role builds, marker
policy, image bounds, and the installed image's guards. Hardware acceptance must
include ordinary USB/battery startup, cold/reset held chord, release rejection,
repeat recovery, RMK scanning after teardown and split typing/wake. The failed
legacy-TWI bootloader gate is not evidence that this TWIM adapter works.

Primary sources: [nRF52833 TWIM](https://docs.nordicsemi.com/r/bundle/ps_nrf52833/page/twim.html)
(STOPPED/stop-disable ordering, EasyDMA, shortcuts/register table);
[TI PCA9555](https://www.ti.com/lit/ds/symlink/pca9555.pdf) (input/configuration/polarity).
Pinned primary register definitions: nrf-pac0.4.0 nRF52833 pac.rs/SVD used by firmware.

## Integrated entry and guard review

Both root-built role ELFs were independently inspected. In each, Reset begins at
0x1204 with vector PC0x1205 and SP0x20020000; word0x1200 is exactlyffffffff.
Reset's BL at0x120a calls __pre_init0x1268 before either BSS clearing at0x1218
or data copying at0x1226. The hook pushes8bytes preservingr4/LR, savesPRIMASK,
masks interrupts, calls the recovery function, restoresPRIMASK and returns.
Incoming stack alignment is retained. Actual linked role branches test correct
left bits40/0 and right bits42/14.

The reachable early call graphs contain14left and13right functions: only stack
array iterators, integer64shift helper, register-only bus/recovery functions and
outlined integer prologue/epilogue helpers. The reviewer inspected their actual
disassembly; no initialized-global access, allocator, panic, HAL, radio, flash
write routine or floating-point instruction appears in the early reachable path.
The latter matters because Cortex-M runtime enables the FPU after this hook.
STOP-timeout calls the non-returning recovery function without unwinding the
Bus-owning frame. Private disassembly evidence is retained separately.

The opt-in linker reserve occupies only the erased old-marker word, with _stext
starting beyond it and an assertion against vector overlap. Role/marker feature
conflicts fail at build/compile time. Prototype lib references the single
production adapter; duplicated scanner implementation was removed.

The new inspect_application_shim guard accepts only aligned exact BIN plus final
pageFFpadding, ordinary application family, application-range addresses, valid
RAM stack, ThumbPC beyond reserve within exactBIN, erased marker word and absent
S140magic. It intentionally does not claim the shim exists from bytes alone:
linked-entry evidence and source/hash binding must accompany a transfer. The
reviewer ran9guardtests independently; all passed. Seven policytests and
standaloneARMobject passed earlier. These are offline checks, not device recovery.

Current production integration is passed offline review; root's complete
per-role harness/build results and device acceptance remain separate evidence.
No image was installed by this reviewer.

## Reviewed source hashes

- `firmware/src/startup_recovery/mod.rs`: `aa7bbc6ee718da694285bf334e115dfce17490ddaf3fd3fe976be041e83af0a6`
- `firmware/src/startup_recovery/nrf.rs`: `a5787dec07845ecf0d2a05ccfd06e54c88ace1aeefe937d044a7d4e41eccc043`
- `firmware/src/startup_recovery/gate.rs`: `ff3b3a3aa2a70c38ab755835e08cd7ad0313db9f792268805e921db51959bafa`
- `firmware/src/main.rs`: `ebfea876055a6463f9a72161963018be3978c72a2e8400e86042014788c14b6d`
- `firmware/build.rs`: `01cc94d81c43f24554ba0c999c64bc81f62f599d8de827d9ed5bd3a2bb23d91d`
- `firmware/startup-recovery.x`: `668fc290392f2d58f66405356a4d926ae1330a1fda4ec2a7bfbad546756c0790`
- `firmware/Cargo.toml`: `28fb4f689eda9a71fbbc7beed9ad1d0719fb5213d0d2d4660f4f3a0720ae77d7`
- `scripts/migration_guard.py`: `9a20028c4f6dd4ecd35d50caa90ca7b4fcd8380444a71e0e4f55d1ef1384d009`
- `tests/test_application_shim_guard.py`: `5a2b86b79b1f4efc033fab83566502c2e802c46298e6477d3ed2f666a76c60ff`
- `experiments/application-recovery-shim/lib.rs`: `697b6090490545d6749ad48284b4c341b896834227b64870e6a618ebcb1aa732`
- `experiments/application-recovery-shim/README.md`: `e5ea53e80520cd856c5c1c714da85d688fdceae465769ae1f26ff1ff767fc328`
