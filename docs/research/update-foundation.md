# An updateable foundation for NocFree

Research 2026-10-01. No device writes. Recommendation: retain the installed Adafruit bootloader and investigate its existing boot-before-application USB recovery convention before replacing bootloaders. Keep previous images on the computer for manual rollback; the current approximately 340 KiB RMK central cannot fit two internal update slots in 512 KiB.

Subsequent root/owner-approved trial: the right-only USB diagnostic now demonstrates the marker's USB-first recovery workflow, application startup, update entry, factory restoration and reinstallation. The research agent did not perform device operations. See [hardware results](../recovery-probe.md). This result applies to the tested right, S140-preserving layout; left, receiver and S140 migration remain separate gates.

## Recovery and rollback are separate

A bootloader can keep USB flashing available when application code fails without retaining an old application. A previous UF2 on the computer provides manual rollback only if that independent USB entry remains reachable. Automatic rollback requires retaining both old and new firmware plus swap state. It does not follow merely from installing a smaller foundational application.

Incremental development should produce complete versioned firmware images, even when each change adds only one feature. Splitting recovery into an immutable lower module inside the same application is not automatically safe: an application UF2 can overwrite it and a failed application can bypass it. Use the established bootloader's recovery rather than inventing a custom resident stub.

## Existing Adafruit bootloader: a promising enclosure-closed route

The installed build reports upstream [0147d71e73b9a2c217f56dbc9877d07bb45d6467](https://github.com/adafruit/Adafruit_nRF52_Bootloader/commit/0147d71e73b9a2c217f56dbc9877d07bb45d6467). Its [main.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c) supports three distinct entries:

| Entry | Independent of functioning application? | Enclosure-closed evidence |
|---|---|---|
| Factory Fn+5 or Rust Bootloader/1200-baud command | No; application must run | Factory Fn+5 MSC and factory 1200-baud serial entry observed |
| Double NRST reset within ~500 ms or board DFU input | Yes | No verified external NRST/DFU control on this NocFree unit; power cycling is not NRST |
| Existing `APP_ASKS_FOR_SINGLE_TAP_RESET` image marker | Yes, if marker remains intact and vendor implements upstream behavior | Subsequently demonstrated on right USB diagnostic; left and receiver unverified |

The marker is a 32-bit word `0x87eeb07c` at the dynamically selected application base plus `0x200`. It is an upstream MakeCode-style convention, not a new bootloader stub. With this word present, the bootloader can enter USB DFU before executing the application. USB enumeration cancels the three-second startup timeout; without USB it times out and runs the application. The marker branch is not gated by RESETREAS: it applies to power-on, software and watchdog reset, not just NRST. Exceptions are GPREGRET skip magic `0x6d` and a valid app with retained RAM marker `0x4ee5677e`; upstream clears that RAM word immediately before launching the application. Existing watchdogs are fed by the bootloader while waiting in DFU. The exact power/reset workflow still needs hardware verification.

In this exact installed upstream base the three-second timer checks `tud_mounted()` only once. If USB is mounted then, the callback returns without rescheduling and DFU remains indefinitely; unplugging afterward does not start a new timeout. Starting the app afterward therefore needs a new reset/power cycle with USB disconnected. If USB is not mounted by three seconds and no DFU packet has arrived, it exits DFU and launches the valid application. The factory left backup currently contains `0x3a10eef1` at `0x27200`, not this recovery marker, so factory startup does not test the mechanism.

A candidate foundation could deliberately reserve this marker in each battery-powered half's development image: powering on with USB attached should present MSC, while powering on disconnected, waiting for startup, then connecting USB should run the keyboard. This trades convenient USB cold boot for recovery independence. It must be tested rather than advertised as guaranteed: the vendor's board build or local modifications may differ, the mode switch may not actually remove MCU power, and retained RAM affects the startup branch. The owner reports a right ON/OFF switch and a left three-position mode switch with WIRED in the middle. The left switch has no established OFF position, so disconnecting its cable cannot yet be called a cold start. A USB-only receiver cannot use the battery-start workflow; the marker would park it in DFU on every cold plug. Receiver marker builds are diagnostic only, not a normal receiver solution.

Before any trial, enforce the marker in the binary/linker at `appbase+0x200` without overwriting vector entries, preserve its flash page in every subsequent package, and verify the USB/no-USB startup sequence. After migration the app base is `0x1000`, so the marker would reside at `0x1200`; with resident S140 retained it is `0x27200`. An update interrupted while erasing the marker page remains a failure case: upstream app-validity fallback may still enter DFU, but exact interrupted-update behavior must be tested with an independent recovery/probe available. This mechanism improves recovery from logically broken but fully installed apps; it is not proof of arbitrary power-loss safety.

Ordinary runtime recovery commands remain worthwhile conveniences. Add USB enumeration and 1200-baud/Bootloader entry first, then scanning, BLE split, host BLE, receiver routing, battery and lights in separate checkpoints. Reflash/restore and verify readback between major stages. Those conveniences cannot substitute for testing the boot-before-application route.

## RMK/Embassy mechanisms and the size constraint

Current [rmk-boot source](https://github.com/rmk-rs/rmk-boot/tree/9ac30d13c50912c9c866f1f68c688e8234f836b8) includes nRF52833 support. Its [build.rs](https://github.com/rmk-rs/rmk-boot/blob/9ac30d13c50912c9c866f1f68c688e8234f836b8/build.rs) allocates 24 KiB bootloader (32 KiB with defmt), 4 KiB state and storage, then divides remaining flash between ACTIVE and DFU (DFU has an additional erase page). Its source README reports approximately 224 KiB ACTIVE for the default nRF52833 build; variant/storage choices change this slightly. A roughly 340 KiB central is too large regardless of that small variation.

[Embassy boot](https://docs.embassy.dev/embassy-boot/git/default/struct.BootLoader.html) records swap progress, can resume an interrupted swap, and reverts an unconfirmed image on a subsequent boot. Confirmation uses `mark_booted()`. A hung application does not cause a reset simply because confirmation is absent: watchdog/reset policy must ensure another boot occurs, and confirmation must follow meaningful health checks rather than happen immediately before keyboard initialization. These are automatic rollback semantics, not manual restoration of a host UF2.

RMK `dfu_nrf` integrates with the matching partition/linker layout and adds runtime update handling. It is not a drop-in enhancement of the installed Adafruit bootloader. [RMK's bootloader documentation](https://rmk.rs/main/docs/configuration/bootloader) calls DFU experimental and warns that repartitioning can require a debug probe.

`rmk-boot noswap` provides more application space but writes ACTIVE directly, with no retained prior version or automatic rollback. [Its nRF entry code](https://github.com/rmk-rs/rmk-boot/blob/9ac30d13c50912c9c866f1f68c688e8234f836b8/src/nrf528xx.rs) enters USB DFU on application-set `GPREGRET=0x57` or double NRST. Neither demonstrates recovery from a broken app with this enclosure closed. It also drives default LED P0.15, which is this keyboard's published BLE-mode input, so stock board pins must not be copied.

External SPI NOR can hold the second slot, but no such memory is documented on NocFree. Adding it changes hardware; the left external nRF24 radio is not storage. Internal automatic rollback for the current central therefore requires a substantially smaller image or different hardware.

## Why replacement is a separate project

Stock rmk-boot expects a low-flash bootloader and its generated memory layout; it is not a same-address replacement for the factory upper-flash Adafruit/MBR/S140 arrangement. The source warns that installing it overwrites the Adafruit bootloader. Migrating requires understanding reset-vector/MBR/UICR behavior, protecting or backing up factory files, and providing a proven probe/recovery path. The existing `CURRENT.UF2` backs up S140 and user application, not MBR/UICR/bootloader; it cannot undo arbitrary bootloader replacement.

An Adafruit-based board-specific bootloader that checks a verified external mode input or offers a USB startup window could provide enclosure-closed recovery, but that is a deliberate bootloader port with hardware identification and installation risks. The current installed source does not check USB attachment as an unconditional DFU trigger, and no source evidence shows the factory mode switch alone enters recovery. Do not imply that replacing the bootloader with stock rmk-boot solves those controls automatically.

The smallest justified next step is to prepare and review an Adafruit-marker foundation image and its restore package, with every package asserting the marker and layout. Establish a tested independent fallback before the first trial if possible; otherwise explicitly treat that first trial as the remaining hardware risk requiring a considered decision. If enclosure-closed marker behavior is verified, use it as the foundation for complete incremental images and manual host-held rollback, retaining the factory bootloader throughout.
