# Observed left-half behavior

Observed during the initial porting session on 2026-09-30 (America/Los_Angeles):

- The owner connected the left half in USB mode via an Anker Thunderbolt dock.
- Normal USB product: `NocFree & ANSI`; VID/PID `2886:8029`. This confirms ANSI layout for this connected device.
- PySerial verified that device's CDC port identity before opening and closing it at 1200 baud.
- The device re-enumerated as `NocFree &`, VID/PID `239a:002a`, exposing CDC serial. No UF2 mass-storage volume or corresponding external disk appeared.
- The owner unplugged and reconnected it; the same normal ANSI application identity returned. No DFU image, erase, init packet or activation packet was sent.

This establishes factory software bootloader entry and no-write return for this left half. It does **not** establish bootloader version, maximum application partition, current firmware readback, rollback after overwriting the application, or entry when the application cannot boot. Right-half and receiver identities have not been observed.

Do not publish local serial numbers or Bluetooth addresses. Serial port names change between connections and are deliberately omitted here. The read-only identity procedure should match the full role/product, not a stale path or shared VID/PID alone.

## Additional MSC observation

The owner held factory Fn+5 on the left while connected in USB mode. A NocFree drive appeared with `INFO_UF2.TXT` and `CURRENT.UF2`. These files were read and copied **off** the device; source/destination hashes matched. No file was copied onto the drive.

Metadata: UF2 Bootloader `0.9.2-39-g0147d71`, model and board ID `NocFree &`, build date December 26, 2025, SoftDevice `S140 7.3.0`.

The current readback is 884,736 container bytes: 1,728 contiguous 256-byte payloads spanning `0x1000..0x6d000`. Its board-specific UF2 family is `0x239a0029`; this differs from the standard nRF52833 family in the official application ZIP. The copy is stored privately under `.evidence/factory-left/` and excluded from Git. Its SHA-256 is `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`.

Decoded SoftDevice metadata corroborates firmware ID `0x123`, size/application base `0x27000`, and version 7.3.0. The application vector's initial stack is `0x20020000`. The readback includes the SoftDevice and existing application, including the proposed RMK settings interval; it excludes the MBR, factory filesystem, bootloader, UICR and bootloader settings. It is therefore a backup of the overwrite candidate region, not a complete device backup.

The owner clarified that there is no external reset pinhole. Recovery access would require opening the enclosure. The supplied manufacturer video shows right-half module edge contacts; this has not established physical recovery on the owner's left half. Software MSC entry alone cannot recover an application that cannot execute its keyboard shortcut.
## Right factory readback, 2026-10-01

The owner unplugged the left, connected the right with its external switch ON, and held factory Fn+0. A `NocFree &` drive appeared. Read-only metadata reports the same `0.9.2-39-g0147d71` bootloader, 2025-12-26 build date and S140 7.3.0 as the left.

`CURRENT.UF2` is saved locally in `.evidence/factory-right/`, with source/destination SHA-256 `b74ba6c686b15a74f837ba26eb67efd241a6744c48d64d6fed7eb72974f3f043`. All 1,728 blocks have valid magic, numbering, 256-byte payloads and contiguous addresses `0x1000..0x6d000`, custom family `0x239a0029`. Factory application vectors are SP `0x20020000`, Thumb PC `0x37875`. This is the right's own backup; it must not be replaced with the left's. It excludes the MBR, filesystem, bootloader and UICR. No firmware was written.

After the owner toggled the right switch OFF/ON, a read-only USB inventory showed normal `NocFree nRF52833 Right`, VID/PID `239a:80d8`. The exact sequence was not recorded well enough to call it a repeatable reset test. The owner then explicitly left the right switch OFF with USB attached; the same normal USB device remained enumerated. Thus OFF alone does not remove MCU power while USB supplies it. A prospective cold-start test must disconnect USB as well as turn the battery switch OFF; battery-only startup requires ON with USB absent before attaching the host cable. These are candidate workflows, not tested recovery from a replacement application.
