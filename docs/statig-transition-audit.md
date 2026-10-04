# Companion transition guards

Visible successful journey stages advance only after an explicit **Next** event. Observations update authoritative readiness and pending results; they do not advance the journey. Readiness depends on the current role, bound USB location, fresh telemetry, and the stage's prerequisites. Missing or changed evidence invalidates readiness. There is no artificial completion delay.

| Machine / boundary | Next prerequisite |
| --- | --- |
| Firmware install / restore return | Normal firmware at the bound USB location; RMK left additionally reports Wired mode. Factory left requires explicit Wired confirmation after normal USB return. |
| Backup return | The same guarded return; saved-copy and return completion are adopted through Next. |
| Recovery worker | One validated, correlated recovery drive is recorded as a pending result. Next adopts it after current evidence is checked. Runtime dispatch occurs only within the explicitly started recovery step. |
| Scope identification | Fresh absence before Next offers connection; then one matching, unique attached candidate before Next records identity. |
| Peripheral batch | Fresh absence of the current recovery drive before Next starts the next part. |
| Check pairing | Fresh link observations queue the next guidance stage; Next adopts it. Repair writes require explicit Next from Ready. Whole-keyboard checks require a dongle. |
| Installation mode tests | Fresh evidence of the requested mode, USB power arrangement and links enables typing. The exact typed token enables Next; it does not advance automatically. |

Navigation, selections, cancellation, errors and asynchronous operation bookkeeping remain immediate. These events do not authorize further device work by themselves. A successful effect updates readiness for the next explicit action. Statig state remains authoritative for rendering; callbacks must carry the current operation ticket or generation.

Power-off/startup waits required by the hardware procedure remain part of their physical steps. Time passing only enables Next. It does not advance to another instruction automatically.

A verified image observed running normally on its bound USB port can satisfy return without another physical power cycle. Restart instructions are conditional on its recovery drive remaining open. USB presence alone still cannot satisfy the left's Wired-mode requirement. Satisfied instructions display a green check derived from the machine's readiness or completed state.

Transfer records distinguish writing from flushing the recovery volume. A completed write proceeds to readback reconciliation even when flushing fails; a write error pauses for explicit verification. `transfer-outcome.json` retains the stage and OS error locally. Neither outcome establishes installation success or permits repeating the copy. Exact readback remains mandatory.

Regression tests use synthetic observations and time. They cover wrong/unknown switch mode, readiness loss, stale evidence, changed identity, cancelled work and late callbacks. Host tests and a successful desktop build are not hardware acceptance. Repeat the owner's adversarial Wired-to-Dongle test in the rebuilt app before accepting the live journey.
