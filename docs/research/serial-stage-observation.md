# Distinguishable startup stages through the existing bootloader

Proposal, 2026-10-01. No candidate was installed and no device commands were sent for this research. The runtime/HAL trial's MSC observations remain consistent with stage execution, rather than independent proof. This proposal changes the observation channel; it does not replace the bootloader or establish a production update route.

## Smallest useful next pair

Prepare two exact, separately guarded lower-layout images:

1. **HAL serial control:** retain the existing startup and `embassy_nrf::init` configuration, then write `GPREGRET=0x4e`, execute DSB and system-reset immediately after HAL initialization.
2. **USB construction stage:** retain that startup, construct the existing USB driver, CDC builder/class and `builder.build()`, then issue the same serial-only request before polling `device.run()`.

Use the existing recovery marker, RAM floor, S140-magic overwrite, linked-vector checks and unchanged retained-range comparison. If both images fit the existing `0x1000..0x4000` touched span, preserve it exactly; inspect actual linked artifacts rather than assuming their sizes. Do not normalize CPU masks, alter clocks, start a watchdog, configure a progress LED, or log to flash in this pair.

Inspect disassembly to ensure the construction boundary survives optimization. The nRF driver constructor enables its IRQ, but `Driver::start` only constructs bus/control state; peripheral enable and readiness waits happen later in `Bus::enable`. Descriptors and builder state that are never observed before a diverging reset may be optimized away. A diagnostic-only `core::hint::black_box(&mut device)` after `build()` can keep the built object observable to the compiler; this is not evidence that the USB peripheral enabled. [Embassy nRF USB driver](https://github.com/embassy-rs/embassy/blob/3861d3088da30d40c777dc05d282352e68ec5511/embassy-nrf/src/usb/mod.rs), [Embassy USB builder](https://github.com/embassy-rs/embassy/blob/50c6aac5b9b3b0af18f5a7d61725cc8e4d1f7acb/embassy-usb/src/builder.rs).

A serial-only bootloader identity is distinguishable from ordinary marker-driven MSC recovery. The owner previously observed factory 1200-baud entry as `239a:002a`, with CDC and no mounted drive; this is recorded hardware evidence that the vendor has a serial-only mode. It does not by itself establish how the vendor selects that mode without S140. [Device observations](../device-observations.md).

## Primary-source behavior and its limits

At the installed bootloader's reported upstream base, `0x4e` selects serial-only mode and `0x57` selects UF2 mode. Recognized requests are cleared from GPREGRET before entering DFU. The marker-driven fallback sets DFU independently, while serial-only selection depends on the captured GPREGRET value. Both explicit requests and the marker use a three-second timeout, with USB enumeration cancelling timeout. An unplugged stage-return application can therefore loop through reset/DFU/application until USB arrives. [main.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c#L205-L292), [timeout implementation](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/bootloader.c).

The descriptor code selects two CDC interfaces and changes PID for serial-only mode; ordinary UF2 uses CDC plus MSC, three interfaces. Record the actual USB mode, not merely absence of a Finder volume: delayed mounting cannot turn a three-interface MSC device into serial-only evidence. Do not open the serial port or send DFU packets during observation. [usb_desc.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/usb_desc.c#L78-L124).

Source predicts that a genuinely fresh boot with the request already consumed selects ordinary marker recovery, rather than inheriting serial-only mode. It does not prove the switch/cable sequence actually resets this vendor board or clears retained RAM. Vendor modifications are unavailable. Therefore require the candidate's own independent WIRED recovery to enumerate MSC and provide exact readback; do not promise that cold recovery must work from source alone.

## Bounded hardware feedback loop

After separate approval for exact reviewed artifacts and the original restore:

1. Verify the normal left connection, factory bootloader and exact original readable image. Install only the HAL serial control.
2. Perform independent WIRED recovery; require the ordinary MSC identity and exact candidate/readback plus retained-range checks. A CDC-only recovery here is not an accepted substitute for MSC restoration capability.
3. Confirm actual USB disappearance before the owner-directed battery-first startup. Observe the same connection without serial commands. Record PID, interface classes/count and appearance timeline privately.
4. Gate the second image on a distinct CDC-only result from the control. If the control is silent, remains MSC, or is ambiguous, use the proven physical route, restore factory and stop this pair. Do not call this a HAL diagnosis.
5. Recover the control physically to MSC, verify exact bytes immediately before copying the USB construction stage, then repeat its independent recovery/readback and battery-first observation.
6. Restore the complete saved factory container through MSC, verify the exact exposed readback and final normal factory enumeration. Leave the right and receiver unchanged.

CDC-only at the HAL boundary followed by silence/MSC at the USB construction boundary narrows investigation to added construction code or its altered layout. CDC-only for both supports execution through construction; it does not validate USB polling, interrupts, VBUS wakeup or application enumeration. A successful descriptor observation is stronger discrimination than earlier identical MSC returns, but still depends on vendor behavior and the controlled launch.

For a claim of causal attribution that excludes a vendor battery-start branch choosing CDC on its own, add a separately reviewed, matched **negative control** that follows the same HAL path and stops before writing GPREGRET/reset. It should never intentionally request serial DFU. Unexpected CDC-only from this control invalidates the proposed discriminator. The negative control adds an intentionally silent image and another physical recovery/restore operation; it is optional for preliminary calibration, required before treating the mode switch as independent proof of the exact hook. No negative-control image is implied approved by preparing the positive pair.

## Other observation channels

CURRENT.UF2 is generated from the configured readable flash range. It does not expose GPREGRET2, SRAM, CPU masks or hidden bank settings; INFO lists bootloader/board/SoftDevice metadata, not startup breadcrumbs. [GhostFAT readback](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/usb/uf2/ghostfat.c#L259-L357).

The inspected serial DFU transport dispatches update START/INIT/DATA/STOP packets, not arbitrary register/RAM reads. Its internal bootloader status is not a host telemetry API. Do not probe undocumented commands as though they were read-only. [Serial transport](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/dfu_transport_serial.c).

A GPREGRET2 tag or reserved SRAM record would need a later working application reader and vendor preservation evidence. Restoring another application can overwrite ordinary RAM, and power cycling loses retained evidence. These channels add complexity without an available stock host reader. The serial-only bootloader mode is the smallest existing externally visible discriminator to calibrate first.
