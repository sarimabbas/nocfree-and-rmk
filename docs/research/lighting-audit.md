# Lighting state audit

Audited 2026-10-04 against RMK fork `a3802774bbec18d004787d2c50618b6ea1942917`
and the combined LED/scanner candidates. This audit changes no firmware.

## Left status indicator

The status indicator describes the left's selected host link, not the right's
split connection. USB power and selected host mode are separate facts.

| Awake state | Battery only | USB powered |
| --- | --- | --- |
| Wired | No firmware mode light | Charger may show red |
| Bluetooth advertising/reconnecting | Blue, 500 ms on / 500 ms off | Same blue blink, with any charger red superimposed |
| Bluetooth connected | Solid blue | Solid blue, with any charger red superimposed |
| Bluetooth inactive | Off | Charger may show red |
| Dongle advertising/disconnected | Off | Charger may show red |
| Dongle connected | Solid blue + red; owner observed purple | Firmware blue only; charger red can make it purple |
| Sleeping | Firmware outputs off | Local USB power prevents ordinary idle sleep; charger remains independent |

The right does not receive the host-mode color changes, by design. It connects
to the left rather than selecting the host transport.

### Findings

1. **Color is ambiguous when USB is connected.** Charging Bluetooth can look
   purple, while dongle mode can look blue after charging completes. The red
   output shares charging hardware. The board adapter releases this open-drain
   output on detected VBUS rather than driving it high or competing with the
   charger. A universal purple=dongle convention is therefore not supported.
2. **Connection feedback differs by mode.** Bluetooth advertises with a blink;
   a dongle search has no firmware light. Bluetooth blinking also includes
   reconnecting a saved host, not just initial pairing.
3. **Solid light does not prove the whole keyboard is connected.** The right
   can be disconnected while the left retains a solid host indicator.
4. **Status brightness is fixed.** F5/F6 affect key backlights only. Connected
   status remains on until sleep or a connection/mode change. Purple uses both
   channels on battery. Its current and battery-life cost have not been measured.

Owner observations confirm purple dongle operation, blue Bluetooth
advertising/connected operation, and working typing across USB attach/detach.
They do not establish the optical result for every charging combination.

## Key backlights

Both halves use 400 Hz PWM and 16 levels, including off (0) and full (15).
Duty is linear, so perceived brightness steps need not appear equally spaced.
Tap changes one level; hold repeats after 350 ms at 80 ms intervals. Limits
stop repeat scheduling. The newest held direction wins; releasing its owner
stops repetition without resuming an older key.

The left owns saved brightness and sends absolute snapshots to the right.
Storage is delayed two seconds after the last change. Sleep temporarily applies
off without overwriting saved brightness. Wake restores the level. A locally
USB-powered half remains awake even if its battery-powered partner sleeps.
Startup can briefly be dark before storage or the central snapshot arrives.

### Findings

1. **Off does not stop PWM.** `NrfPwm::set_level(0)` submits zero duty, and
   `BufferedPwm::set_duty_async` starts a sequence without issuing STOP.
   Sleep uses this same zero-duty path. The light is off, but the PWM generator
   remains active. This is a software power defect; the current penalty is
   unmeasured. Nordic specifies that generation continues at the last value
   until explicitly stopped. Fix off/sleep to stop PWM and restore it on wake,
   preserving DMA lifetime and cancellation safety.
2. **A remapped right-side held brightness key survives disconnect.** A focused
   isolated test pressed brightness-up, published the actual split-disconnect
   event, then advanced the repeat deadline. Level advanced from 6 to 7 instead
   of remaining 6. No backlight subscriber clears the held action on disconnect.
   Default F5/F6 are on the left, so this requires a peripheral remap. This is a
   host service reproduction, not a radio/hardware test.
3. **Right reconnection can retain illumination.** Its reconnect loop clears
   peripheral sleep, tries the saved central for ten seconds, then searches for
   up to 300 seconds. There is no immediate backlight-off transition on link
   loss. On battery, a previously lit right half can therefore stay illuminated
   while seeking the left. Actual duration remains a hardware test.

## Validation and recommended order

- Isolated framework run: 275/275 tests passed with storage, backlight and
  status mixing.
- Split/Vial/async-matrix/BLE feature run: 300/300 tests passed.
- Additional disconnect regression: failed with the expected 7-versus-6
  counterexample. Temporary test source was removed from the fork afterward;
  reproduction and logs are private evidence.
- No code changes, cross-builds, flashes or measured current claims in this audit.

Fix PWM shutdown first, then held-action cancellation. Make dongle searching
feedback consistent with Bluetooth, and settle the charging overlay convention
before describing colors as a reliable mode identifier. Keep the current
brightness controls, saved-level behavior and local USB awake override.

## Evidence

- Board: `firmware/src/status_indicator.rs`, `firmware/src/main.rs`.
- Pinned RMK: `rmk/src/status_led.rs`, `rmk/src/backlight.rs`,
  `rmk/src/backlight/hold_tests.rs`, `rmk/src/ble/sleep.rs`,
  `rmk/src/split/driver.rs`, `rmk/src/split/ble/peripheral.rs`.
- Pinned Nordic HAL: `embassy-nrf-nocfree` revision `4e3d8c7`, `src/pwm.rs`.
- [Nordic PWM specification](https://docs.nordicsemi.com/r/bundle/ps_nrf52833/page/pwm.html?contentId=tF4489dcQq9LN2iwRSaljg).
- [Community board pin evidence](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting).
