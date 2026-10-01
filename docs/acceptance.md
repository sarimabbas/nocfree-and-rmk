# Hardware acceptance gate

Status: **not run**. Cross-compilation and host tests do not establish device behavior.

Record firmware commit/hash, layout, board revision, bootloader identity per role, OS versions, cable, battery state, measurement method, and raw trace path in a local `.evidence/` directory. Public results should omit serial numbers and Bluetooth addresses.

## Recovery before replacement

1. Preserve the official ZIP and verify its hash against `docs/research/factory-images.json`.
2. Confirm normal enumeration with one connected device.
3. Observe the vendor-supported bootloader entry without erasing anything. Save `INFO_UF2.TXT` and inspect bootloader version, board ID, family and SoftDevice.
4. Confirm a complete factory application rollback route for that exact role. The ZIP contains application images, not a complete bootloader backup. Do not replace the bootloader, SoftDevice, filesystem or UICR.
5. Inspect any `CURRENT.UF2` backup for coverage; never assume it includes pairing/configuration or the bootloader.
6. Only after the app handoff and protected ranges are confirmed may an RMK application be packaged for a physical trial. Existing factory app starts at `0x27000`; this alone does not establish compatibility with RMK's controller stack.

## Scanner and input

- Verify each physical key independently against the ANSI map, then every modifier chord, layer key, and repeated press/release across both halves.
- Press keys on both halves simultaneously, including all populated positions; compare expected and observed HID press/release streams. Every missing or duplicate transition, stuck key, I²C error or queue overflow is a failure.
- Use a repeatable electrical or mechanical actuator and timestamp switch actuation and host event capture with a shared clock. A human typing test cannot quantify missed events or latency.
- Run at least 100,000 transitions in USB, direct BLE and receiver modes, separately. Include fast taps, holds, rolls, chords, and saturation. Store raw traces and report count, median, p95, p99 and maximum press/release latency. Set a numerical latency budget with the owner before marking a pass; there is no physically meaningful zero-latency budget.
- Test sustained traffic under Wi-Fi/Bluetooth interference and weak signal. Zero loss in a finite run is evidence for that workload, not a universal guarantee.

## Disconnects, host and power

- Disconnect the right half while a key is held, reconnect, and verify release recovery without a stuck modifier or ghost key. Repeat while host or receiver disconnects.
- Test USB attach/detach, host sleep/wake, connection loss/reconnect, profile change and physical transport switch under input. State clearly whether keys during outages are intentionally dropped or replayed. Do not silently equate reconnect with reliable input.
- On macOS, Windows and Linux independently verify enumeration, ordinary keys, NKRO/chords, media keys, caps-lock host output, BLE pairing/unpairing, receiver pairing and persistence after reboot.
- Test battery reporting for both halves while charging and discharging; compare voltage with a meter. Verify divider-enable polarity and ADC settling. Do not treat a nominal voltage curve as calibrated remaining capacity.
- Measure idle/active/charging current, low-battery behavior and wake latency. Initial development disables split sleep to avoid unmeasured wake delay; battery life is unproven.

## Feature accounting

A feature is complete only when its implementation, build and hardware test all pass. Track factory backlight control, status LEDs, physical mode switch, web remapping, optional numpad, and factory dongle protocol separately. RMK USB HID compatibility does not make factory configuration software or receiver protocols compatible.
