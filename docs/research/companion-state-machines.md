# Companion state-machine audit

The UI projects workflow states. Evidence (USB identity, archived bytes, device bindings,
request generations) is separate from navigation and never inferred from the screen.

## Active flows

| Flow | Authoritative model | Transitions |
| --- | --- | --- |
| Backup orchestration | `backup_flow::State` and `Event` | Intro → Guiding → Recovering → Guiding → Saving → Returning → Complete. RecoveryFailed and Failed have explicit retry paths; navigation pauses a cancellable flow. |
| Backup evidence | `journey::Journey`, `session::Session` | Guiding → ReadyToSave → Returning → Complete; role advances only after the previous part returns. Paused/Failed retain saved archives. The current backup plan covers halves, not the dongle. |
| Recovery prerequisite and utility | `recovery_journey::RecoveryJourney` | Choose → Identify → Guiding → Ready or Failed. Attempt generations reject late, wrong-role and cancelled callbacks. |
| RMK install/update | `firmware_journey::FirmwareJourney` | Recovery → Approval → Reconcile → physical return states → next role/Complete. One-shot transfer and durable intent/readback records prevent replay after restart. Discovery failure enters Failed; successful read-only observation resumes without another transfer. |
| Check pairing | `dongle_pairing::Journey` | Connect/TurnOnRight/SwitchMode → Ready → Pairing → Connected or Failed. Existing valid links complete without mutation. Explicit repair binds both selected USB peers and their radio identities. |

Backup UI no longer carries independent started, completed, stopped and recovery flags.
Its reducer rejects invalid events, including a recovery result after cancellation and
navigation while saving. Cancellation signals the recovery worker before its state is
paused. Native asynchronous operations retain their generation/identity checks.

The session's evidence facts are not independent user journeys: can-save and return
readiness are derived from validated observations. Firmware and pairing views likewise
project their model rather than letting buttons grant eligibility.

Factory restoration is not an operational shipped state machine yet; the disabled
navigation item must not be described as an implemented restore workflow. The legacy
`trial.rs` controller is not connected to the application's shipped navigation.

## Reopen and failure behavior

Backup can read a single metadata-validated recovery drive already mounted when the app
starts. An archive adopted without normal-device correlation is marked role-unverified
and cannot grant firmware-write eligibility. A previously identified other part is
rejected rather than silently labelled as the selected part. Firmware installation keeps
the stricter normal-device/port correlation requirement.

PermissionDenied and macOS EPERM reading recovery metadata are failures, not absence.
The app directs the user to Files & Folders → Removable Volumes. It cannot establish
that the owner's Full Disk Access change has been applied without a successful read.
A bootloader present without its mounted drive has a finite observation deadline.
USB requests distinguish failures before submission from uncertain outcomes after
submission; uncertain outcomes reconcile against drive appearance without resending.

## Library decision

Rust enums and exhaustive event matching are sufficient for these bounded workflows.
No additional state-machine runtime is needed to make the transitions authoritative.
[Statig](https://github.com/mdeloof/statig) supports hierarchical machines and
[smlang](https://github.com/korken89/smlang-rs) offers a declarative transition DSL.
Neither would repair duplicate UI flags, missing filesystem errors, or lost device
correlation merely by adding a dependency. Reconsider one if nested reusable machines
make the explicit reducers harder to inspect than a transition table.

## Validation boundary

Host regression tests cover mounted-drive adoption, ambiguous/mismatched evidence,
permission errors, cancellation, stale callbacks and one-shot firmware reconciliation.
A passing host test is not evidence of macOS permission restoration or hardware recovery.
Live app and device checks must be recorded separately.

## Re-requesting removable-volume access

Apple documents `tccutil reset SystemPolicyRemovableVolumes <bundleID>` to clear the
previous choice, after which the next protected file access can request consent again.
For this app the bundle ID is `io.github.sarimabbas.nocfree-companion`. Reopen the app
then explicitly start Backup. Do not reset all privacy services or automatically erase
a denial while polling. Users can also change the existing Removable Volumes toggle
in Files and Folders. Full Disk Access may already permit access without another prompt.

The bundle includes `NSRemovableVolumesUsageDescription` explaining that the recovery
drive is used for backups and firmware installation. Current live validation reopened
the app with LEFT already in recovery, saved its archive, and advanced to Returning.
That proves access worked on this Mac during that check; it does not prove a newly
reset permission will display a prompt under every existing privacy configuration.

Primary sources: [permission prompt purpose key](https://developer.apple.com/documentation/bundleresources/information-property-list/nsremovablevolumesusagedescription),
[resetting protected-resource access](https://developer.apple.com/documentation/xcode/resetting-access-to-protected-resources-in-macos).
