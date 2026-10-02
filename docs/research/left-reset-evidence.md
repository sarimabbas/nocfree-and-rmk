# Left reset evidence

Read-only source review, 2026-10-01. This review did not operate either device. The left remains a separate recovery gate from the tested right. Record new owner/root observations below before treating a proposed startup sequence as verified.

## What the published sources establish

The [manufacturer's community porting guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md) describes left switch position inputs P0.15 (BLE) and P0.17 (2.4 GHz). It does not document battery isolation, a reset output, or an OFF position. Neither GPIO reading nor a position label establishes that the switch removes controller power.

The installed bootloader previously reported base commit `0147d71`. At that [base's USB callbacks](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/usb.c), USB removal is forwarded to TinyUSB and unmount changes LED state. Neither callback requests a chip reset or exits DFU. Its [startup timer](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/bootloader.c) returns when USB is mounted; it does not reschedule. Once the three-second startup interval has elapsed with USB mounted, later unplugging is not an upstream app-start command. Vendor modifications remain possible; the version string is not a bootloader binary audit.

The [marker branch](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c) operates before the application and does not require a reset-pin reset. It nevertheless requires some actual path through bootloader startup. Factory Fn+5 and factory 1200-baud update entry depend on the running factory application. USB disconnect/reconnect while the normal application is active does not by itself establish a controller reset.

## Minimal non-writing experiment and its limits

1. Identify the connected factory left; enter its MSC bootloader using factory Fn+5, and save/validate its own current image and bootloader identity.
2. Leave MSC enumerated for more than three seconds without writing an update or opening its serial port.
3. With the switch remaining in middle WIRED, unplug USB for at least five seconds and reconnect. Observe bootloader versus normal application descriptors. Avoid serial opens, DTR/RTS toggles, DFU packets, and mode changes during this comparison.

If normal factory USB returns, the tested cable sequence supplies an operational escape from bootloader mode independent of the factory application's Fn/CDC handler. Given the inspected base, reset/power cycling is a plausible explanation. This observation does **not** identify RESETREAS, prove controller power reached zero, or exclude vendor bootloader VBUS-exit logic. If MSC returns, the proposed cable-only reset route is unproved. No bootloader STOP/activation opcode is a harmless restart substitute.

Even a successful factory result does not finish the recovery-marker startup question. If WIRED disconnects battery, plugging USB into a marker image could enter MSC on every cold plug. A usable development image also needs a demonstrated application-start sequence, such as battery-first startup in another position followed by USB connection and WIRED selection without an intervening reset. That sequence is currently an assumption, not a recommendation to flash.

## Application trial boundary

A left USB input diagnostic can retain resident S140, MBR, factory bootloader, and protected RAM while using the existing marker; full central flash-space migration is unnecessary for this stage. Before its first write, require a guarded left-role image, fresh device-specific backup and identity, explicit trial authorization, and evidence that the owner can reach recovery independently of candidate application code. The right's successful trial is useful precedent but cannot replace left-device evidence. Preserve the distinction between testing the marker after installation and proving the reset path beforehand.

## New device observations

Root observed the following on 2026-10-01. No firmware was written to the left:

- In middle WIRED, owner-operated factory Fn+5 exposed the left MSC bootloader, VID/PID `239a:0029`. INFO reported `0.9.2-39-g0147d71`, built December 26, 2025, with S140 7.3.0. A fresh readable `CURRENT.UF2` backup matched the earlier left backup: SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`. This container covers `0x1000..0x6d000`; it is not a complete MBR/bootloader/UICR backup.
- After directly observing MSC for more than three seconds, the owner kept middle WIRED, unplugged only the left for five seconds, and reconnected it. Factory `NocFree & ANSI`, VID/PID `2886:8029`, returned instead of MSC. No serial port was opened during this comparison. This establishes the operational escape described above, with its stated limits.
- In a later combined owner instruction (Fn+5, then move to Bluetooth with USB still attached), the subsequent inventory showed factory USB. Root did not observe MSC between those two actions. It therefore does **not** establish that changing the switch caused a reset. The controlled comparison below isolates that action.
- In the controlled comparison, root directly confirmed MSC in middle WIRED before the owner moved only the switch to Bluetooth with USB attached. MSC remained enumerated afterward. This switch change did not visibly exit the bootloader; it does not support the earlier apparent switch-reset interpretation.
- With MSC still confirmed and Bluetooth selected, the owner unplugged the left USB for five seconds and reconnected it. MSC returned, unlike the earlier middle-WIRED cable sequence. This is consistent with battery power retaining bootloader state in Bluetooth, but is not a rail-voltage measurement or proof that no reset occurred.
- Starting from that MSC state, the owner selected WIRED, disconnected USB for five seconds, selected Bluetooth while still disconnected, waited another five seconds, then connected USB. Factory `2886:8029` returned. Moving back to middle WIRED with USB attached left factory USB enumerated. This demonstrates a usable factory battery-first sequence; it does not test the recovery marker or establish that the last switch change caused no transient reset.

The right input diagnostic remained enumerated throughout. Private backups and observation records are in ignored `.evidence/left-bringup/`. The observed WIRED escape and Bluetooth startup support proposing a controlled marker-bearing trial, but neither that left application's startup nor its independent recovery has been tested. The owner subsequently explicitly approved the first trial; initial diagnostic installation, greeting, software update entry and exact readback passed. Physical USB-first marker recovery and diagnostic battery-first startup subsequently passed. Factory restoration and diagnostic reinstallation subsequently passed exact readback; final battery-first startup and owner-assisted basic left typing subsequently passed; the full acceptance sweep remains pending. See [left trial](../left-input-probe.md).
