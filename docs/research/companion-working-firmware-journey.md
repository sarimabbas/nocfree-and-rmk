# Companion journey for the working firmware

Implementation plan, 2026-10-02. This records the smallest next integration; it does not enable installation or declare the existing read-only app a finished installer.

## Three explicit journeys

Owner-directed UX, 2026-10-02: there are three supported intents, but the home screen shows only the one that applies. Do not ask a newcomer to decide whether their keyboard needs installation or an update.

- No recognized keyboard: **Connect your keyboard**, with one cable instruction.
- Factory firmware: **Switch to RMK**.
- RMK with a verified newer compatible port release: **Update RMK**.
- RMK with a verified matching current release: **You’re up to date**, with neither installation nor update action.
- RMK without sufficient release information: **Your keyboard is running RMK**, with neither installation nor update action and no claim that it is current.
- Recovery mode: show a recovery-specific status without inferring which half is connected.

**Go back to factory** belongs under **More options** only when RMK is recognized. Saving firmware copies and developer tools are secondary options. The main view uses one paired-board illustration, one headline, short supporting copy and at most one primary action. Use GPUI Kit components and restrained native motion; keep release internals, backup eligibility rules and diagnostics out of the everyday view.

Current normal USB descriptors identify factory versus RMK, not the exact NocFree firmware release. Rynk's `GetVersion` is the wire protocol version; `GetDeviceInfo.rmk_version` is the RMK library version. Neither may stand in for a port release/version or justify “up to date.” Production release assessment stays unknown until an installed port identity and compatible release catalog are available. Ambiguous or failed discovery clears the inferred home state; the receiver's shared VID/PID does not identify it as the left. These read-only UI observations do not grant installation authorization.

| Journey | Plain-language promise | Preparation | Final confirmation |
| --- | --- | --- | --- |
| **Install RMK** | Replace factory firmware with RMK | Identify the keyboard and applicable recovery routes; save and verify eligible factory copies; validate the bundled RMK release for the required components | Show the pictured components, target release and the saved factory recovery location; one explicit Install action |
| **Restore factory firmware** | Return to the original firmware | Prefer an eligible factory backup from this keyboard; otherwise accept separate user-provided Left, Right and Dongle UF2 files and validate each required component | Show the pictured components, the verified source for each and any missing requirement; one explicit Restore action |
| **Update RMK** | Move an existing RMK keyboard to a newer release | Identify the installed release; save current RMK for rollback; validate the new release and split/receiver compatibility; update only required components | Show old → new release, affected components, rollback copy and whether pairing will reset; one explicit Update action |

No official firmware bytes are bundled, published, or downloaded automatically by the app. Official firmware must come from the owner's eligible backup or files they supply. RMK artifacts may be bundled with their pinned release manifest and compatible role metadata. Keeping old images on the host is a recovery copy, not automatic onboard rollback.

Factory restoration starts with one recommended source: **Use your saved factory backup** when an eligible matching archive exists. Otherwise it opens **Provide factory firmware**, with three pictured drop spots labelled **Left half**, **Right half**, and **USB dongle**. Keep these source options within the restoration journey, not as another top-level decision. Also allow a file picker so dragging is optional. A component already running verified factory firmware can be skipped explicitly; required files must not be silently omitted.

A drop spot accepts a UF2 for inspection, not authorization to copy it. Validate container coverage, addresses, vector, family, supported factory release/role/layout metadata, hashes and device/recovery binding through the canonical policies. A filename, user-selected slot, shared bootloader product or nRF family alone cannot establish compatibility. Wrong or unsupported files remain unconfirmed; explain the correction beside the pictured slot. The final source summary cross-checks each accepted file with the actual component immediately before the one-shot transfer.

A backup is offered as factory restoration only when its provenance and bytes establish an eligible factory image for this device. A copy taken after RMK installation is **Previous RMK firmware**, never a factory backup. Some CURRENT.UF2 archives omit chip regions; validate that the archive contains all required restore ranges, including S140 where the migration replaced it. Never imply that an arbitrary saved UF2 is a full-chip or proven factory restore image. Restore policy remains device- and layout-specific.

Inside each journey, automatically advance through **Connect → Prepare recovery copy → Confirm → Install/Restore/Update → Verify → Done**. Show one pictured component and one physical instruction at a time, advance from fresh observable USB/mount/readback state, and ask for acknowledgment only for unobservable switch actions or the final consequential action. Preserve exact readback and applicable recovery gates. Fold technical diagnostics behind Details; never expose flash addresses or developer trial sequences as normal decisions. Reconcile an interrupted write before continuing, never automatically replay it.

The current implementation selects the relevant intent from fresh device observations and keeps unsupported write actions disabled. Saving firmware copies remains a secondary read-only tool. The developer startup test is separately named and does not masquerade as installation or restoration. The following implementation checkpoints are required before these journeys can actually transfer firmware.

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

The home screen now shows only the relevant primary journey. Restoration, firmware copies and the read-only developer guide are secondary options; no writer is being enabled in this checkpoint.

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

### Home composition

Treat the home as an accessory overview, not a welcome page or a dashboard. At the 640 × 580 default window, a 460px column with 24px horizontal insets holds a small connection header, paired-board artwork, a 20px status and 15px supporting copy. The surface is white, foreground #23262b, secondary text #626870 and interaction accent #007aff. Ordinary secondary rows are 44px high; they expand below a separator. Developer tools are disclosed separately. The 400ms one-shot artwork reveal respects GPUI reduced motion.

GPUKit supplies separators, icons and primary buttons. Its virtualized List is unnecessary for these few rows; its current Collapsible does not provide the keyboard/accessibility behavior needed here, so the small disclosure uses explicit GPUI semantics. This component choice keeps the layout simple while preserving named controls and visible focus.
