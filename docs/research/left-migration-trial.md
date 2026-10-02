# Left lower-layout USB trial

Owner explicitly approved this controlled left-only trial on 2026-10-01. Candidate source checkpoint `1121d39`, [candidate bounds and hashes](left-migration-candidate.md). This was a USB recovery diagnostic, not RMK keyboard or split firmware. The right Mac input diagnostic remained enumerated; the receiver was not touched.

## Observed hardware results

| Gate | Observation |
|---|---|
| Connected-role preflight | Left Mac diagnostic identity and exact CDC greeting; correlated transition to unique vendor bootloader; full readable flash matched the prior verified left image |
| Candidate transfer | Addressed application-family UF2 copied with complete host write/flush/fsync; no automatic retry |
| Immediate enumeration | Neither left probe nor left bootloader enumerated after transfer; application launch not proven |
| Independent recovery | Owner moved left to middle WIRED, removed USB for five seconds, reconnected in WIRED; existing bootloader returned without an application command |
| Candidate readback | Exact BIN and final-page FF padding at `0x1000..0x5000`; all readable bytes above `0x5000` matched pretrial |
| Bootloader metadata | Same reported bootloader version/model/date; `SoftDevice: not found` |
| Battery-first startup | Owner removed USB in WIRED for five seconds, switched to Bluetooth while unplugged, waited ten seconds, reconnected USB; neither probe nor MSC enumerated; no greeting obtained |
| Recovery after startup failure | Same independent WIRED USB-first sequence returned bootloader; exact candidate readback unchanged |
| Factory restoration | Original saved left UF2 transferred through MSC; normal `NocFree & ANSI` factory USB identity returned |
| Full factory readback | Owner entered factory Fn+5 recovery; complete `CURRENT.UF2` equals saved original SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`; bootloader reports S140 7.3.0 again |

Candidate readback SHA-256 was `021e2610d089785d86786dd8cead2ce1dd78a818b531a09d3803945e5941f325`. Private images, device correlation and transfer logs remain under ignored `.evidence/left-migration/`.

## Interpretation and next gate

The existing vendor bootloader remains recoverable after the lower-address write, and restoration of the readable S140/application/settings region now has exact hardware evidence. This does not validate the lower-layout application's startup, interrupt forwarding, USB runtime, arbitrary interrupted updates or any radio behavior. No MBR, bootloader, filesystem or UICR readback is available; candidate/restore address bounds protect those excluded regions.

The failed startup is an observation, not a diagnosed cause. Switching/reset behavior, the vendor lower-layout launch branch and application interrupt/clock initialization remain distinguishable possibilities. Do not repeat the same candidate or install the full RMK central to see whether it helps. Review the lower-layout prerequisites, prepare a focused candidate with observable startup stages if justified, and obtain approval for that distinct hardware trial.

Final normal factory startup passed: after the owner left the switch in middle WIRED, unplugged USB for five seconds and reconnected, `NocFree & ANSI` enumerated and MSC was absent. The right diagnostic remained present. The left is factory firmware again; no migration diagnostic reinstallation or further candidate transfer occurred.
