# Held-key startup recovery experiment

Offline prototype, not installed firmware and not a flashable bootloader. No device operations, flash writes, installed RMK changes, or image-guard changes are part of this experiment.

`recovery_gate.c` is portable C intended to run before launching the application. It reads local PCA9555 inputs through hardware callbacks. It requests USB recovery only if USB power is present and a startup chord remains sampled as held for at least 60 ms. Missing initial keys or a subsequent sampled release returns immediately to ordinary startup. I2C failure returns a distinct error, never a fabricated pressed-key state. Three expanders must produce complete snapshots; a partial snapshot cannot enter recovery. Other held keys do not prevent recovery.

The nominal startup deadline is 200 ms as measured by the adapter clock, with an additional sample-count bound for a frozen clock. It is not a hard 200 ms wall-clock guarantee. Per-transfer callbacks must honor a 4 ms timeout; a C caller cannot rescue a callback that never returns. The hardware adapter is responsible for the timeout, monotonic clock, USB VBUS sensing, and bus cleanup. The software policy checks deadlines between transfers; one transfer already started immediately before the deadline can finish up to 4 ms afterward. A wait already started before the deadline can cross it by up to 10 ms. With a frozen clock, the cap is 69 bus transfers and 21 waits: at most 486 ms under the callback contracts, plus bounded clock/USB callback overhead. The wait callback must not exceed its requested 10 ms. These are explicit adapter requirements, not proven device properties.

## Local physical chords

| Role | Physical chord | Expander bits |
| --- | --- | --- |
| Left | Fn + Escape | 40 + 0 |
| Right | Fn + Backspace | 42 + 14 |

Mappings are derived from the existing `firmware/src/keymap.rs` ANSI order and `crates/nocfree-input/src/lib.rs` electrical order. Left Fn is logical 32 mapped through LEFT_BITS to 40; Escape is bit 0. Right Fn is global logical 79 minus right offset 37, and Backspace is global 51 minus 37; RIGHT_BITS is identity over 0–46. They do not depend on user keymaps or RMK. The startup chords themselves have not been validated on hardware.

The input register pair is read little-endian from register 0. Each PCA9555 is configured with both configuration registers set to input (`6,255,255`), and both polarity-inversion registers cleared (`4,0,0`). Active-low board input interpretation and 0x20/0x22/0x24 ordering match the current scanner. The [TI PCA9555 datasheet](https://www.ti.com/lit/ds/symlink/pca9555.pdf) specifies input/configuration/polarity registers and sequential register-pair access. Confirm the actual fitted component and power-up settling time before hardware use.

## Upstream integration design

The reported bootloader base is [Adafruit revision 0147d71, main.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c). Its existing `check_dfu_mode()` distinguishes software requests, physical DFU input, reset-pin double reset, and application single-tap reset marker. The vendor bootloader binary has not been proven identical to upstream source.

A board-specific integration would:

1. Initialize the GPIO/I2C/monotonic-clock adapter before application entry, after hardware setup. Read USB VBUS presence; do not require enumeration or a mounted UF2 drive.
2. Run this gate on both normal power-on and reset, making USB power plus the local chord deliberate maintenance entry. Keep existing software-request and invalid-application recovery paths intact. Decide explicitly whether intentional application “skip DFU” requests should override a physically held recovery chord; this prototype assumes the chord is authoritative but does not patch upstream priority yet.
3. On `ENTER_USB_RECOVERY`, choose the existing UF2-capable DFU route and stay there until installation/restart, rather than applying the old three-second single-tap timer.
4. On `START_APPLICATION`, follow ordinary upstream validity/start logic. On `RECOVERY_IO_ERROR`, record/report the diagnostic as possible without relying on RMK, then follow the same ordinary validity/start logic. Never enter automatic recovery on a mere bus fault when a valid application exists.
5. Stop I2C and restore owned pins/peripheral state on every exit before USB DFU or application launch. No expander output-drive configuration, interrupts, radio, debounce engine, storage or flash operations belong in the gate.
6. Remove the application's recovery-first marker only in conjunction with a bootloader and image-policy migration that has passed real, independent recovery tests. The installed firmware and current guard remain unchanged here.

No nRF adapter or upstream patch is included. A pseudo-adapter would hide the important remaining work: independent bounded I2C when a bus is stuck, reliable USB VBUS sensing before enumeration, actual power/reset behavior, peripheral cleanup, bootloader build-size/link constraints, and installation/recovery of the bootloader itself. Cold USB, warm reset, battery-to-USB, USB charging while the switch is OFF, and each half's held chords must be observed on hardware. A right-half switch may isolate battery while USB still powers the MCU; “OFF then ON” cannot be assumed to reset it.

The dongle is excluded: it has no local keyboard keys and needs its own independently verified recovery design.

## Repeatable host harness

Run `./experiments/held-key-recovery/check.sh`. It compiles the actual C policy with C11 strict warnings and AddressSanitizer/UndefinedBehaviorSanitizer, runs the fault harness, and removes its temporary executable.

Coverage includes both real bit masks, wrong/incomplete chord, ordinary no-key startup, no USB without bus access, sampled release, USB loss including during configuration, a late chord rejected after an absent initial chord, each missing right key, uint32 clock rollover, each of 27 I2C transfer-failure positions through the stable-chord window, transfer timeout, worst permitted transfer duration, all inputs held, invalid API inputs, and a frozen clock. These establish host policy behavior only. They do not establish bootloader buildability, device recovery, latency, pin correctness, or installation safety.
