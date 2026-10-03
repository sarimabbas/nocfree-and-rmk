# Application recovery shim prototype

This is an application-entry experiment, not bootloader code. The source lives at the board scanner seam under
`firmware/src/startup_recovery`; firmware integration is opt-in. The factory bootloader remains untouched.

Integration: put `nrf::check(false)` (left) or `check(true)` (right) in the
application's `__pre_init` hook before data/BSS initialization. Receiver has no
local key scanner and must not run this hook. The hook uses only stack buffers,
volatile peripheral accesses and immediate constants. No HAL/SystemInit, UICR,
radio, storage, allocation, interrupts or initialized statics are needed.

The old application recovery marker at offset 0x200 must be absent for ordinary
USB startup. Do not remove it until this hook has been tested on hardware with
physical reset available. The shim survives later RMK failures only if execution
reaches its intact application entry. It cannot survive corruption of itself,
the vector table, or the factory bootloader, nor detect keys before that
bootloader chooses to launch the application.

With USB present, complete scans must contain left Fn+Escape or right
Fn+Backspace for 60 ms. Other held keys are allowed. An absent chord adds only
initialization and one scan; no USB bypasses the bus entirely. Initialization,
NACK, incomplete transfer, key release, USB removal, deadline and stalled-clock
failure all return to the application if the bus stops cleanly. A fatal bus that
does not acknowledge STOP enters factory recovery instead: continuing would
leave EasyDMA ownership of stack buffers uncertain. The scanner remains RMK's scanner after
the hook returns; this does not implement key debounce or keyboard behavior.

The adapter uses TWIM0 repeated-start EasyDMA, matching the working async scanner
rather than reusing the failed legacy-TWI bootloader gate. Both transmit and
receive buffers live in the stack-owned bus object through teardown. Each
transaction has a 4 ms deadline plus a bounded STOP attempt. Only confirmed
STOPPED permits disable and buffer reuse. If STOP times out, a non-returning
system-reset path requests factory recovery, keeping all DMA buffer stack frames
alive until reset. A DSB or peripheral disable alone is not treated as EasyDMA
completion. Source review and cross-compilation do not establish the actual
electrical outcome.

Pins are P0.11 SDA/P1.09 SCL at 100 kHz, open-drain with pull-ups. All three
PCA9555s are configured as inputs with polarity inversion off, matching
`crates/nocfree-input`. GPIO configuration is restored after the check, TWIM and
TIMER1 are stopped, interrupts/events and DMA pointers cleared. Exclusive
TIMER1/TWIM0 ownership and masked interrupts are prerequisites. The pre-init
linker placement and disassembly must be independently checked during firmware
integration; exported hook symbols and image marker handling are root-owned.

Recovery sets the factory bootloader's already observed GPREGRET request 0x57,
then resets through AIRCR with DSB. It writes no bootloader pages or UICR.

Repeatable host policy tests (fault injection and timer wraparound):

```sh
rustc --test --edition=2021 firmware/src/startup_recovery/gate.rs -o /tmp/nocfree-shim-tests
/tmp/nocfree-shim-tests
rustc --crate-type lib --edition=2021 --target thumbv7em-none-eabihf -C opt-level=s --emit=obj -o /tmp/nocfree-shim.o experiments/application-recovery-shim/lib.rs
```

These tests cover policy, not hardware register behavior or physical recovery.
Required acceptance before marker removal: ordinary USB and battery startup,
held chord on genuine cold/reset startup, release rejection, repeat recovery,
normal RMK scanning after teardown, bus failure fallback and typing/split wake.

Primary references: [Nordic nRF52833 TWIM specification](https://docs.nordicsemi.com/bundle/ps_nrf52833/page/twim.html)
specifies repeated-start shortcuts, RAM-only EasyDMA buffers, STOPPED completion,
pin configuration and stop/disable ordering. [TI PCA9555 datasheet](https://www.ti.com/lit/ds/symlink/pca9555.pdf)
specifies the configuration, polarity and input register pairs. Register offsets
were checked against pinned `nrf-pac` 0.4.0 nRF52833 definitions already used by
this workspace. The previous held-key bootloader gate failed hardware trials;
this alternative is a prototype, not evidence that the previous cause is fixed.
