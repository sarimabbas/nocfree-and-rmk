# Companion recovery rollout

The right half has owner-observed Companion recovery without a key chord and normal battery-start split typing. The left startup-stage candidate is installed and its exact readback is verified, but Companion startup recovery has not passed. The receiver still needs that device acceptance. Preserve the factory bootloaders; the application startup USB stage runs before RMK starts.

## Confirmed and pending

| Part | Last observed working baseline | Prepared change | Remaining device evidence |
| --- | --- | --- | --- |
| Left | No-chord startup candidate; exact application, padding and untouched readable regions verified through runtime DFU recovery | Temporary startup exit diagnostics | Companion startup recovery, normal USB/BLE/split/dongle operation |
| Right | No-chord startup stage; exact readback, automatic Companion recovery and battery-start cross-half modifiers observed | None needed for this rollout | Broader release, simultaneous-input and latency acceptance remain separate |
| Receiver | Working RMK receiver; exact readback and unplug/replug typing isolation observed | No-chord USB startup stage, then receiver RMK | Fresh baseline, exact installed readback, Companion recovery, pairing and isolated dongle typing |

The receiver's existing recovery depends on its running application. Its previous owner-specific risk exception is not evidence of independent recovery and is not a public installation guarantee.

## Left first

The installed left candidate removes Fn+Escape. Do not give that shortcut as its recovery procedure. Two captured WIRED USB reconnects showed the normal RMK interface without an observed startup-stage interface. This does not establish which startup branch ran or prove a cold MCU reset.

A separately reviewed, exact-left standard runtime DFU request opened the factory recovery drive after a fresh USB disappearance and reappearance. Exact candidate readback, page padding and all untouched readable bytes then matched the saved baseline. This application-dependent route is diagnostic access, not proof that recovery survives an RMK crash.

The next diagnostic will retain the existing startup timing and record whether USB-power detection, clock acquisition or USB enumeration prevented entry. It will expose the result through the ordinary USB manufacturer string, without storage writes or a new host protocol. Leave the right and receiver unchanged.

Before transfer, bind a fresh CURRENT.UF2 and INFO_UF2.TXT to that left's previously observed identity and USB location. Require the application prefix to match the last installed, verified left shim, and save the fresh untouched gap and settings. Construct a rollback covering every page the new candidate will touch from this fresh baseline, rather than only the shorter old binary.

The prepared left candidate covers application pages from `0x1000` through `0x61000` exclusive. Its UF2/BIN pair passes the application-shim guard. The factory bootloader, MBR, UICR and settings are outside that write. Recheck the exact artifact and source bindings in a left-specific operator; the previous right operator is not reusable. Save the durable one-shot attempt before copying, and never automatically retry a transfer.

After transfer, prove Companion recovery using a real restart: WIRED, unplug USB for five seconds, reconnect in WIRED with Companion waiting. No keys should be required. Compare the exact binary and page padding, and compare every untouched readable byte to the fresh baseline. Normal USB startup and owner typing must be checked separately. Then check BLE reconnect, right split input/shared modifiers and the existing dongle route.

The prepared left's RMK storage schema matches the established left schema. The correction retains logging because feature selection changes RMK's schema. Leaving settings addresses untouched alone does not establish schema compatibility.

## Receiver second

The working receiver's observed recovery route is standard runtime DFU_DETACH during RMK's first 30 seconds after a genuine unplug/replug. It requires a fresh, exact receiver identity/interface binding and its own one-shot command operator. It is not a serial route, a dock power action or a generic USB reset. Keep the left/right working while preparing this route.

The prepared receiver candidate covers application pages from `0x27000` through `0x65000` exclusive, preserving the resident S140 below the application and storage above it. Its ordinary application UF2 guard passes. Before transfer save a fresh receiver readback, verify the known working application, and derive a complete rollback for the touched pages. Do not substitute a downloaded factory image for this particular receiver's backup.

The receiver candidate changes RMK revision and therefore changes its storage schema. Expect pairing/settings reset on startup; separately verify pairing and keyboard input. Application readback cannot prove radio correctness. A new device-specific operator and independent review are needed, with the existing absence of independent receiver recovery made explicit.

## Startup stage limits

The stage runs after HAL initialization and before RMK/MPSL. It presents a role-specific DFU endpoint for two seconds only when USB power is present at startup. Companion must already be watching. A cable reconnection may not restart a battery-powered half; guide the proven power-switch procedure instead of treating USB disappearance as proof of reset. The stage requests the existing factory bootloader through the observed retained-register entry mechanism, without replacing it.

This protects against many failures after the stage, but not damage to the stage or a failure before it. Software guards and successful builds are not proof of recovery, lossless typing, charging correctness or measured latency.

## Temporary exit diagnostic

The opt-in `usb-rescue-diagnostic` feature publishes a 27-character manufacturer string: `rescue:E i=03 a=03 c=1f e=0`. It does not change the normal product name, startup gates, two-second window, clock ownership, USB cleanup or RMK feature selection. The trace lives in RAM only.

The reason is `V` for absent VBUS, `C` for clock-acquisition timeout, or `E` after the USB stage returns. `I` is the initialized state. The hexadecimal samples are before HAL (`i`), after HAL (`a`), and after clock acquisition returns (`c`). The last sample includes timeout cleanup if acquisition fails. Sample bits 0/1 are VBUS/USB-output-ready, bit 2 is the outstanding high-frequency clock request, bit 3 is clock-running, and bit 4 is crystal source. `e` latches USB reset/configuration events in bits 0/1. A zero clock sample on a VBUS exit is unsampled, not evidence of a clock failure.

The diagnostic identifies a branch; it does not establish crash-safe recovery. Hardware acceptance must still show the startup stage opening recovery when the later RMK application cannot run.

## Current offline check

At preparation, both corrected candidate guards exactly match the saved results, and the reviewed firmware source hashes still match the current sources. The left schema is compatible; the receiver schema is not. These are read-only artifact checks. No device command, serial open, firmware copy, reset or port action was performed during this preparation.
