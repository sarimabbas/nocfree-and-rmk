# Observed left-half behavior

Observed during the initial porting session on 2026-09-30 (America/Los_Angeles):

- The owner connected the left half in USB mode via an Anker Thunderbolt dock.
- Normal USB product: `NocFree & ANSI`; VID/PID `2886:8029`. This confirms ANSI layout for this connected device.
- PySerial verified that device's CDC port identity before opening and closing it at 1200 baud.
- The device re-enumerated as `NocFree &`, VID/PID `239a:002a`, exposing CDC serial. No UF2 mass-storage volume or corresponding external disk appeared.
- The owner unplugged and reconnected it; the same normal ANSI application identity returned. No DFU image, erase, init packet or activation packet was sent.

This establishes factory software bootloader entry and no-write return for this left half. It does **not** establish bootloader version, maximum application partition, current firmware readback, rollback after overwriting the application, or entry when the application cannot boot. Right-half and receiver identities have not been observed.

Do not publish local serial numbers or Bluetooth addresses. Serial port names change between connections and are deliberately omitted here. The read-only identity procedure should match the full role/product, not a stale path or shared VID/PID alone.
