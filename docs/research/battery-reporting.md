# Battery reporting

Audited 2026-10-02 local time against installed RMK fork `b41bd7caa7de67a47f6e7380519b730630f16152` and Embassy nRF fork `1b5fc397aa65026925d9641d88073a7e91993647`. Existing firmware already measures both batteries; a firmware update is unnecessary to expose their current status in Companion.

## Live observation

A bounded read-only Rynk USB session selected exactly one `NocFree RMK` product, excluding the receiver. Its handshake read protocol version/capabilities, followed by `GetBatteryStatus` and `GetPeripheralStatus(0)`. Results:

| Battery | Reported level | Charging state | Split connection |
|---|---|---|---|
| Left | 12% | Unknown | Central |
| Right | 2% | Unknown | Connected |

These are live firmware-reported voltage estimates, not independently measured remaining capacities. Private source, lockfile, probe hash, timestamp and result are saved under `.evidence/battery-reporting/`. The probe used Rynk/Rynk USB 0.3.0 from the exact installed fork, nusb 0.2.7 and Tokio 1.53.1; it issued no setters, unlocks, subscriptions, resets, serial commands or firmware writes. Dropping the driver releases its vendor-interface session.

Read-only macOS Bluetooth inventory listed `NocFree RMK` without battery fields. That inventory does not establish whether macOS can discover either Battery Service or display both levels.

## Sampling and conversion

The [vendor-published guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md) assigns ADC P0.04 to both halves, divider enable P0.05 left/P0.31 right, active-high enable, 12-bit sampling and measured-voltage multiplier 130/100. These are published hardware claims, not new meter/oscilloscope observations. The community ZMK port's `docs/limitations.md` explicitly excludes battery implementation; it provides pin information rather than a ready-made battery driver.

Our board seam enables the divider, waits 10 ms, samples once, disables it before publishing and repeats after 30 seconds. A drop guard disables the divider on cancellation. ADC calibration runs at initialization. The [pinned Embassy SAADC defaults](https://github.com/sarimabbas/embassy-nrf-nocfree/blob/1b5fc397aa65026925d9641d88073a7e91993647/src/saadc.rs) are 12 bits, internal reference, gain 1/6, 10 µs acquisition and no oversampling. With nominal 0.6 V reference, the conversion is approximately `Vbattery = count × 3.6 / 4096 × 1.3`; the nominal count range is about 3151 at 3.6 V to 3676 at 4.2 V. This agrees with the configured `BatteryProcessor::new(100, 130)` ratio; no configuration mismatch was found.

RMK clamps an approximately linear voltage estimate using normalized counts 4055–4755. It is not a calibrated lithium-battery discharge model. Actual divider ratio, reference/gain error, 10 ms settling, loaded voltage and capacity curve remain unmeasured. No `ChargingStateReader` is configured; Unknown is the correct result. USB presence must never be displayed as proof of charging. Charge-indicator pins are shared hardware and must not be driven push-pull merely to add a charging icon.

## Existing reporting paths

The [battery processor](https://github.com/sarimabbas/rmk-nocfree/blob/b41bd7caa7de67a47f6e7380519b730630f16152/rmk/src/input_device/battery.rs) caches the central's current status. The [split peripheral](https://github.com/sarimabbas/rmk-nocfree/blob/b41bd7caa7de67a47f6e7380519b730630f16152/rmk/src/split/peripheral.rs) forwards battery changes and sends its cached status when the central's connection message arrives. The central caches peripheral slot 0; Rynk reads those producer-owned snapshots directly.

The [BLE Battery Service](https://github.com/sarimabbas/rmk-nocfree/blob/b41bd7caa7de67a47f6e7380519b730630f16152/rmk/src/ble/battery_service.rs) exposes separate standard 0x180F/0x2A19 service instances labelled Left and Right, with percentage presentation formats, read and notify. Initial connection snapshots prevent stable levels waiting indefinitely for a change. Later central notifications are activity/sleep gated; peripheral notifications follow split events. Native OS presentation of both instances needs its own acceptance check.

Rynk reads over left USB avoid that OS UI dependency. The receiver source relays Rynk frames; this audit has not validated battery reads through its installed image. Standard USB keyboard HID alone does not expose these percentages to a native battery panel.

## Smallest next step

Display the existing two getters in Companion. Keep charging Unknown unobtrusive. A disconnected peripheral retains its last cached battery: use the separate `connected` flag to mark that value stale/unavailable. Polling faster than the 30-second sample interval cannot improve measurement freshness. No firmware defect requiring an immediate correction was demonstrated.

Charge both halves, especially the reported 2% right, then repeat the same getters after several minutes. Confirm both remain available and plausible; unplug charging cables and re-read after a fresh sample. Inspect direct-BLE Left/Right characteristics separately when practical. Meter-based calibration, charge-state wiring/polarity, drain/current, low-voltage behavior, all-OS presentation and battery-life claims remain pending; the successful status read establishes none of those.

## Low reported level while USB-powered

The owner reports the left has been connected to its USB dock for much of the day. That challenges interpreting 12% as the true state of charge. Subsequent root observations using the same getters were Left 11%/Right 15%, then a Companion screen showing 13%/13%. They are different current snapshots, not boot-time constants. The owner subsequently confirmed plugging in the right USB cable; its increase is cable-correlated, although charger current remains unmeasured. The change also means a single snapshot must not be used to claim the battery is empty, charging, healthy or calibrated.

Current Rynk getters expose percentage and charge-state enum only. Neither raw ADC counts, sample timestamp nor measured millivolts is available. Under the exact configured RMK formula, a reported 12% corresponds to counts 3184–3189 and nominal 3.638–3.644 V; 2% corresponds to counts 3130–3135 and nominal 3.576–3.582 V. These are inverse calculations from the estimate, not independently observed raw samples or voltage. Initial one-shot offset calibration corrects ADC offset; it does not calibrate the divider, gain, reference or discharge curve.

Read-only analysis of the saved original left image adds a useful distinction: the stock battery loop calls its ADC wrapper at `0x2c8bc`, disables the divider, then multiplies by 3300 at `0x2c8d6`. The reciprocal arithmetic at `0x2c8de`–`0x2c8e8` divides by 4095, and `0x2c8ea`–`0x2c8fc` applies 130/100. It smooths old/new voltage 75%/25% and compares against a 2700 mV low-voltage threshold. The private disassembly and binary remain under `.evidence/backlight-pwm-diagnosis/`; no factory bytes are published. This establishes a stock software conversion of approximately `ADC × 3.3 / 4095 × 1.3`; it does not establish that this conversion matches the physical divider/reference.

The stock ADC wrapper programs gain/reference from globals, uses a direct single-ended sample, and the stock initialization selects 12 bits. Its structure matches [Adafruit's primary Arduino SAADC implementation](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/master/cores/nRF5/wiring_analog_nRF52.c), which defaults to internal 0.6 V with gain 1/6 (3.6 V range). The saved reset path at `0x27264`–`0x2726a` zeros BSS `0x200067c0`–`0x2000de18`, including gain `0x2000ddd4`, reference `0x2000ddd8`, acquisition `0x2000dddc` and burst `0x2000ddd0`. The ADC wrapper loads those fields at `0x44964`–`0x4498a`; zero gain/reference selects 1/6/internal. Each exact global address appears only once in the image, in that load's literal pool; no direct setter was found. This establishes matching reset defaults, not a complete proof against all possible indirect writes or physical calibration error. Replacing RMK's 3.6 scale with the stock 3.3 constant would *lower* the inferred voltage by about 8.3%, so it cannot explain a low report by turning 12% into a full battery. The shared 1.3 ratio matches the published guide; the guide expressly calls for calibration and provides no complete schematic.

The vendor documents releasing the red charge/status pin while USB-powered. Current firmware does not configure the shared left P0.09/right P0.17 charge/status pins as outputs, nor configure a charge-enable GPIO. Neither vendor documentation nor this audit establishes a firmware-controlled charging gate, charge-complete polarity, or USB-mode-dependent charging circuit. A functioning USB keyboard proves MCU USB power and data operation, not current entering the battery.

Investigation priorities, without claiming a diagnosed cause:

1. Validate the estimate: uncertain divider/reference and a linear curve are direct source-visible limitations. Raw sample/voltage observation is the strongest next discriminator; current getters cannot supply it.
2. Check actual charging: USB/VBUS can be present while the battery/charger path is inactive. Split sleep is disabled and backlights can add load; whether load approaches the charger current is unknown. Neither current nor charger component is verified. Observe the hardware's charging indication and compare a later snapshot, but do not equate indicator color or USB attachment with measured charging current.
3. Check measurement stability under load: the sampling path uses one sample and lacks the factory's smoothing. The varying right reports motivate raw-sample comparison, not a speculative smoothing patch that could hide a scale or charging problem.

A read-only repeated GET can determine whether the *reported* value changes; it cannot distinguish these causes. A meter measurement or a separately reviewed raw-ADC diagnostic would be needed to resolve that boundary. Firmware and charging GPIOs remain unchanged during this audit.

## Direct USB follow-up

The owner moved the left from its dock directly to the Mac after being asked to turn the backlights off, and confirmed that the right had just been connected by USB. A GET-only read returned Left 13% / Right 9%. Three subsequent successful reads 35 seconds apart returned Left 12%, 12%, 12% and Right 8%, 9%, 8%; the right remained connected over the split link and both charging states remained Unknown. Private timestamped results are retained under `.evidence/battery-reporting/`. This short observation shows right percentage fluctuation and no immediate correction of the left estimate after moving off the dock; it does not establish discharge, charging current, dock fault or calibrated capacity. The nominal linear conversion spans about 6.15 mV per percentage point, so small voltage changes can produce visibly different percentages. This is a property of the formula, not a measured voltage-noise result. No firmware or charger GPIO changes were made.

## Companion status preservation, 2026-10-03

A reproducible host defect was found at the actual getter-to-view conversion:
`desktop/src/battery.rs` reduced `BatteryStatus::Available` to its percentage,
discarding the producer's charge state and conflating an available status with
an unknown percentage with an unavailable battery. A test passing a valid
`Charging` status through the actual conversion failed before the fix. The
conversion now preserves the framework's complete typed `BatteryStatus`, rejects
levels above 100, and makes a disconnected right half unavailable rather than
retaining its cached charge state. Eight tests of the actual module pass,
including unknown percentages, zero percent and disconnected snapshots. The
GET path also captures the selected Rynk USB device ID and performs a bounded
read-only enumeration after the getters. It accepts the snapshot only when the
same unique ID remains present, rejecting a missing, replaced or duplicate
connection. On macOS, nusb uses the IOKit registry entry ID rather than the
physical USB port as this identity; no persistent device identity is stored.
This is a host freshness check, not a new battery measurement.

This fixes host information loss. The installed firmware currently reports
`ChargeState::Unknown`, so this change does not establish that either battery
is charging or correct the low percentage estimate. No ADC scale, GPIO,
sampling behavior or firmware image was changed for this fix. The saved
percentage trace remains insufficient to reproduce a calibration error:
it contains neither raw samples nor a simultaneous voltage reference.

The next calibration discriminator is one paired observation on a single half:
backlight off, battery-terminal voltage from a meter, the same interval's raw
ADC count and its reference/gain/resolution configuration. With the present
configuration, compare the observed ratio against
`Vbattery = count × 3.6 V / 4096 × 1.3`. If that relationship holds, investigate
the generic capacity curve and actual charge path; if it does not, investigate
divider scaling, settling and acquisition first. A burst of raw samples then
distinguishes measurement noise from a persistent scale error. Current Rynk
getters expose no raw count, so a separately reviewed temporary measurement
path is needed before that observation can be made. Changing the percentage
curve or adding smoothing beforehand would hide the unresolved distinction.
