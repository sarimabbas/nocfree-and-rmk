# Implementation and hardware bring-up plan

Status, 2026-10-01: the complete left, right and receiver applications cross-build in the explicit reclaimed-SoftDevice layout. Only the right USB recovery diagnostic has run on the owner's board. Its successful restore/reinstall and cold-start recovery trial do not validate keyboard scanning, radio, battery or lighting. Left and receiver remain factory firmware. No hardware writes are part of the unattended software work.

## Next implementable milestone

The next software milestone is a reviewed input/recovery bring-up candidate with unchanged production split ownership: run the host scanner harness, build all three roles, package only guarded images, and prepare the complete physical key-sweep and held-key disconnect checklist. Keep the already tested recovery probe available as its own checkpoint. Do not fold unimplemented backlight, physical-selector APIs or power optimization into this candidate.

Offline work can finish source review, repeatable fault tests, build/size checks, image packaging and targeted RMK API proposals. Owner-assisted work begins with choosing the connected role, proving recovery for any role not already tested, installing a specifically reviewed candidate, physically exercising keys, cycling power, pairing hosts/receiver, and measuring voltage/current/latency. The left migration and receiver installation stay blocked on their own recovery evidence rather than on a software build result.

## Boundaries and evidence

Keep the conventional three-role architecture: right publishes physical input over RMK BLE split; left owns the full keymap and routes reports to USB, host Bluetooth or the RMK BLE receiver; receiver exposes USB HID. Wired host mode still uses a wireless inter-half link. The published hardware does not establish a wired inter-half data connector. Factory receiver compatibility would require implementing a separate proprietary radio protocol, so the original receiver must eventually be backed up, given its own proven recovery route, and reflashed.

The pinned framework is RMK [`9607aedf343b17dd6b27307583ae80c4f728fbbd`](https://github.com/rmk-rs/rmk/tree/9607aedf343b17dd6b27307583ae80c4f728fbbd). The following implementation evidence is specific to that commit, rather than an assertion about future releases:

- [`keyboard.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/keyboard.rs): `process_user` and `fire_user_hold` implement profile selection/clearing and dongle selection/pairing; `Action::Light` currently warns that light control is unsupported.
- [`ble/profile.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/ble/profile.rs), [`state.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/state.rs) and [`channel.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/channel.rs): profile commands, the dongle slot and preferred-transport mutation are crate-private. They are not a public board switch API.
- [`input_device/battery.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/input_device/battery.rs): `BatteryProcessor` owns voltage-to-percentage conversion and charging state; `ChargingStateReader` already supports a polled input with configured polarity.
- [`input_device/adc/nrf.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/input_device/adc/nrf.rs): `NrfAdc` owns periodic ADC event production but has no measurement-enable GPIO/settling hook. The existing NocFree divider-gating adapter supplies that electrical requirement without replacing the battery processor.
- [`split/mod.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/mod.rs) and [`split/peripheral.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/peripheral.rs): battery, sleep and lock-indicator messages already exist. There is no brightness-level synchronization message. Key writes currently discard their result after consuming an event; full reconnect matrix synchronization is not established.
- [`ble/ble_server.rs`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/ble/ble_server.rs): central and peripheral Battery Services are part of the host GATT server. This does not require each OS to display both instances in its battery UI.

Hardware mappings come from the [vendor-published community README](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md), with uncertainty recorded in [hardware research](research/hardware.md). The community port supplies valuable board knowledge, not drop-in RMK implementations of battery, lighting, mode switching or receiver support.

AGENTS.md limits custom code to the scanner seam and image-safety tools. Missing general framework behaviour belongs in RMK, with focused tests and an upstreamable change, rather than in a parallel NocFree transport/key-action engine. Do not start a local lighting/pairing subsystem merely because a public API is missing.

## Feature accounting

| Feature | Current software | Hardware status | Next smallest step |
|---|---|---|---|
| 84-key ANSI layout and scanning | Expander adapter and RMK debounce integrated; host tests cover bus failures and mapping | No complete physical key sweep | Validate each half's electrical order and simultaneous transitions |
| Left USB HID | RMK transport integrated | Untested | Establish left recovery and application handoff before a guarded trial |
| Right-to-left BLE split | RMK central/peripheral roles integrated | Untested | Pair only these two units, then test reconnect and held-key cleanup |
| Direct host Bluetooth | Five stored profiles and clear/re-pair actions mapped | Untested | Test native pairing, reboot persistence and profile changes on each OS |
| Receiver USB HID | RMK receiver role integrated | Factory receiver untouched | Identify, back up and prove receiver recovery before installation |
| Both batteries | Divider-gated ADC sampling and RMK processor integrated; two descriptions configured | Uncalibrated and untested | Compare voltage with a meter and inspect both GATT service instances |
| Backlight | Not implemented; existing Fn brightness keys target the host screen | Pin polarity and PWM behaviour untested | Add framework light processing/split sync before a minimal electrical output adapter |
| Physical left selector | GPIO mappings published; no reader integrated | Untested | Expose an idempotent RMK output-selection command, then supply stable GPIO state |
| Sleep/power policy | Aggressive split sleep disabled; scanner polls continuously | Current draw/wake delay unmeasured | Measure baseline, then add scanner interrupt-assisted idle wake |
| Right ON/OFF | No switch GPIO published or software handling implemented | USB remains powered with OFF; tested startup sequences distinguish USB/battery starts | Check battery-only isolation and current; do not claim a mechanical switch repair |

## Stages and exit criteria

### 1. Keep the recovery foundation separate

Retain the USB-only right probe and verified backups as a checkpoint. A typing trial is a new candidate, not the already tested diagnostic. The production right role forwards input to the left and does not currently expose a local USB keyboard; a dedicated USB scanner test candidate, if needed, must remain clearly separate from that role and obey the same image guard.

Right application firmware fits beside the unused resident S140. Current RMK does not use that resident stack on either half or receiver: its SDC/MPSL controller is linked into the application. Preserve S140 during early right bring-up. The full left currently requires the larger application layout; reclaiming S140 remains a separately gated migration. Do not replace the bootloader as a shortcut.

The left needs device-specific evidence that recovery survives failed replacement firmware before migration. The receiver needs its own identity, backup and entry route. The right's USB-first marker is unsuitable as the receiver's final startup policy: a permanently USB-powered receiver must normally start typing on cold plug. Updating each role remains a host-assisted guarded restore/update procedure; internal automatic A/B rollback does not fit the current full left image.

Exit: role-specific candidates and rollback instructions are reviewable, and all required hardware recovery evidence exists for the next trial. While the owner is absent, build/package/review only; no mode changes, serial DFU entry or installation.

### 2. Establish physical input and USB host behaviour

Check the 37 left and 47 right positions against physical keys. Validate modifier chords, both Fn keys, layers and simultaneous input. Preserve full snapshots on I2C faults; keep RMK's debounce and key actions. Establish left USB typing and ordinary/media reports before introducing host Bluetooth or the receiver.

Exit: an actual complete key sweep, repeatable transition capture and USB disconnect/suspend/reconnect tests pass. Source mappings and cross-builds alone cannot close this stage.

### 3. Establish split reliability and host pairing

Use the existing RMK split discovery, stored peer addresses and reconnect management. Host pairing is distinct from inter-half peer discovery. Current keymap taps Fn+1..5 to select host slots; holding one for five seconds clears that slot and re-pairs. Fn+0 clears the selected host bond. Fn+B toggles USB/BLE preference. Fn+U selects the dedicated receiver slot; holding it for five seconds clears that receiver bond and seeks a receiver. Verify gestures against real host pairing UI rather than copying factory combinations.

Test peripheral loss with ordinary keys and modifiers held, reconnect with keys still held, brief weak-signal interruptions and concurrent input. An event consumed before a failed split write is a concrete upstream reliability question. Reproduce it in a focused RMK harness before changing framework behaviour; an event retry alone is insufficient without deciding duplicate handling and physical-state reconciliation. Fix or document the framework failure before claiming reliable release recovery.

Expose split-peer reset through RMK's existing `ClearPeerEvent`/hold action if needed; do not create another peer database. Host bond clearing should not erase unrelated host profiles or the receiver slot.

Exit: peer and host persistence, profile clear/re-pair, held-key cleanup and bounded reconnect behaviour pass on device. The no-loss claim remains limited to the measured workloads in [acceptance](acceptance.md).

### 4. Add the receiver and physical mode selector

Keep left aggregation/keymap ownership unchanged. Use RMK's receiver bond slot and BLE-to-USB relay. Test startup order in both directions, receiver unplug/replug, host suspend, and left/right reconnect during receiver use. Measure the extra radio hop; topology does not itself guarantee a latency budget.

For the left selector, published pull-up inputs are P0.15 low for BLE and P0.17 low for dongle. The owner reports the middle position is wired. Verify the stable truth table on this revision; treat both inputs low as an invalid state pending electrical evidence.

A physical switch needs absolute, idempotent selection at startup and on transitions. Synthesizing the existing USB/BLE toggle is unsafe when boot state or persisted preference differs from the switch. The smallest compliant framework addition is a public output-selection command handled by RMK's existing transport/profile machinery. It should choose USB, the last selected ordinary BLE profile or the dedicated receiver profile, reuse RMK release/channel handling, and avoid storage writes for repeated unchanged samples. The board only provides stable switch readings through its permitted hardware-input seam. Do not hard-code the private receiver slot number or maintain a second preferred-transport state.

Exit: each selector position chooses the intended output after reboot and during held-key input; preference changes release keys on the old host, and reconnect does not leak stale modifiers.

### 5. Complete battery and lighting

Retain the existing divider-gated sampling: ADC P0.04 on both halves, enable P0.05 left/P0.31 right, published voltage multiplier 1.3. Check settling time, ADC configuration and voltage against a meter. RMK's current percentage mapping is a voltage estimate, not a learned fuel gauge. Verify right status refresh after reconnection and both host GATT services. Record what each OS actually displays; standard USB HID does not promise a battery percentage UI.

Charge indicator pins are shared circuitry (P0.09 left/P0.17 right). Confirm they can safely be read as charge status before configuring RMK's existing `ChargingStateReader`. Leave them inputs/released; do not drive them push-pull or infer charging from USB attachment alone. If framework ADC gating is generalized later, move only the sampling-enable hook upstream, preserving RMK's existing battery processing/service.

Backlight P0.20 is published on both halves, but physical active polarity is unverified. The initial feature should offer on/off and brightness steps, with no animation engine. RMK already defines light actions but does not execute them at the pinned commit. Add a small framework processor for these actions and an explicit brightness state message using the existing split transport; resend current state on reconnect. Do not overload `LedState(bool)` or host lock indicators to represent brightness, and do not embed lighting controls in a second board-local keymap handler. Only after that framework work and review should an electrical PWM adapter be enabled for the board. Persistence and sleep dimming should use framework storage/sleep events, if added, rather than another flash store or timer policy.

Exit: both battery readings are calibrated within a recorded tolerance; safe charging indications work; both halves follow brightness controls across reconnects; current and PWM behaviour are measured.

### 6. Optimize power after responsiveness is established

Start from the current awake baseline and measure active/idle current and press-to-host latency. PCA9555 interrupt mappings are left P0.31/right P0.05. An interrupt-assisted scanner may wait only when all keys are released and debounce is settled; it needs periodic reconciliation and active scans around changes. Keep initialization, input polarity and I2C-failure semantics unchanged. Test a held interrupt, missing edge, bus failure and first key after idle before accepting the optimization.

Continue to use RMK sleep events/radio policy. Do not enable BLE subrating or long sleep intervals merely to improve a battery estimate; upstream comments show meaningful wake-delay tradeoffs. Test latency and current for each proposed setting and accept only settings meeting the agreed input budget.

The right OFF observation with USB attached does not prove a faulty switch. Firmware cannot repair mechanical contacts or disconnect a physical USB power path. Any software-off feature requires a verified readable control or power-enable circuit; neither is currently established for the right half.

Exit: measured battery-life/power improvement without failing the wake-latency and release-recovery gates.

## Repeatable checkpoints

For each scanner or transport change, run the host harness and cross-build all three roles before and after the change. Record the active layout, image guard, flash/RAM sizes and artifact hashes; never mask the intentionally failing protected-layout left size gate as a passing build. Give independent implementation/review agents separate file ownership, integrate only reviewed results, and commit concise source checkpoints without device identifiers, factory images or generated binaries.

Hardware acceptance follows [acceptance.md](acceptance.md): controlled input counts in all three modes, simultaneous input, six-key ordinary rollover limits, disconnect/release recovery, wake latency, RF interference, native macOS/Windows/Linux behaviour and power measurement. A release requires explicit per-feature results. No unattended cross-build or mock test changes an untested entry into a hardware pass.
