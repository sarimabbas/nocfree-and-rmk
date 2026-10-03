# Held-key bootloader integration review

Independent source review, 2026-10-03. Scope: portable recovery gate, nRF52833
adapter, pinned-upstream `main.c` patch and DK build-only wrapper. No device was
accessed and no firmware was transferred. This is not a vendor-compatible image
or an installed recovery result.

## Adapter

The adapter uses TIMER1, legacy TWI0, SDA P0.11, SCL P1.09 and POWER VBUS sensing.
Pinned board initialization uses RTC1/SysTick, and the version word occupies
TIMER2 CC0. TIMER1 and the TWI0/TWIM0/SPIM0 alias require exclusive ownership;
optional upstream display configurations using SPIM0 must be excluded. The
adapter rejects an enabled alias before touching it. Its entry and exit disable
interrupts and clear pending IRQs for its owned peripherals.

Source-level RX sequencing agrees with the pinned nrfx TWI driver: suspend at
the first byte boundary, read RXD, change to final-byte STOP and resume. The
PCA9555 register-pointer write and both input bytes share one 4 ms elapsed
deadline. Every polling loop also has a finite iteration limit. Configuration
or read failure cannot yield a partial successful chord.

Cleanup is reached after every initialized-gate result. It attempts bounded
STOP, disables the peripheral even when STOP fails, clears events/errors/IRQ
sources, disconnects the pin selection, releases GPIO inputs and stops/clears
TIMER1. Early no-USB, invalid-role or already-enabled-peripheral returns do not
claim hardware ownership and do not disturb unrelated hardware.

A healthy timer bounds transfer/wait/STOP polling by elapsed time. A failed timer
leaves only a finite instruction-count cap; this is not proof of a 4/10/1 ms
wall-clock bound. The adapter document correctly distinguishes this from the
portable harness's hypothetical 486 ms callback budget. Actual timings and
peripheral handoff still need hardware measurements.

Independently passed strict ARM GCC syntax checking with the pinned nrfx MDK and
CMSIS headers, Cortex-M4/Thumb, C11, `-Os`, and warnings as errors. This establishes
source compilation only. The portable sanitizer harness remains separate from
the register-level adapter; it cannot validate real TWI bus behavior or cleanup.

## Bootloader decision integration

The hook precedes application-skip handling. A successful local chord requests
USB UF2 recovery and overrides GPREGRET skip, serial-only and OTA-reset choices,
including the existing physical DFU+FRESET OTA expression. Held-key
entry uses the no-timeout branch even when the application contains its older
recovery-first marker. Absent/error chords retain upstream validity and software
request behavior. No GPIO, flash, storage, radio or UICR changes belong in the
decision patch.

OTA application jumps can arrive with SoftDevice already running. The patch
explicitly bypasses the raw hardware adapter on that path and retains original
OTA handling; the chord is not probed in that exceptional entry context. On
ordinary cold/reset paths it runs before BLE initialization.

The current application marker remains intact. This patch alone therefore does
not resolve today's ordinary USB recovery behavior. Marker removal is a later
separately guarded application operation, after each actual half demonstrates
independent recovery and normal USB startup with the new bootloader.

## Build wrapper and installation boundary

The wrapper requires the pinned upstream revision and matching initialized
submodules, copies sources into private evidence workspaces and builds only the
explicit pca10100 DK ELF target. It has no packaging, flashing or device command.
The generic DK configuration is explicitly not a NocFree installation candidate.
Tracked and untracked source changes are rejected. Two GCC 15 diagnostic
exceptions are scoped to upstream objects only: intentional fixed-low-address
MBR access and non-NUL-terminated FAT names. The recovery sources retain strict
warnings as errors.

The first smoke builds silently omitted the patch because `git apply` discovered
the enclosing repository outside the copied source tree. Those ELF size results
were not integration evidence and are discarded. The wrapper now applies the
patch directly inside the snapshot and asserts the actual entry call is present;
the rebuilt snapshots contain that call and the physical-OTA priority correction.
Both rebuilt ELFs retain `recovery_gate` and `recovery_nrf_startup`; independently
inspected disassembly shows the actual call from `main`, with role argument 0
for left and 1 for right. Each rebuilt DK ELF reports text 32020, data 676 and BSS
22282 bytes. This establishes integration and fit for the generic DK build, not
vendor board compatibility or its available headroom.

Independently ran `test_integration.py`: both role variants pass strict host C
compilation plus AddressSanitizer/UndefinedBehaviorSanitizer. The harness extracts
the actual patched `check_dfu_mode` function and rejects unpatched source, rather
than implementing a second decision algorithm. It covers local chord priority
over legacy software requests and physical OTA, APPJUM bypass, I/O-error and
invalid-app behavior, existing marker/retained state, serial/UF2 requests, skip,
OTA reset and double-NRST handling. Register/radio/DFU operations are mocked;
these results establish decision flow only, not device behavior.

Upstream's named ELF recipe links and reports size; its UF2/HEX generation and
bootloader self-update recipes are separate targets. A successful ELF link does
not establish vendor CF2 identity, pin/power initialization, original bootloader
rollback evidence or compatibility of the vendor's self-update implementation.
The self-update path installs bootloader code; it does not replace the independent
startup gesture or establish recovery from a bad bootloader replacement.

The receiver remains outside this keyboard-key design. Closed-case genuine reset
after an application hang, deliberate local recovery, missing/stuck expanders,
ordinary cold USB startup, warm retained expander state and peripheral handoff are
required hardware acceptance on each half. No installation or removal of the
existing recovery marker is approved by this review.
