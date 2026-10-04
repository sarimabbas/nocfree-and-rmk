# Companion state-machine audit

The UI projects workflow states. Evidence (USB identity, archived bytes, device bindings,
request generations) is separate from navigation and never inferred from the screen.

## Active flows

| Flow | Authoritative model | Transitions |
| --- | --- | --- |
| Backup orchestration | `backup_flow::Machine` and `Event` | Choose → Guiding → Recovering → Guiding → Saving → Returning → Complete. RecoveryFailed and Failed have explicit retry paths; navigation pauses a cancellable flow. |
| Backup evidence | `journey::Journey`, `session::Session` | Guiding → ReadyToSave → Returning → Complete; selection binds exactly one left half, right half or dongle. Paused/Failed retain saved archives; no other part is added automatically. |
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

## Statig implementation

Companion pins [Statig](https://github.com/mdeloof/statig) 0.4.1. The library owns
current states for navigation, backup orchestration, backup evidence reconciliation,
recovery attempts, physical return, pairing and installation. Public enums are
read-only projections; none is a separately writable copy of the machine state.

State handlers consume explicit events and return transitions. Time is supplied in
events. USB/HID requests, filesystem operations and cancellation of native permits
remain outside handlers. Device observations and archived-byte evidence are inputs,
not screen flags. This makes decisions deterministic for a given event sequence;
Statig's engine mutates its state internally, rather than making the entire app immutable.

`operation::Operation` owns Idle, Running and Failed states. Running holds a unique
ticket, operation kind, selected role and, while the installer is owned by its worker,
an immutable presentation snapshot. Completion must match the ticket before the UI
adopts a result. Duplicate or old completion cannot clear a newer job. Recovery also
requires its attempt generation and selected role; pairing retains native one-shot
permits and fresh correlated observations.

`ui::Companion` no longer stores separate `view`, `firmware_view`, `backup_component`,
`busy`, `message` or page flags. Render reads the current journey or running-operation
presentation. Backup saving progress derives from the Saving state while its worker
owns the journey. Each role's battery/mode/USB facts retain their observation caches;
the status strip projects these facts independently of the navigation machine.

The backup orchestration and evidence machines are composed deliberately: the first
owns recovery-worker and save lifecycle, while the second owns eligibility established
by Session's observations. A recovery result must satisfy both before saving starts.
Neither UI navigation nor a progress indicator grants write eligibility.

This removes duplicated workflow/view authority and rejects invalid events; it cannot
promise freedom from every possible stateful failure. Faulty observations or a missing
event still need tests and clear error handling. Factory restoration remains unavailable,
and the private legacy `trial.rs` controller is outside the shipped UI.

## Journey entry audit

Every visible journey enters Navigation's Setup state. Sidebar navigation produces
no journey-start effect. Backup and recovery cards update a draft selection only;
Next consumes that selection once. Pairing and installation likewise need Next
before their worker or release preparation can start. Duplicate Next in Active is
ignored. Setup has no Cancel button or workflow progress stepper.

Normal status discovery, battery and version polling remain independent of journey
entry. Firmware observation/automatic advancement and pairing worker observations
require Active, so opening their setup pages cannot adopt old results or start work.
Leaving a cancellable journey invalidates its workers. Cancel/finish reset setup;
clicking the already selected sidebar page is a no-op. A paused backup retains its
archive evidence, but requires a fresh selection and Next before resuming.

Factory restoration remains disabled and cannot start. This audit covers all shipped
Companion navigation paths, including install/update, backup, recovery and pairing.

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

## Explicit backup selection

Backup and Recovery share the same illustrated peripheral picker. Selection starts a
single-part journey; Left half, Right half and USB dongle are explicit model values.
Returning to Backup shows the picker. Choosing the same paused component resumes its
remaining state; choosing another starts a fresh journey while existing archives remain
on disk. A dongle archive and its unplug/reconnect guide use the dongle role throughout,
never a fallback left-half label.
