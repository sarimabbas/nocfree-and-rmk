# USB startup recovery experiment

`usb-rescue-startup` is opt-in. Final app-only builds omit the key-chord shim;
transitional hardware trials can retain the already-proven chord until USB recovery
has been demonstrated on the board.
It runs after the one Embassy HAL initialization, before MPSL and RMK. It borrows
USBD, uses the existing Embassy DFU runtime class on interface 0, then disconnects,
disables USBD interrupts/endpoints, clears events and returns ownership to RMK.
The factory bootloader, storage boundaries and UICR remain unchanged. WinUSB MS
OS 2.0 descriptors accompany the rescue interface.

The feature retains this port's established `defmt-logging` feature, including
when callers disable default features. RMK hashes its enabled features into its
storage schema: dropping logging in the first right trial caused a settings
reset, despite the updater leaving storage untouched. The owner confirmed
Companion recovery without keys and subsequent right-half typing/shared Shift;
exact readback confirmed application/padding/gap, with nine changed storage bytes
independently decoded as the schema reset. The final candidates must preserve
the existing role's complete RMK feature set.

The USB host sees experimental VID 0x4c4b and PID 0x4660 (left), 0x4661 (right),
or 0x4662 (receiver). A scoped class/interface DETACH request with an empty payload
requests the existing GPREGRET 0x57 recovery reset. The existing DFU handler resets
before its status ACK, so a host transfer error alone is neither success nor failure;
the subsequent identified bootloader is the result to observe.

Polling VBUS avoids enabling CLOCK_POWER's MPSL handler before MPSL is initialized.
No VBUS at stage entry skips USB; present VBUS offers a nominal two-second window,
followed by a 25 ms disconnect before normal RMK descriptors. This timer bounds
async polling, not the driver's synchronous hardware/DMA wait loops.
HAL retains its normal clock configuration. Only USB-present rescue requests HFXO,
with a 100 ms asynchronous readiness limit, and releases its own request before
MPSL starts; an already-outstanding preceding-stage request is preserved.

## Cross-build measurements

Release, no default features, `usb-rescue-startup` retaining `defmt-logging`, existing RMK/Embassy revisions,
the repository's canonical cached build without C compiler environment overrides.
Halves also select reclaimed-softdevice,
mac-keymap and backlight-active-high; receiver preserves its factory SoftDevice.

| Role | BIN bytes | Application origin | Padded end | Application limit |
| --- | ---: | --- | --- | --- |
| Left | 389504 | 0x1000 | 0x61000 | 0x65000 |
| Right | 227072 | 0x1000 | 0x39000 | 0x65000 |
| Receiver | 251264 | 0x27000 | 0x65000 | 0x65000 |

These final candidates omit the key-chord feature. BIN sizes use the canonical
Arm GNU exporter with erased flash gaps. The earlier no-logging transitional
right trial was 228172 bytes and ended on the same `0x39000` page boundary.

Left static initialized data is 4812 bytes and BSS is 58304 bytes; receiver data
is 4572 bytes and BSS is 39408 bytes. Dynamic stack acceptance remains hardware work.
Right data is 4636 bytes and BSS is 33712 bytes.
The receiver has only 2688 bytes of unpadded flash headroom.

Restoring logging preserves the established left and right storage schema hashes.
The current right trial already wrote the no-logging schema, so correcting it
causes another settings reset. The receiver's saved working image uses an older
RMK revision: its proposed update changes the schema even with logging preserved.
Receiver pairing and recovery need a separate device acceptance trial; this table
does not establish that its existing settings will survive the update.

A separate initialized Embassy USB stage was also measured at 12336 BIN bytes.
A page-aligned independent prefix would consume 16 KiB and would not fit the
receiver's previous 8 KiB headroom. The integrated stage shares existing framework
code and avoids a second HAL initialization and unsafe direct-vector handoff.

## Limits and required device observations

These are cross-build and source observations, not hardware validation. The stage
protects against failures in later RMK code, provided startup/HAL/stage remain sound.
USB attach does not restart an already battery-powered half. A real reset/startup
is still required. There is no watchdog and no changed bootloader.

Before installation is considered proven, observe normal USB enumeration after
the window, targeted Companion DETACH and matching drive/readback, battery startup,
and recovery with deliberately non-running RMK. Verify release recovery, simultaneous
split input, disconnect/reconnect and wake latency after the new USB stage.
