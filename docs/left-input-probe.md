# Left USB input diagnostic

Status: ongoing owner-approved left-only trial on 2026-10-01. Diagnostic identity, greeting, update entry, exact readback, physical USB-first recovery and battery-first startup passed. Factory restoration and diagnostic reinstallation both passed exact readback. Final battery-first startup passed; the owner reported that the requested left typing, Shift, Space and Tab checks all worked. The left is running the diagnostic; the complete key sweep and disconnect/wake acceptance remain pending. See [left reset evidence](research/left-reset-evidence.md) for the separate recovery gate.

The existing input diagnostic now selects exactly one half. The left uses the same PCA9555 scanner seam, explicit external crystal startup, RMK debounce, shared keymap and USB transport as the tested right. Its 37 positions are the first slice of the shared ANSI map; the right remains the last 47. No new debounce, HID processing or split protocol was introduced.

From `firmware/`, build without writing to a device:

```sh
cargo build --locked --release --bin input-probe --target thumbv7em-none-eabihf \
  --no-default-features --features left,input-probe,usb-recovery-first
```

The diagnostic requires the factory-preserving application layout and existing recovery marker. Compilation rejects missing/multiple roles, the receiver role, reclaimed SoftDevice memory and omission of the marker feature. Packaging must separately pass the UF2 and serial-package guards; a cross-build does not establish safe device recovery.

Expected development identity is `4c4b:4652`, product `NocFree Input Probe Left`. Expected CDC greeting is `NocFree input probe 0.1.0 left factory\r\n`. These are development identifiers, not registered production identifiers. The left feature also enables RMK's vendor and reset-only DFU-detach interfaces; no application DFU writer or radio task is started. The public USB builder supplies room for the diagnostic CDC interface.

The diagnostic does not initialize battery ADC, backlight, mode selection, BLE or storage. Fn host-profile actions are transparent. The shared left Fn+Escape action requests the existing bootloader on Escape release; unlike factory Fn+5, it has no five-second hold. CDC update entry uses the same 1200-baud/DTR request as the right. Both depend on working application code and cannot replace the independent recovery gate.

Expected base positions, scanner columns `0..36`:

```text
Escape F1 F2 F3 F4 F5 F6
Grave 1 2 3 4 5 6
Tab Q W E R T
CapsLock A S D F G
LeftShift Z X C V B
Fn LeftCtrl LeftAlt LeftGUI Space
```

The repeatable harness now cross-builds both diagnostics alongside all production roles. The recovery-first migration configuration passed 39 host tests and formatting; all three production roles and both factory-preserving input diagnostics cross-built. Invalid diagnostic feature selections were separately checked for compile-time rejection. These are software results, not left hardware validation. The default factory-preserving full left central remains oversized.

Before a first trial, record exact candidate hashes, flash/erase bounds and guarded package results; verify this left's backup and bootloader identity; establish an application-independent recovery route and a plausible application-start sequence; then obtain explicit authorization for the concrete trial. Retain the factory image for rollback. Initial hardware checks must cover identity/readback, startup, independent recovery, keys and releases before proceeding to the disconnect, held-modifier and wake tests in [acceptance](acceptance.md).

## Guarded first-trial candidate

The reviewed package from source commit `728a568` is saved locally in ignored `dist/input-probe-left/`. Its BIN is 60,692 bytes at `0x27000..0x35d14`, UF2 pads to `0x35e00`, and serial DFU erases only `0x27000..0x36000`. Initial SP is `0x20020000`, reset vector `0x27205`, and recovery marker `0x87eeb07c` is at `0x27200`. Independent review reconstructed the BIN from ELF LOAD segments exactly and reran both guards. The owner subsequently approved installation; initial readback results are below.

- BIN SHA-256: `496db7c4377c0f5cf77d3875806fd178c78174bb78aded8fc9c893eba192a436`.
- UF2 SHA-256: `d3b45ab451568145ee74f98d755e80c42d836f5244a88a5de0933e086a81182a`.
- Serial ZIP SHA-256: `fdede75e5a33ca4839cb108e72c5870065cd069244bc9b8a6f17b19f881ebc38`.

A separate local factory restore reproduces this left's entire application slot `0x27000..0x65000` byte-for-byte from the freshly matched backup, retaining all readable ranges outside that slot. Its standard-family UF2 passes address/vector/family inspection without requiring the diagnostic marker: factory firmware intentionally lacks that marker. Its legacy serial package is separately checked against the exact factory BIN, manifest/init metadata and CRC; the diagnostic serial guard intentionally requires the marker and is not claimed to pass for factory restoration. Restore BIN SHA-256 is `7087c8c8612f12a3dc75c6db88801a781b4414492226ac99e70a684bc23dd54e`; restore ZIP SHA-256 is `4061de7230bdfafb9b27a167cd44e9ed794ea8e9081d457e952a9ce6ad6cd36d`. These backups/packages remain outside Git.

The owner-assisted factory observations establish a WIRED cable escape from MSC and a usable Bluetooth battery-first startup sequence. They support proposing this first trial; they do not yet prove marker recovery on this left. Vendor bootloader differences remain a risk, and failure could require opening the enclosure. A controlled trial should install the guarded diagnostic, verify exact readback and unchanged protected ranges, test application-independent USB recovery and battery-first startup, restore and verify factory firmware, then reinstall only if every required recovery check passes. The owner explicitly approved this controlled left-only diagnostic trial. No SoftDevice migration is authorized.

## Initial authorized installation

The factory serial update request did not expose MSC; the owner entered the existing bootloader with factory Fn+5. Immediately before transfer, the full readable factory image matched SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`, its complete UF2 structure passed, and bootloader INFO content matched. The older saved text had LF line endings and the mounted file CRLF; normalized lines were identical.

Application-only serial DFU completed. `NocFree Input Probe Left`, VID/PID `4c4b:4652`, enumerated and returned its expected greeting. The deliberate 115200/DTR-high → 1200/DTR-low sequence returned to the existing MSC bootloader. Readback matched every candidate BIN byte, the expected FF tail through `0x36000`, unchanged resident S140 below `0x27000`, and unchanged readable flash above `0x36000`. Full readable UF2 SHA-256 is `2ce6efa1e82adff1be74e359163f61ac21a6d67037e57813b8d4665baed7a023`. Bootloader INFO content remained identical; it is not a bootloader binary readback. The right diagnostic stayed enumerated.

Private logs and evidence are in ignored `.evidence/left-bringup/trial/`. At this checkpoint, physical marker recovery, diagnostic battery-first startup, factory restoration, input mapping and disconnect/wake acceptance remain untested. No successful transfer or readback is counted as a key or latency test.

## Physical recovery, startup and restore progress

With the diagnostic marker installed, the owner selected middle WIRED, unplugged only the left USB for five seconds, then reattached it without pressing keys. The existing MSC bootloader appeared; full readable image SHA-256 still matched `2ce6efa1e82adff1be74e359163f61ac21a6d67037e57813b8d4665baed7a023`, and bootloader INFO content matched. Together with the earlier factory comparison, this is observed USB-first recovery evidence for this specific left. It is not a rail-voltage measurement, an interrupted-update test or evidence for migration/receiver recovery.

The owner then selected WIRED, disconnected USB for five seconds, selected Bluetooth while still disconnected, waited five seconds, and reattached USB. The diagnostic enumerated, returned the exact left greeting and exposed keyboard HID usage page 1/usage 6. This verifies one battery-first diagnostic startup on macOS, not a latency or stress test.

The diagnostic's deliberate CDC update sequence returned to MSC again. Before restoration, the image hash still matched the diagnostic; the exact local factory restore ZIP/BIN/UF2, metadata, CRC and equality to the saved factory slot were rechecked. Approved serial restoration of the application slot `0x27000..0x65000` completed, and factory `NocFree & ANSI`, VID/PID `2886:8029`, enumerated. Factory readback and diagnostic reinstallation are pending owner-operated Fn+5 entry. The right diagnostic remained enumerated throughout; the dongle was not touched.

## Verified factory restoration and reinstallation

The owner entered the restored factory bootloader with Fn+5 in middle WIRED. Its entire readable `CURRENT.UF2` was byte-identical to the original pre-trial backup, SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`; INFO content also matched. This is an observed application rollback, not merely a prepared restore package.

With the required recovery/startup/restore checks passing, the same approved diagnostic ZIP was revalidated and reinstalled. Its expected greeting passed, the software update route returned to MSC, and the complete readable image matched the first diagnostic installation byte-for-byte, SHA-256 `2ce6efa1e82adff1be74e359163f61ac21a6d67037e57813b8d4665baed7a023`. Final battery-first startup remains pending owner participation. No S140/bootloader migration, dongle update, left key sweep or transport acceptance is implied by this trial.

## Final startup and owner-assisted typing

After reinstallation/readback, the owner repeated the battery-first sequence. The left returned its exact expected greeting again, with the right diagnostic still enumerated. The owner then reported “everything works perfectly” for the requested `123456 qwert asdfg zxcvb`, left-Shift uppercase text, Space and Tab check. This is an owner-assisted functional pass without raw HID capture; it is not a complete key/modifier sweep or latency/no-loss validation. The owner subsequently confirmed Ctrl+Fn+F1/F2 changes the external display brightness; Fn+F1/F2 alone did not on this setup.

## External display brightness and next Mission Control mapping

The owner reported no brightness change from Fn+F1/F2 alone on their Apple external display, while another keyboard worked. Holding left Ctrl+Fn and tapping F1/F2 then worked. This matches [Apple's external-display Control+brightness shortcut](https://support.apple.com/en-us/102650); the existing standard consumer brightness mapping remains unchanged. No raw HID capture was required after the physical shortcut succeeded.

The next source candidate maps left Fn+F3 to RMK's native `MissionControl` keycode, which the pinned framework translates to consumer usage `0x029f` (Desktop Show All Windows). Plain F3 remains F3. This mapping has not yet been installed or hardware-tested; the running left still has transparent Fn+F3. Do not count framework naming as macOS validation. The documented initial-trial hashes above remain the installed checkpoint.
