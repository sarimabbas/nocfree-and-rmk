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

## Check pairing

Open **Check pairing** under Additional utilities. Connect LEFT by USB and turn
RIGHT on; the utility observes their automatic split link. Plug in the dongle
to check it too and select the top Dongle position on LEFT. Healthy saved links
finish automatically without clearing anything. If the dongle needs re-pairing,
Next explicitly pairs those two devices and preserves ordinary Bluetooth hosts.
Keep both USB connections in place while it works. The utility verifies the
selected peers and encrypted link before reporting success. Cancel stops the
check; an accepted pairing change cannot be undone by cancellation.

## Recovery and updates

Open **Enter recovery mode** in Companion and choose LEFT, RIGHT, or dongle.
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
Restore factory uses the same guided recovery, backup, transfer and readback
machines as Install RMK. Official firmware is never bundled. A complete local
factory backup is required for each part, including its S140 system firmware;
an official application-only UF2 can be selected only over a validated complete
backup. Restart, settings reset and factory restoration are different actions.

## Sleep

USB power keeps that local half awake regardless of mode. On battery, LEFT's
current idle candidate sleeps after 30 minutes; RIGHT follows LEFT except while
RIGHT has its own USB power. The 30-minute first-key wake test is still pending.
Backlights follow sleep and restore the saved brightness on wake. Battery
endurance and instrumented wake latency have not been measured.

## Status indicator

LEFT uses the same blue indication for Bluetooth and dongle: blink while
seeking a connection, then steady blue for 30 seconds after connecting and off.
Selecting a different wireless mode starts a fresh indication. Wired and sleep
turn the blue output off. The charging circuit independently controls red;
charging can mix its red light with blue without changing the indication rules.
Firmware does not drive the red channel. RIGHT's indicator remains unchanged;
it connects to LEFT rather than selecting a host mode.

Key backlights remain independent of the status indicator. Off/sleep explicitly
stops PWM; wake and a nonzero level restart it. A disconnected battery-powered
RIGHT darkens its backlights while seeking LEFT; local USB power preserves its
lighting. Stored brightness is retained. A split disconnect cancels any held
brightness repeat.

## Factory firmware

Factory Bluetooth uses tap Fn+1/2/3 to select and long-hold to pair. Factory
recovery uses LEFT Fn+5 with USB/wired, RIGHT Fn+0 with its USB/ON, and the
dongle DFU action assigned by NocFree Link. These depend on running factory
firmware and are distinct from RMK's Companion recovery.

Use the [factory manual](https://www.nocfree.com/pages/nocfree-and-manual) and
[firmware guide](https://www.nocfree.com/blogs/news/nocfree-firmware-update-guide)
for complete vendor procedures. Do not assume a factory shortcut recovers a
crashed replacement application.

## Install RMK in Companion

Choose **Install RMK**, then **Next**. Opening the page does not start recovery
or installation. Companion uses its bundled, hash-pinned local release; it does
not download firmware or embed factory firmware.

The journey handles the dongle first, then right and left. This keeps the factory
left available for the factory dongle's recovery shortcut. Each part shares the
recovery guide, saves a fresh local backup, installs once, and verifies the actual
readback before proceeding. Only the physical steps that cannot be done by the app
need user action. An exact installed image skips copying. Returning to the
journey reconciles pending writes using their original backup and fresh readback;
USB names or version strings alone never count as installed-image proof.

After installation, Companion runs Check pairing for both halves and the dongle.
It automatically repairs the dongle link when needed, then guides wired,
Bluetooth and dongle checks. For each, type `qwert HJKL h` using left letters,
left Shift with right capitals, and right lowercase after releasing Shift.
Next becomes available only with the expected connection and matching text.
A changed or lost connection clears that confirmation. The Bluetooth check uses
fresh macOS connection discovery and owner typing because USB telemetry is
unavailable with every cable and dongle unplugged. These are functional owner
checks, not measured latency or current tests.

## Restore factory in Companion

Open **Restore factory** to inspect the three saved factory files without starting
recovery. Choose or drop a file on the matching part card if needed; **Next**
authorizes the guided restore only when all three files pass validation.
The first recognized complete factory backup is retained privately and never
replaced by later RMK backups. Factory bytes are not distributed with the app.

Restoration handles left, right, then dongle. Restoring left first makes the
factory dongle recovery shortcut available again. Every part receives a fresh
RMK backup before any copy, and its complete S140, application and settings
readback must match before proceeding. Interrupted RMK installation can be
undone with the validated factory target without retrying its pending RMK write.
No bootloader, MBR or UICR writes are part of either journey.

Factory firmware does not expose RMK's pairing/status protocol. The shared mode
check machine instead asks you to confirm the physical setup and type
`qwert HJKL h` over wired, Bluetooth and dongle connections. Fresh USB and macOS
Bluetooth observations gate these checks; unknown switch or split telemetry is
not invented. Stock left and dongle share a USB descriptor, so only the intended
one is plugged in for each check. You can then return to Install RMK using the
same recovery and transfer machines and the preserved factory originals.

Software tests cover both targets, refusal guards, interruptions, role binding,
and stale callbacks. A repeated live factory → RMK → factory roundtrip remains
a separate hardware acceptance gate; software tests do not establish it.

## Already-current firmware

Install RMK and Restore factory stay visible in the sidebar. Their shared setup
state checks the connected parts without starting recovery. When all three parts
report the bundled RMK version, Install RMK shows **Already latest version**.
When the complete connected set is factory firmware, Restore factory shows
**Already on factory firmware**, independent of factory version. Neither status
starts a transfer or offers Next.

Missing or mixed parts continue through the guided checks. Exact current RMK
readback skips copying that part; a correlated factory part is left unchanged by
Restore factory regardless of its version. This factory no-op does not weaken
role validation for any actual firmware write. Firmware release versions must be
bumped together in the firmware and bundled release whenever shipped bytes change;
same-version development builds are distinguished by exact readback, not SemVer.

## Factory restore source

Choose **Use saved backups** for the privately retained originals, or **Supply
UF2 files** to choose/drop one official factory file for left, right and dongle.
The supplied option requires all three validated selections before Next.
Switching source clears supplied selections and reloads the originals; returning
to the page preserves a validated supplied target without silently reverting it.

NocFree's official app-only UF2s use the complete saved factory backup as the
system-firmware/settings donor. An app-only file without that backup is refused.
The original backups are never overwritten by supplied targets. Supported
official files currently include the inspected ANSI 2.4.5 set; unknown future
files must pass an updated role-specific guard before use.
