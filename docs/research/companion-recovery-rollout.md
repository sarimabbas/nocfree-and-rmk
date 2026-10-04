# Companion recovery rollout

The right half has owner-observed Companion recovery without a key chord and normal battery-start split typing. The left and receiver still need that same device acceptance. Preserve the factory bootloaders; the prepared change adds an application startup USB stage before RMK starts.

## Confirmed and pending

| Part | Last observed working baseline | Prepared change | Remaining device evidence |
| --- | --- | --- | --- |
| Left | Application recovery shim; normal wired startup and Fn+Escape recovery observed | No-chord USB startup stage, then existing RMK | Fresh baseline, exact installed readback, Companion recovery, normal USB/BLE/split/dongle operation |
| Right | No-chord startup stage; exact readback, automatic Companion recovery and battery-start cross-half modifiers observed | None needed for this rollout | Broader release, simultaneous-input and latency acceptance remain separate |
| Receiver | Working RMK receiver; exact readback and unplug/replug typing isolation observed | No-chord USB startup stage, then receiver RMK | Fresh baseline, exact installed readback, Companion recovery, pairing and isolated dongle typing |

The receiver's existing recovery depends on its running application. Its previous owner-specific risk exception is not evidence of independent recovery and is not a public installation guarantee.

## Left first

The next physical step is to connect the working left half by USB, hold its Fn key, tap and release Escape, then release Fn. This opens the existing recovery drive. Leave the right and receiver unchanged.

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

## Current offline check

At preparation, both corrected candidate guards exactly match the saved results, and the reviewed firmware source hashes still match the current sources. The left schema is compatible; the receiver schema is not. These are read-only artifact checks. No device command, serial open, firmware copy, reset or port action was performed during this preparation.
