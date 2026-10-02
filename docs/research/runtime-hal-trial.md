# Left runtime and HAL trial

Observed on the owner's left ANSI half, 2026-10-01, under explicit approval for the paired probes and original factory restoration. Source checkpoint `e2e303c`; exact candidates and software checks are recorded in [runtime/HAL candidates](runtime-hal-candidates.md). The right Mac USB diagnostic and factory receiver were unchanged.

## Observations

The initial normal factory identity, correlated bootloader and complete original readable image were verified before installing either candidate. Both guarded images touch only pages `0x1000..0x4000`; exact binary bytes, FF page tails and unchanged readable bytes above `0x4000` passed after each transfer and physical recovery check.

| Stage | Exact installation/readback | Independent WIRED USB-first recovery | Controlled battery-first observation |
| --- | --- | --- | --- |
| Runtime: first async main poll, before HAL configuration | Passed | Passed | Bootloader/MSC reappeared; exact readback passed |
| HAL: immediately after `embassy_nrf::init`, before USB driver construction | Passed | Passed | Bootloader/MSC reappeared; exact readback passed |

For each battery-first sequence, the owner disconnected USB in middle WIRED; the host confirmed disappearance of the correlated USB device and MSC before the owner moved to Bluetooth, waited ten seconds unplugged and reattached USB. An initial runtime disconnect attempt still showed USB/MSC present. That attempt was not accepted as a launch sequence; a separate confirmed disappearance preceded the recorded observation.

Runtime full readable readback SHA-256 was `49372c49415a6b10e3527ed7bbfd80c9874955dbd0ffa2373fc9d6d37eb44059`; HAL was `1755fdad4cb4aeedf55e7a3df5f6ef7d43e682c56756c5e05151829212d05343`. The runtime observation and a fresh exact runtime readback gated the HAL transfer. Bootloader identity/location correlation was rechecked immediately before each transfer and every readback. Private identifiers and factory bytes remain outside Git.

The complete original factory container was restored. Factory USB enumeration returned, and a fresh owner-directed Fn+5 readback matched the original SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100` exactly, with S140 7.3.0 metadata restored. This verifies the complete exposed readable image, not excluded MBR/filesystem/bootloader/UICR regions. Final owner-directed WIRED unplug/reconnect returned to normal factory enumeration with MSC absent; the right diagnostic remained present. Final state: left factory, right Mac USB diagnostic, receiver untouched.

## Interpretation and next boundary

Both observations are consistent with reaching their respective reset-to-DFU boundaries. They do not independently prove hook/task execution: bank metadata inaccessible through CURRENT.UF2, marker-driven DFU, vendor bootloader behavior and incomplete battery power transitions remain alternatives. Do not interpret these results as a proven clock diagnosis or successful full application startup.

The full lower-layout USB probe previously remained silent, while these smaller stage-return probes reappeared in recovery. USB driver construction, run/wakeup and enumeration are the next useful boundaries to investigate in software. No additional diagnostic or complete RMK image is approved by this trial. Split/BLE/receiver operation, input loss, wake latency and power-loss update safety remain unvalidated.

Independent review verified the helper gates and reconstructed the saved runtime/HAL readbacks against linked binaries and retained ranges. Host tests and cross-builds remain software verification; the owner-assisted observations above are the hardware evidence.
