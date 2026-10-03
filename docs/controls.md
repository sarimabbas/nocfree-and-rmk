# Keyboard controls

This guide separates installed development firmware from the intended release.
The proposed controls and held-key startup recovery are not installed or hardware
validated. Do not try the proposed startup gestures on the current firmware.

## Intended release

Choose the connection with the left switch:

| Position | Connection | USB cable attached |
| --- | --- | --- |
| Top | RMK receiver | Charging/maintenance; typing stays on the receiver |
| Middle | USB | Typing over USB; without USB, no typing |
| Bottom | Bluetooth | Charging/maintenance; typing stays on Bluetooth |

The right half sends its keys to the left wirelessly in all three modes. It does
not become a separate USB keyboard. Right OFF must prevent typing, including
while charging, if hardware makes the switch state observable. Full electrical
power isolation cannot be promised from firmware.

In Bluetooth mode, tap Fn+1, Fn+2 or Fn+3 to choose a saved computer. Hold the
same combination for five seconds to replace its pairing, then choose
**NocFree RMK** in that computer's Bluetooth settings. Other slots are retained.

Companion provides connection repair, restart, confirmed settings reset, updates,
and factory restoration. The install/update action is selected from the detected
firmware state; an already current RMK installation offers neither action.
Factory files come from the user's verified backups or individually validated
left/right/receiver files; official firmware is not bundled.

Ordinary Fn+Escape, Fn+0, Fn+U and Fn+B have no maintenance or connection action
in this release design. Updates are deliberate Companion journeys. Restart does
not erase settings; settings reset and restoring factory firmware are separate
confirmed actions.

## Intended emergency recovery

The proposed board-specific bootloader checks physical keys before RMK starts:

| Half | Hold during a genuine startup with USB attached |
| --- | --- |
| Left | Its own Fn + Escape |
| Right | Its own Fn + Backspace |

Hold until the firmware drive appears, then release and open Companion's recovery
journey. Each half must work alone with its peer off and radio unavailable.
Saved keymap changes must not change these physical recovery positions.
Ordinary cable attachment, mode selection, and these chords during normal typing
must not enter recovery. Recovery entry alone never clears settings or writes a
firmware image.

These gestures are proposals. A precise power-on procedure is still required:
USB unplugging is not necessarily an MCU reset when battery power remains, and
the right power switch does not necessarily reset a USB-powered MCU. The gesture
must be tested on each actual hardware revision before publishing final steps.
The receiver has no keys; its independent emergency entry is unresolved and must
not inherit this procedure.

## Installed development firmware

The left mode selector is not yet authoritative. Leave the left in Bluetooth
after battery-first startup; attach USB only once the application has started.
Middle WIRED with USB removed for five seconds, then Bluetooth for ten seconds
before reconnecting USB is the previously tested left startup procedure. The
right's tested battery-first procedure is OFF with USB removed for five seconds,
then ON for ten seconds before optionally attaching USB. Neither is release UX.

Current shortcuts are Fn+1..5 for Bluetooth slots (hold five seconds to replace
that slot's pairing), Fn+U to select the RMK receiver (hold five seconds to
replace receiver pairing), Fn+B to toggle USB/BLE preference, and Fn+0 to clear
the selected host bond. Left Fn+Escape requests the existing bootloader without
a long hold. Right Fn+0 does not locally enter recovery in the full split build.

The halves' recovery-first application marker intentionally makes USB-first
startup enter the existing firmware drive before RMK. This preserves the tested
development recovery route, but conflicts with the intended release startup.
Do not remove the marker until the replacement route is independently verified.
The receiver's independent crash-recovery route remains unresolved.

## Factory firmware

The factory selector chooses dongle/wired/Bluetooth. Tap Fn+1/2/3 to select a
Bluetooth computer; long-hold the same chord to pair. Receiver repair uses left
Fn+4 for five seconds in dongle mode. Factory split repair uses long-hold left
Fn+4 and right Fn+9, following the manufacturer's complete guide.
See the [factory manual](https://www.nocfree.com/pages/nocfree-and-manual) and
[troubleshooting guide](https://www.nocfree.com/pages/nocfree-and-troubleshooting).

Factory update entry is left Fn+5 for five seconds with USB in wired mode, or
right Fn+0 for five seconds with that half connected directly by USB and ON.
The receiver uses the factory Dongle DFU action, typically assigned to Fn+6;
check the assignment in NocFree Link. These depend on running factory firmware
and are not a universal crashed-firmware recovery route. See the
[factory firmware guide](https://www.nocfree.com/blogs/news/nocfree-firmware-update-guide).

## Release acceptance

Before replacing the current development procedures:

1. Establish genuine restart and selector readings on each half with and without
   USB. Document any right-switch hardware limitation.
2. Test ordinary cold USB startup without any recovery gesture on both halves.
3. Test deliberate local recovery with RMK unable to run and the other half off.
4. Test a remapped keymap, single held keys, brief chords, late key presses, and
   mode changes: none may accidentally enter recovery.
5. Test unavailable, failed and stuck I2C reads: bootloader decisions must finish
   within a bounded time; no fabricated pressed-key state or indefinite loop.
6. Verify selector routing never falls back to another host, and releases held
   keys/modifiers on the previous host. Test simultaneous input, disconnect,
   reconnect, saved pairing, and wake latency on USB, Bluetooth and receiver.
7. Verify recovery survives each half's update and that Companion validates the
   role and image before transfer. Establish a separate receiver recovery route.

The host harness and role cross-builds are prerequisites, not substitutes for
these hardware observations. Bootloader installation is a separate reviewed
operation; existing application readbacks do not back up bootloader code or UICR.
