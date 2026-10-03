# Battery drain: source audit and measurement plan

Read-only research, 2026-10-03. No firmware changes or device commands. This audit
examines the working RMK source, not the temporary USB-only inspection application's
energy use. Hardware recovery remains the immediate priority.

## Established and unestablished facts

The owner photo shows a left battery labeled 1250 mAh, 3.7 V, 4.625 Wh. This is a
label rating, not a measured remaining capacity or charge rate. The controller
photo shows two RF module assemblies. Two RF modules do not establish two MCUs:
the vendor [architecture](https://github.com/NocFreeKB/NocFree-and-zmk#3-hardware-architecture)
identifies the nRF52833 controller and a separate nRF24L01 radio on the left.
Unreadable component markings were not used to identify charging ICs or regulators.

The present percentages cannot establish battery drain or failed charging. They
come from sampled voltage and a generic conversion, with no verified charging
state GPIO, cell curve, divider calibration, or current measurement.

## Source-backed power opportunities

| Area | What the source actually does | What remains to measure |
| --- | --- | --- |
| Scanner | `firmware/src/scanner.rs` reads all three expanders continuously, then yields for 1 ms, including with no keys pressed. Its comment estimates 1.5 ms wire time at the default 100 kHz bus. No expander interrupt pin gates idle scans. | Idle current and first-key/release latency with interrupt-driven idle scanning. The nominal ~2.5 ms scan cycle is a source estimate, not a measured waveform. |
| Split sleep | `firmware/keyboard.toml` explicitly sets `split_central_sleep_timeout_seconds = 0`. Pinned RMK `rmk/src/ble/sleep.rs` treats zero as disabled. | Current reduction and wake acceptance from framework sleep; do not enable a timeout until first-key, simultaneous input, disconnect and release behavior are tested. |
| Backlight off | Pinned RMK `rmk/src/backlight.rs` sends level zero through `NrfPwm::set_level`, which still invokes `BufferedPwm::set_duty_async`. Pinned Embassy initializes PWM enabled, starts its sequence, and does not issue STOP/disable on zero level; disabling appears only in explicit methods/drop. | Current with brightness zero versus PWM stopped and output held at the verified off polarity. This is a likely avoidable peripheral load, not a measured number or confirmed dominant cause. |
| BLE links | RMK requests an awake split interval of 7.5 ms and peripheral latency 30. It has separate sleep parameters. The left also maintains a host or receiver route. | Negotiated intervals, packets/retries and whole-board current, separately for USB, direct BLE and receiver. Requested parameters do not prove negotiated behavior. |
| Extra left radio | RMK does not use the external nRF24L01 path and contains no explicit external-radio configuration/power-down sequence. | Its CE/CSN levels and actual supply current/state. It may already be in its hardware reset state; absence of an initialization sequence does not prove it is receiving continuously. |
| ADC divider | `firmware/src/battery.rs` enables the divider for a 10 ms settle plus one sample, disables it via RAII, then waits 30 seconds. | Divider-enable active level and true off current. The source does not leave the divider enabled continuously. |
| Clocks | Main RMK uses the HAL default internal HF clock configuration; it does not force the external HFXO continuously. MPSL owns radio clocks, with LFRC calibration configured. The temporary inspection probe explicitly selects HFXO and is a different workload. | HFCLK/PWM/TWIM state and board current at idle. Do not attribute all battery behavior to a permanently requested HFXO without measurement. |

Nordic describes HFCLK-dependent peripherals and recommends stopping/disabling
unused TWI hardware for minimum consumption. Its PWM documentation distinguishes
loading the last duty value from explicitly stopping the generator; STOP forces
outputs into their configured idle levels. Therefore off-level PWM should be
tested as a power-management change rather than assumed free.
[nRF52833 TWI](https://docs.nordicsemi.com/r/bundle/ps_nrf52833/page/twi.html),
[PWM](https://docs.nordicsemi.com/r/bundle/ps_nrf52833/page/pwm.html).

## Why the battery display can fall while USB is attached

`BatteryProcessor::new(100, 130)` applies the vendor-published 130/100 divider
scale. Pinned RMK's conversion assumes the default 12-bit SAADC, 0.6 V reference,
1/6 gain and a rough 3.6–4.2 V capacity range. It maps scaled counts linearly;
it is not a cell-specific fuel gauge. Near the mapped limits a small voltage
change can produce several percentage points of movement.

Firmware does not instantiate `ChargingStateReader`. Absent charging events,
RMK publishes `ChargeState::Unknown`; USB presence is not a charging measurement.
Voltage can vary with load, cell relaxation and charging circuitry. A displayed
decline during USB attachment alone cannot distinguish those from genuine net
discharge or a calibration error. The hardware charge controller's identity,
programmed charge current and power-path behavior are not established by this
photo or the current firmware.

## Minimal useful measurement sequence after recovery

1. Log raw ADC count, calibrated battery millivolts and timestamps, separately
   for each half. Compare with a meter at the battery connector using safe
   accessible measurement points; establish divider and ADC calibration first.
2. Measure average battery current with both halves idle and backlight off,
   then while typing, then with a fixed backlight level. Measure USB input current
   separately; USB current includes device operation and is not itself battery
   charge current.
3. In separate one-variable trials, stop PWM at level zero, use expander interrupts
   for idle scanning, and evaluate RMK's existing sleep. Record both current and
   first-key/release/disconnect/wake acceptance before integrating changes.
4. Identify the actual charger from readable markings/vendor evidence, then
   establish its charge-status signal and net charge behavior. Only then add
   reliable charging/discharging labels and a cell-appropriate percentage curve.

Until those measurements exist, the honest UI is an estimated level with unknown
charging state. A build, successful typing, or a falling percentage is not a
battery-runtime or charge-controller validation.
