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
- Policy rejects first scan without chord, incomplete/NACK scans, release and
  time/iteration cap. Hold timing uses wrapping subtraction. Extra simultaneous
  held keys are intentionally allowed. Every keyboard startup now scans once;
  USB detection is recorded only as diagnostic metadata.

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

## Feature-gated startup diagnostic review

The left diagnostic ELF was independently inspected at SHA256
`6aea6b5f5ffbebf3efcebd4254b6e1b0199650448569da1e89d86e366298777a`.
The 48-byte trace is in NOLOAD `.uninit` at `0x20017290`, exactly after
`__ebss`, outside data/BSS initialization. Pre-init first writes all twelve
words, then reset/USB/ownership observations and the magic last, before any
branch can return. The linked initializer contains only volatile register/trace
writes and stack operations. Runtime descriptor access reads initialized trace
words and returns short immutable strings; no trace writes occur after handoff.

Reset still calls the hook before BSS/data initialization and FPU enablement.
The hook preserves PRIMASK and stack alignment. The thirteen reachable early
functions, including outlined tail epilogues, contain only integer operations,
stack array helpers, registers and the uninitialized trace. No initialized-data,
HAL, allocation, panic, NVMC, UICR or floating-point dependency was found. STOP
failure still resets without dropping DMA-owning frames. The trace is overwritten
on every startup: a fatal reset reason does not survive into a later RMK session.

Ten host tests independently passed, covering policy/reason distinctions,
initialized trace handoff and label selection. NACK labels mask ERRORSRC bits1/2;
overrun bit0 alone remains a generic failure. These diagnostics do not establish
which electrical fault occurred or prove held-key recovery. Actual installed
readback and physical startup/typing/recovery checks remain separate.

## USB-independent startup correction

The diagnostic hardware observation `boot:noUSB` established that early VBUS
sampling skipped the held-key scanner. This revision removes both the initial
VBUS condition and the policy's repeated USB checks. Every keyboard startup
initializes and reads one complete snapshot; absent/partial chord returns to
RMK immediately after that snapshot. A continuously held local chord for60ms
requests factory recovery. Release, incomplete scan, initialization failure,
200ms policy deadline and128-iteration cap still reject; fatal STOP handling is
unchanged. Ten host tests independently passed, including initialization time
counting against the deadline. Physical acceptance remains pending.

Final left ELF SHA256 is
`fca216edd9e0cb3628a3a3eeb3302eee859be5ba64de0fedf8df32389496e103`.
Its thirteen early reachable functions were inspected again. USBREGSTATUS is
read once into the initialized trace and no longer controls startup. Correct
left chord bits40/0, the erased reserve, pre-BSS hook and trace NOLOAD placement
remain intact; allocated ELF sections match the guarded binary exactly.

The original factory bootloader binary reads GPREGRET at0x745a6 and accepts
0x57 at0x745e0..0x745e2 without a VBUS requirement. The matching0x57 timeout
path at0x7480c..0x7481c passes3000ms; upstream check_dfu_mode documents return
to the application when USB fails to enumerate. A battery-start held chord can
therefore repeat recovery after each timeout until released or USB connects.
This is not permanent battery-only recovery, and the new shim's electrical
behavior is not established by that bootloader analysis.

## Reviewed source hashes

- `firmware/src/startup_recovery/mod.rs`: `dc7cc24656d9c6513e15de8c8851d17581a676312dbbbd1e4b057e207b42ebda`
- `firmware/src/startup_recovery/nrf.rs`: `d1f7496ea60929fea04e38b7ef26087bdb206eb1d1903b59896f4e00fe8c54d7`
- `firmware/src/startup_recovery/gate.rs`: `d5f41bff73dc05a4203ea0a31df7e131ae6f86d8a9e4b920728f365a8077f8b4`
- `firmware/src/startup_recovery/trace.rs`: `b38ab8efbaa490e9bb196d12f684bd2f6a4bdb5065ce9d1fb8e3e37194713742`
- `firmware/src/main.rs`: `ad8f3e637243ac7166c35f37fefd914d9dd9b2f8401ff705d73341b816959a06`
- `firmware/build.rs`: `01cc94d81c43f24554ba0c999c64bc81f62f599d8de827d9ed5bd3a2bb23d91d`
- `firmware/startup-recovery.x`: `668fc290392f2d58f66405356a4d926ae1330a1fda4ec2a7bfbad546756c0790`
- `firmware/Cargo.toml`: `31fc521bcb772ac43c67ef7adc58d8a9150faade27e829ec57aba2613bec5ec6`
- `scripts/migration_guard.py`: `9a20028c4f6dd4ecd35d50caa90ca7b4fcd8380444a71e0e4f55d1ef1384d009`
- `tests/test_application_shim_guard.py`: `5a2b86b79b1f4efc033fab83566502c2e802c46298e6477d3ed2f666a76c60ff`
- `experiments/application-recovery-shim/lib.rs`: `697b6090490545d6749ad48284b4c341b896834227b64870e6a618ebcb1aa732`
- `experiments/application-recovery-shim/README.md`: `4da75a67c64d55ba782a9054dc6029229574d6df019fbb59312ca779c44c4abb`
- `experiments/application-recovery-shim/check.sh`: `be8a14c867e2e52a16977e4326fc8126cdf834db6e62c7c1a2aaba5df7e2d02e`
