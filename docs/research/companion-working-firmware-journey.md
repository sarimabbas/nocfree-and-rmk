# Companion journey for the working firmware

Implementation plan, 2026-10-02. This records the smallest next integration; it does not enable installation or declare the existing read-only app a finished installer.

## Start with an existing RMK keyboard

Keep the two clearly named journeys: **Back up & restore** and **Install RMK**. An existing RMK keyboard should automatically see **Update RMK**, with “Your current firmware will be saved first.” A factory keyboard sees **Install RMK**, with “Your factory firmware will be saved first.” A current readback must never be relabeled a factory backup. Keep installation policy and diagnostics behind Details; the main screen shows one pictured component, one physical action and its result.

The working keyboard now has macOS evidence for USB, direct Bluetooth, the split link, shared modifiers and the original dongle. Both halves have owner-observed brightness steps, hold-to-repeat, release stopping, right restart synchronization and left restart persistence. These observations do not establish Windows/Linux acceptance, measured input latency, release recovery after radio loss or newcomer installation. Battery integration is a separate checkpoint; the root has inspected its current app display, while battery calibration and reporting acceptance remain separate from installation readiness.

The smallest useful prerequisite is already implemented in this patch:

- Recognize the exact full-left normal USB tuple `4c4b:4643`, product `NocFree RMK`. The receiver's different product string remains excluded despite sharing the numeric IDs.
- Show **Hold Fn and tap Escape** for that firmware. Factory left retains **Hold Fn + 5**.
- After saving an RMK left copy, guide its actual battery-first return: WIRED and unplug; observe five seconds absent; switch to Bluetooth while unplugged and acknowledge; wait ten seconds; reconnect. Cable observations advance automatically. Only the unobservable switch action needs an acknowledgment.
- Keep the developer trial's factory evidence predicate explicit, so recognizing RMK does not accidentally attest factory restoration.

These changes remain read-only. The existing app can save an identified full-left copy with the correct shortcut and return instructions. They do not solve full-right identification or activate an installer.

## One finite installation session

Reuse `Session` for the user-facing progression and the current off-UI observation/file work. Add one bounded installation service behind it, rather than extending the private developer trial protocol into a generic workflow engine. Keep GPUI/gpuikit responsible for presentation and acknowledgments; the service owns image policy, transfer and reconciliation.

| Phase | Automatic work | Unavoidable user action |
| --- | --- | --- |
| Identify | Bind a supported normal device to its USB connection; reject ambiguity | Connect the pictured component; remove another ambiguous component if necessary |
| Save | Read bootloader metadata and current image; check stable identity before/after; save private files and hashes | Enter recovery using the firmware-specific shortcut |
| Ready | Validate exact bundled role/layout/version/protocol manifest, image guard, recovery evidence and retained ranges | Approve the concrete update once |
| Copy | Recheck identity; write the one approved image once; flush; journal the outcome | Keep connected |
| Verify | Compare actual readback with the image and check untouched ranges | Follow the applicable cold-start/recovery cable step |
| Start | Observe disappearance and running-device return; retain the saved rollback | Move the physical switch where required; reconnect when prompted |
| Finished | Report the verified version and save the local checkpoint | Brief typing check when required for this release |

A successful host copy is not a successful installation. An uncertain transfer must stop at reconciliation, with no automatic retry. An app restart returns to observation and reconciliation rather than replaying the write. Countdowns begin from observed USB absence; they cannot certify battery power or switch position. No dock power control is part of this plan.

The current Home card labeled Install RMK still routes to a developer startup test, whose finish copy describes factory restoration. Do not expose that as the production installation journey by default. Keep the developer guide separately named and hidden from ordinary onboarding; no writer is being enabled in this checkpoint.

Replace indefinite “Checking” with a bounded service operation and its actual result. USB polling can remain once per second initially. When the app has the expected evidence it advances in-process, without a Codex heartbeat, chat callback or a typed “done.” If evidence is missing, show the single needed cable/switch action; if verification fails, preserve the image and explain the concrete next step. Details can expose the journal and diagnostic reason.

## Reuse the safety policy deliberately

The app's readback parser checks archive coverage. It is not an installation guard. Reuse the canonical policies in `scripts/image_guard.py` and `scripts/migration_guard.py` through a packaged validator, or deliberately replace them with a Rust implementation proven equivalent against the existing valid and malformed fixtures. Do not duplicate rules in button handlers.

The local trial operators provide reusable behavioral requirements: fresh stable readbacks, an exact approved image binding, one-shot transfer, exact installed-byte comparison and unchanged bytes outside the permitted write range. Their private owner identifiers, approval files and factory images are not distribution inputs. Extract the policy into a small service with a pinned manifest; do not make the app invoke arbitrary private trial scripts.

Ship only a reviewed artifact set first. Its manifest names each role, layout, flash policy, image hash, framework revision and split protocol compatibility. Keep the old image privately for rollback. RMK feature/revision changes can change the storage schema and reset bonds/settings even when the copy leaves storage addresses untouched. Disclose re-pairing when that applies; do not bypass RMK's schema checks or restore incompatible settings silently. An unchanged compatible receiver need not be reflashed for a half-only lighting or battery update.

## Distinct routes and remaining blockers

| Route | What exists | What still blocks a newcomer-ready journey |
| --- | --- | --- |
| Existing RMK left update | Exact normal USB identity, Fn+Escape, saved readbacks and owner-verified USB-first recovery/battery-first startup | Bundled approved images, installation service, durable reconciliation journal and an app-driven approved hardware trial |
| Existing full RMK right update | Working BLE peripheral; owner-verified manual recovery/startup sequence | Full right firmware has no normal USB HID identity. Establish a supported role-binding/runtime-evidence route; a shared bootloader alone cannot identify it |
| Fresh factory left | Exact ANSI normal identity, factory Fn+5 and a successful read-only copy flow | General supported factory-image/layout policy and the independently applicable migration/recovery gate |
| Fresh factory right | Earlier owner-specific recovery/readback evidence | Companion does not recognize factory right normal mode. Add and test its supported identity and correlation path before offering installation |
| Factory restoration | Private owner images were restored and checked during bring-up | Current migration restore guard binds one exact owner factory hash. Generalize supported restore eligibility deliberately; arbitrary archives are not proven restore images |
| Receiver update | Working dongle, protected application image policy and owner-specific DFU/replug trial | No proven closed-case recovery independent of a broken application. The owner's risk exception is not a public recovery guarantee; keep receiver installation unavailable until its supported policy is settled |

Do not quietly treat the current RMK right as the earlier USB diagnostic. Do not infer a half from bootloader VID/PID, volume name or silicon family. Current archives exclude MBR, filesystem, bootloader and UICR; say “saved firmware copy,” never “full-chip backup.”

## Small implementation checkpoints

1. Land the read-only full-left recognition, shortcut and return changes, with collision and transition tests. Have the owner run the existing copy flow on the working left firmware. This is useful immediately and requires no firmware write.
2. Introduce the finite service and local journal for one exact macOS existing-left update. Use fixture-backed guards and transfer-failure/restart tests before making the approval action available. Independently review the policy, then run one approved device trial.
3. Resolve full-right role binding, add its explicit recovery/startup plan, and package a compatible pair update. Preserve the known working dongle when protocol compatibility permits.
4. Add general factory migration and eligible restoration as separate routes. Keep unsupported board/firmware states blocked with a simple explanation rather than asking newcomers to choose flash addresses or firmware internals.
5. Package an ordinary offline-capable download with embedded assets/validator and signing/notarization. Add Windows/Linux discovery, mounted-volume and transfer adapters, then test actual hardware journeys on each OS. Rendering-library platform support alone is insufficient.

The current bundle is a macOS development app, not a notarized portable release. `desktop/README.md` contains historical prototype/heartbeat statements; the deleted development heartbeat is not a production dependency. Avoid showing those implementation details in the normal journey.

## Local source evidence

- [Device discovery and identity](../../desktop/src/device.rs), [copy/return session](../../desktop/src/session.rs), [read-only developer trial](../../desktop/src/trial.rs), [GPUI views and polling](../../desktop/src/ui.rs).
- [Protected image guard](../../scripts/image_guard.py), [reclaimed-layout and restore guard](../../scripts/migration_guard.py).
- [Firmware role construction](../../firmware/src/main.rs), [lighting implementation and observations](backlight-implementation.md), [hold validation](backlight-hold-validation.md).
- [Earlier installer architecture](guided-installer-app.md), [newcomer route research](newcomer-installation.md), [prototype limitations](../../desktop/README.md).

Host tests and cross-builds remain software evidence. The next installation checkpoint still needs its own device-specific approval and hardware verification.
