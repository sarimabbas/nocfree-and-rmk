# Companion transition guards

Automatic successful journey steps use `CompletionGate`, with a shared five-second default. Each observation must still satisfy the step prerequisites. Losing readiness, changing the bound part or step, or losing fresh evidence resets the pending completion. The gate uses an injected monotonic clock; it does not block the UI thread.

| Machine / boundary | Completion prerequisite |
| --- | --- |
| Firmware install / restore return | Normal firmware at the bound USB location; RMK left additionally reports Wired mode. Factory left requires explicit Wired confirmation after normal USB return. |
| Backup return | The same guarded return, followed by the saved-copy completion boundary. |
| Recovery worker | Exactly one validated, correlated recovery drive; continued drive observations through the completion interval. Runtime request dispatch also requires stable target presence. |
| Scope identification | Stable detach, then exactly one matching newly attached candidate. Stale inventory resets completion. |
| Peripheral batch | Observed detach of the current part before offering the next part. |
| Check pairing | Stable readiness before dispatch, stable completed links before success. Whole-keyboard checks require a dongle. |
| Installation mode tests | Fresh evidence of the requested mode, USB power arrangement and links for five seconds. Matching typed text enables the manual Next button immediately while prerequisites remain satisfied. |

Navigation, scope selection, explicit Next, cancellation, failures, operation permits and effect callbacks remain immediate. These are control events rather than automatically completed physical steps. Delaying them could retain a cancelled effect or permit duplicate work. Statig state remains the authoritative source for rendering.

A verified image observed running normally on its bound USB port can satisfy return without another physical power cycle. Restart instructions are conditional on its recovery drive remaining open. USB presence alone still cannot satisfy the left's Wired-mode requirement. Satisfied instructions display a green check derived from the machine's readiness or completed state.

Regression tests use synthetic observations and time. They cover wrong/unknown switch mode, readiness loss, stale evidence, changed identity, cancelled work and late callbacks. Host tests and a successful desktop build are not hardware acceptance. Repeat the owner's adversarial Wired-to-Dongle test in the rebuilt app before accepting the live journey.
