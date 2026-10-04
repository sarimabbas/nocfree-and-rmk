# Companion desktop design

The user's supplied sidebar wireframe defines the composition. Preserve the existing accessory identity while replacing the large centered wizard layout with a conventional native application shell.

## Surface

Operate: a newcomer connects a keyboard, sees its state, and follows one clear next action. A fixed approximately 220 px sidebar provides Backup firmware, Enter recovery mode and Check pairing navigation, with the relevant firmware task only when supported by evidence. Sidebar items use the component’s natural padding with a 6 px gap between items. Hide Tasks when there is no available task. Keyboard status and observed firmware live in a persistent bottom status bar. Development trials stay out of navigation.

## Visual system

Use the system sans-serif, compact native type sizes, a pale neutral sidebar, a white content surface, restrained blue selected states and primary buttons. Follow system appearance automatically at launch and when it changes; use GPUI Kit theme roles for readable surfaces and text in both modes. Journey action buttons use a consistent 40 px control scale and natural widths. A separate bottom action row spans the right pane: Cancel on the left and Next on the right. Use consistent 8 px spacing increments and approximately 28–32 px content insets. Use GPUI Kit selection cards for parallel choices, with authentic component drawings and one clear action per card. Preserve the existing accurate left/right SVG assets. Avoid large hero headlines, decorative badges, ambiguous progress indicators, and blocks of technical disclosure.

## Journey canvas

The same content region centers each journey within the right pane while its action row remains anchored below the scrolling content: operation title, active component artwork, one instruction, observed progress, and a primary action only when human action is required. Waiting advances automatically. Cancel returns safely; retry retains a saved host archive while obtaining fresh device evidence. Never label an unsupported write as completed or a readback copy as guaranteed restoration.

## Status

Show left, right, and receiver separately using real observations. Unavailable or stale battery readings are unknown. USB connection is distinct from charging. Battery calibration limits belong in contextual detail, not reassuring invented values.

## Shared recovery primitive

Recovery is a state machine with component selection, firmware identification, the applicable entry procedure, verified readiness and explicit failure/retry/cancel transitions. Opaque attempt identities reject late results after cancellation or changing components. Use one illustrated guide in the standalone utility and journey prerequisites; let reliable device observations advance it. Firmware writes remain separate guarded operations. Waiting for a person does not expire; dispatched USB requests and drive observation do.

Factory key-chord entry and compatible RMK app entry converge on the same recovery-ready state. Detect supported factory identities before showing their shortcuts; never infer a component or firmware capability from a recovery drive alone. Receiver entry retains its factory-left pairing and mapping prerequisite. Keep installation policy outside this entry primitive.

## Flow feedback

Use the linked GPUI Kit library exclusively for the app’s component system. Long, multi-component firmware and backup journeys have a read-only Stepper derived from their state machine. Short recovery and pairing utilities omit the Stepper. Never let visual navigation bypass recovery or verification. Avoid duplicate component counters and progress bars beneath the Stepper. Reserve Progress for measurable long transfers; show Spinner while discovery, USB negotiation, readback or saving has no measurable progress. Do not fabricate byte percentages or completion from elapsed time. StatusBar carries observed firmware, per-half battery availability and receiver connection without duplicating a status page.

Backup and recovery reuse one illustrated peripheral picker (left half, right half, USB dongle). Backup saves only the chosen part; do not infer a multi-part plan from attached devices. Cancelling or returning to Backup presents the chooser, and choosing an already paused part continues its remaining steps.
