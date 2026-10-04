# Keyboard controls

## Current RMK firmware

Choose the connection with the LEFT switch:

| Position | Typing connection | USB cable attached |
| --- | --- | --- |
| Top | RMK dongle | USB supplies power; typing stays on the dongle |
| Middle | USB | Typing over USB; unplugged in this position, left is off |
| Bottom | Bluetooth | USB supplies power; typing stays on Bluetooth |

RIGHT sends keys to LEFT wirelessly in every mode. Its USB connection supplies
power and Companion maintenance; it does not become an independent USB keyboard.
RIGHT OFF controls its battery supply. USB can still power it, so a lit indicator
with USB attached does not prove the battery switch is faulty.

Tap Fn+1 through Fn+5 to select a saved Bluetooth profile. Hold the same chord
for five seconds without pressing another key to replace that profile's pairing,
then choose **NocFree RMK** in the computer's Bluetooth settings. The intended v1
limit is three profiles; current development firmware still provides five.
Fn+0 currently clears the selected Bluetooth bond. It is not an RMK recovery key.

Mac F1/F2 control display brightness, F3 Mission Control, F4 Spotlight, F5/F6
keyboard backlight, F7–F9 media, and F10–F12 audio. Fn exposes ordinary function
keys. Hold F5/F6 to repeat backlight adjustments; brightness is saved and shared
between halves.

## Recovery and updates

Open **Start Recovery Mode** in Companion and choose LEFT, RIGHT, or dongle.
Follow its steps for the detected firmware. Normal RMK runtime recovery works
through the connected component's USB maintenance interface. Recovery entry
alone does not erase settings or install firmware.

Current RMK has no Fn+Escape bootloader shortcut or startup key chord. Fn+U and
Fn+B no longer select transports; the physical left switch does that. Do not use
old development instructions that intentionally entered recovery on USB startup.
The factory bootloader is retained. The watchdog protects application hangs;
normal runtime recovery and deliberate-hang recovery are separate validation
results, not a guarantee against every possible damaged application.

Use Companion's available guided journeys to save backups and open recovery.
The public factory-return journey is still disabled pending completion and
acceptance; controlled developer restore trials do not make it available to
newcomers. Official firmware is not bundled: the completed flow will use the
appropriate user-supplied files or a verified local backup. Restart, settings
reset, and factory restoration are different actions.

## Sleep

USB power keeps that local half awake regardless of mode. On battery, LEFT's
current idle candidate sleeps after 30 minutes; RIGHT follows LEFT except while
RIGHT has its own USB power. The 30-minute first-key wake test is still pending.
Backlights follow sleep and restore the saved brightness on wake. Battery
endurance and instrumented wake latency have not been measured.

## Status indicator trial

LEFT indicates the keyboard's host mode: Bluetooth advertising blinks blue,
connected Bluetooth is steady blue, and connected Dongle mode on battery mixes
blue and red. USB power releases the shared red channel for the charger, so the
observed color can differ while charging. Wired mode turns off the blue output.
RIGHT's indicator remains unchanged; it connects to LEFT rather than selecting
a host mode. Physical color and sleep behavior are still under acceptance.

## Factory firmware

Factory Bluetooth uses tap Fn+1/2/3 to select and long-hold to pair. Factory
recovery uses LEFT Fn+5 with USB/wired, RIGHT Fn+0 with its USB/ON, and the
dongle DFU action assigned by NocFree Link. These depend on running factory
firmware and are distinct from RMK's Companion recovery.

Use the [factory manual](https://www.nocfree.com/pages/nocfree-and-manual) and
[firmware guide](https://www.nocfree.com/blogs/news/nocfree-firmware-update-guide)
for complete vendor procedures. Do not assume a factory shortcut recovers a
crashed replacement application.
