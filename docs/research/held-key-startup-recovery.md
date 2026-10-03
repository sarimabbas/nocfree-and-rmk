# Held-key recovery before RMK

Research and proposed implementation, 2026-10-03. No device writes or hardware validation performed here. The owner wants ordinary USB startup to run the keyboard, and intentional local-key startup to enter recovery even when RMK crashes. Implement this in a board-specific Adafruit bootloader extension; an RMK shortcut or additional application marker cannot provide that independence.

## Concrete implementation seam

Retain the reported upstream bootloader base [`0147d71`](https://github.com/adafruit/Adafruit_nRF52_Bootloader/tree/0147d71e73b9a2c217f56dbc9877d07bb45d6467), existing MBR handoff and USB update machinery. Add one bounded `board_startup_recovery_requested()` hook before the application-skip return in [`check_dfu_mode()`](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c). A held chord must override `GPREGRET=0x6d`; explicit host DFU requests and invalid-application recovery remain intact. Held-key entry uses untimed USB recovery, not the application's three-second recovery-marker branch. No BLE, RMK, dynamic keymap, radio, battery measurement or lighting engine belongs in this hook.

Remove the application recovery-first marker only after both independent held-key routes pass hardware acceptance. A new bootloader retaining the upstream marker behavior can first be tested with the existing marker still present, then with a separately guarded marker-free app. These are distinct installation stages, not one combined unreviewed transfer.

## Local physical keys and current prototype

Vendor-published [hardware interface](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting) identifies shared SDA P0.11, SCL P1.09, and PCA9555 addresses 0x20/0x22/0x24 on both halves. Keys are expander inputs, not directly wired nRF GPIOs. Existing `crates/nocfree-input/src/lib.rs` reads active-low bits in expander port order; `firmware/src/keymap.rs` maps those bits into the ANSI layout. Typing was owner-tested, but these precise startup chord bit combinations have not been independently sampled on hardware.

| Role | Chord | Physical bits | Minimum devices |
|---|---|---|---|
| Left ANSI | Fn + Escape | Fn bit 40; Escape bit 0 | 0x24/P1 bit 0; 0x20/P0 bit 0 |
| Right ANSI | Fn + Backspace | Fn bit 42; Backspace bit 14 | 0x24/P1 bit 2; 0x20/P1 bit 6 |

These are physical bits, unaffected by Vial remapping. Do not reuse the ANSI masks for other layouts without their validated electrical map. The current portable prototype takes complete snapshots from all three expanders, matching the established scanner contract; it does not use the minimum-devices optimization in the table.

The [PCA9555 datasheet](https://www.ti.com/lit/ds/symlink/pca9555.pdf) defines input, polarity and direction registers. Factory firmware uses inverted input polarity; RMK explicitly clears inversion. Therefore do not assume polarity survives a warm MCU reset. The current prototype configures all three expanders as inputs (registers 6/7 = FF/FF), clear inversion (4/5 = 00/00), then read input registers 0/1 and interpret low as pressed. Do not touch output registers. This accommodates powered expanders retaining factory state.

The current [portable prototype](../../experiments/held-key-recovery/README.md) requires USB VBUS and a chord sampled continuously for at least 60 ms. It initializes all three expanders and requires each snapshot to be complete. Initial missing keys or a subsequent sampled release immediately continues ordinary startup. NACK, timeout or malformed state returns `RECOVERY_IO_ERROR`: this is **not a recovery request**. The integration must continue the existing bootloader's application-validity and explicit software-request logic. An I²C error alone must not turn ordinary startup into the accidental recovery behavior we are removing. Automatic error-to-DFU was an earlier research candidate and is rejected for this UX.

Its nominal deadline is 200 ms, checked between callbacks, rather than a hard wall-clock guarantee. Every transfer callback must honor a 4 ms timeout; waits must honor their requested 10 ms. A transfer or wait started before the deadline may cross it. The 21-sample cap independently bounds a frozen-clock case: at most 69 bus transfers and 21 waits, or 486 ms under those contracts, plus other bounded clock/USB callback overhead. The hardware adapter must provide the actual timeout, clock, VBUS sensing and cleanup; portable C cannot stop a callback that never returns. These limits are software policy and adapter obligations, not device measurements.

A two-expander, 50 ms probe is only a potential later optimization. It is not implemented, its stability policy has not been selected, and it must not be advertised as the current prototype.

The eventual adapter must ensure every exit stops/disables the selected TWI instance, clears its events/errors and interrupt sources, releases SDA/SCL as inputs and returns ownership to board teardown/application initialization. The hardware adapter must treat stuck STOP itself as bounded: disable on its transaction deadline instead of waiting forever. The caller must invoke bounded cleanup on every exit; the portable prototype has no hardware cleanup callback and does not itself configure nRF hardware registers. Do not drive battery-enable, charge-status, external radio or backlight pins. Reuse an SDK peripheral already compatible with the bootloader build, not the application's Embassy executor. The hook runs only at startup: no steady-state typing or radio latency cost is introduced by design; startup timing still needs measurement.

## Flash budget and installation compatibility

Upstream [`nrf52833.ld`](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/linker/nrf52833.ld) places code at 0x74000..0x7d800 (38 KiB), CF2 configuration at 0x7d800..0x7e000 (2 KiB), MBR parameters at 0x7e000, and settings at 0x7f000. An extension must fit the existing code region; no bootloader relocation or guessed UICR change. Measure actual code headroom from a reproducible board build before promising fit. The vendor's reported revision and exposed user readback do not prove its complete private board configuration or binary equivalence.

There is an established upstream self-update mechanism, rather than a need to invent direct bootloader writes. [`uf2cfg.h`](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/uf2cfg.h) defines a separate bootloader family 0xd663823c. [`ghostfat.c`](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/ghostfat.c#L411-L503) stages the new image and checks fixed addresses and CF2 identity before MBR activation. UICR records in that package are verification metadata, not permission to write UICR. This is separate from application family 0x621e937a and must remain rejected by the existing application guard.

With matching upstream constants, staging occupies up to 0x63000..0x6d000: user end 0x6d000 minus 0xa000 maximum bootloader size. That range overlaps our settings at 0x65000..0x6d000 and may overlap a large central application. This arithmetic follows [`dfu_types.h`](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/dfu_types.h) and the staging formula; it is not permission or evidence of the vendor update path. A reviewed installation plan must explicitly save and restore affected app/settings and reconcile reboot state. Do not claim bootloader self-update preserves bonds.

Before an actual bootloader trial, establish the exact vendor board configuration, CF2 identity, UICR addresses, upper bootloader bytes and applicable MBR behavior for each target. Existing `CURRENT.UF2` does not contain MBR, bootloader, UICR or factory filesystem; it cannot undo a bad replacement. Keep a verified original bootloader restoration source and independent SWD recovery capability for the first trial. An upstream power-loss-resumable copy mechanism is not proof that a candidate bootloader starts, reads the correct pins or enumerates on this board. No flash, bootloader replacement, erase, unlock or UICR operation is authorized by this research document.

## Power-on procedure and receiver boundary

A pre-application hook needs the CPU to reset. Plugging USB into a battery-powered half already running or hung does not necessarily reset it. Right OFF with USB connected was observed to leave its indicator on; the vendor pin table exposes no right switch-sense GPIO. The tested right development sequence removes USB, switches OFF, then starts from battery; it does not establish reset while USB remains attached. The left selector's battery/WIRED behavior has tested startup observations, but still needs one finalized documented power-on sequence with the held chord.

Accordingly “hold these keys and reconnect USB” is not yet an accurate universal instruction. Acceptance must demonstrate actual reset with the keyboard closed, after simulated hangs as well as ordinary operation, for each half. A watchdog may help eventual reset but cannot be treated as an established power-button substitute, especially for firmware that keeps feeding it while key processing is broken.

The receiver has no keys. Do not compile a half's I²C probe or marker-first behavior into it. Preserve its working runtime update mechanism until a separately verified physical/reset or independent bootloader entry method exists.

## Ready-to-implement acceptance plan

1. Host hook harness with fake register/I²C adapter: exact local chord, one key, bouncing chord, remapped keymap irrelevance, NACK at each transaction, stuck SCL/STOP, deadline, cleanup, skip-magic override and no-radio entry. Assert no flash/MBR/UICR writes occur in the entry hook.
2. Reproducible left/right bootloader cross-builds with map/size and constrained package manifests. Preserve fixed upper layout and board config; application-only tools must reject bootloader packages. No hardware claims from these checks.
3. Trial on a recoverable bench board with original bootloader backup and SWD first. Demonstrate ordinary USB startup, each local chord, warm reset retaining expander inversion, application hang, and failed input bus. A failed bus must not request automatic DFU; existing invalid-image or explicit-request logic remains authoritative. Verify real reset path and USB drive on each half independently.
4. Owner-board installation only after device-specific evidence and separate reviewed approval. Keep app marker until new bootloader entry itself is demonstrated, then remove marker in a second guarded application update and verify normal USB startup plus both crash-recovery gestures. Preserve current radio/key behavior while measuring startup delay.

The next software milestone is the bounded hook and its harness, not another unrelated RMK feature. The installation milestone remains a board-specific bootloader recovery trial, not an ordinary RMK update.
