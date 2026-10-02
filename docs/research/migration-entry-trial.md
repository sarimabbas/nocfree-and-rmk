# Left entry-probe controlled trial

Owner approved the left-only entry probe and complete factory restoration on 2026-10-01. Candidate checkpoint `3dcdd69`; [exact artifact bounds](migration-entry-candidate.md). This trial did not install typing or radio firmware. The right Mac diagnostic remained present; the receiver was untouched.

## Observed results

| Gate | Observation |
|---|---|
| Device preflight | Normal factory left at its known USB location; owner Fn+5 entry; original complete readable factory hash matched before transfer |
| Transfer/readback | Addressed UF2 transfer completed; exact BIN and final-page FF padding; readable bytes above `0x5000` unchanged |
| Immediate USB state | Bootloader drive appeared; not counted as application-entry proof |
| Independent recovery | Middle WIRED, USB removed five seconds, reconnect WIRED; stable bootloader and unchanged candidate readback |
| Prelaunch USB absence | First reported unplug still showed serial/MSC; corrected physical unplug was verified absent before proceeding |
| Battery-first observation | With USB absent, owner switched left to Bluetooth, waited ten seconds, then attached USB; bootloader drive was present and readable |
| Battery-first readback | Exact candidate and retained readable bytes still match; no S140 reported |
| Factory restore | Complete original container copied; normal factory `NocFree & ANSI` enumerated |
| Complete factory readback | Owner Fn+5; entire original container SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100` matched; S140 7.3.0 present |

All candidate readbacks had SHA-256 `cee60d730bda823935100b57d17bd929c7c20e56e13c016a328ac5e9a883d679`. Private images, identifiers and logs remain under ignored `.evidence/left-entry-trial/`.

## Interpretation

The late bootloader appearance is consistent with execution of the early `__pre_init` hook, which requests DFU and resets before runtime RAM/clock/USB initialization. The earlier full USB probe did not enumerate under a similar battery-first sequence. This is useful comparative evidence, but does not prove hook execution: unreadable application-bank metadata, vendor bootloader behavior and physical power-state ambiguity are not independently excluded. The initial failed unplug check is retained rather than counted as a valid launch.

Do not promote this result into validation of HAL clocks, executor interrupts, USB runtime, or BLE. The original startup failure remains undiagnosed. A separately reviewed runtime/HAL stage-return candidate would further narrow it; this approval does not include that image or an automatic next-stage transfer.

Recovery and exact restoration of the complete readable S140/application/settings region passed again. Excluded MBR, filesystem, bootloader and UICR contents cannot be read back through CURRENT.UF2; address guards protect those regions. No deliberate interrupted-update test or automatic rollback was performed.

Final normal factory startup passed: owner left the switch in middle WIRED, removed USB for five seconds and reconnected. Normal `NocFree & ANSI` enumerated, MSC was absent, and the right diagnostic remained present. The left remains restored factory firmware. No entry-probe reinstallation occurred.
