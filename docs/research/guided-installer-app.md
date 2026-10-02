# Guided native installer

Research and proposal, 2026-10-01. No application was built, no devices were accessed and no firmware was written for this research. This extends [newcomer installation](newcomer-installation.md); it does not establish a production installation route or broaden any approved hardware trial.

## Decision

Yes: a GPUI application with **iamnbutler/gpuikit** can replace the repeated chat instructions with one illustrated step at a time, automatic observation of USB removal/reappearance, local backup handling and clear verification results. The benefit comes from an installer session that tracks evidence. GPU rendering is incidental to that benefit. The person still moves cables and switches; host USB observations cannot directly prove a mechanical switch position or battery power state.

Start with a macOS read-only companion. Give it the same workflow model that will later support Windows and Linux, but do not promise those builds before testing them. Keep current experimental migration sequences available only as explicitly labeled developer sessions. Ordinary newcomers should see a supported installation route after firmware and recovery acceptance pass.

## Library evidence

Context7 resolved `/iamnbutler/gpuikit` and returned initialization examples; these were checked against primary repository/docs sources. This is a separate project from **longbridge/gpui-kit**, whose facade and components are not interchangeable with gpuikit. Longbridge is an alternative if the chosen toolkit blocks a required platform; do not mix both component systems. [gpuikit README](https://github.com/iamnbutler/gpuikit#readme), [Longbridge README](https://github.com/longbridge/gpui-kit#readme).

The inspected gpuikit package is 0.9.0, MIT OR Apache-2.0, edition 2024, with a declared Rust 1.85 floor. Its actual dependency tree can require a newer compiler. It depends on `gpui-unofficial` and `gpui-platform-gpui-unofficial`; the app must use matching GPUI packages. The README/Context7 example says 1.14, whereas the inspected main-branch Cargo manifest says 1.18. Resolve the published package's actual dependencies and pin one compatible graph plus toolchain in the prototype; this note is not a tested Cargo manifest. [Manifest](https://raw.githubusercontent.com/iamnbutler/gpuikit/main/Cargo.toml), [published API documentation](https://docs.rs/gpuikit/0.9.0/gpuikit/).

The initialization pattern attaches `gpuikit::assets()` to an application using `gpui_platform::current_platform(false)`, then calls `gpuikit::init` in the application run callback. gpuikit supplies controls, themes, input and accessibility modules; it does not supply keyboard identification, UF2 safety or USB monitoring. It is pre-1.0 and explicitly warns of breaking releases. [API documentation](https://docs.rs/gpuikit/0.9.0/gpuikit/), [README](https://github.com/iamnbutler/gpuikit#readme).

GPUI upstream describes macOS Metal rendering, Linux X11/Wayland backends and Windows Win32/DirectWrite. The unofficial distribution publishes platform crates for these three systems and is unaffiliated with Zed; it describes its GPUI code as Apache-2.0. This is upstream capability, not validation of our installer. [Upstream GPUI README](https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/README.md), [unofficial distribution](https://github.com/iamnbutler/gpui-unofficial#readme).

gpuikit's inspected CI tests macOS and Linux, checks WASM, and explicitly omits Windows. Therefore Windows is a concrete compatibility spike, not a supported installer claim. Verify the selected release's platform feature requirements too; current GPUI upstream requires a Linux windowing backend and macOS font support. [gpuikit CI](https://raw.githubusercontent.com/iamnbutler/gpuikit/main/.github/workflows/ci.yml), [GPUI platform configuration](https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/README.md).

## One window, one session

Use a calm window with a drawing of the selected component, the next physical action and its observed status. A short progress strip can say Connect → Save recovery → Install → Check. Keep technical evidence in an expandable details panel. Show a specific failure and its safe next action rather than restarting the journey or reporting generic failure.

For example, “Unplug the left cable” remains on screen until the selected USB device and its correlated mounted drive are absent. Then show “Move to Bluetooth; keep the cable unplugged.” The battery-start countdown begins only after USB absence is observed and the person acknowledges moving the switch to Bluetooth. Premature USB reappearance invalidates that step; repeat the confirmed unplug/switch/wait sequence. Switch position and battery startup still need a concise human acknowledgment because the host cannot observe them. Reconnection advances only when the expected device appears; a fixed timer never substitutes for that evidence.

Keep the architecture small:

| Boundary | Owns | Does not expose to the UI |
| --- | --- | --- |
| Installer session | Selected role, current phase, required evidence, permitted next action, persistent local journal | USB platform calls, address parsing, raw transfer operations |
| Device and image service | Read-only discovery, role correlation, backup/readback, canonical image guards; later one bounded approved transfer | Arbitrary flash addresses or a generic “write any file” operation |
| GPUI view | Current instruction, observation status, user acknowledgment, explicit install/restore action | Safety decisions or device matching rules |

Use one transition function over observations and user intents; keep role-specific tested sequences as a small set of explicit plans. Avoid a generic workflow language, plugin system, server or keymap editor. Platform differences belong inside discovery/mount adapters. File I/O and observation work run away from rendering, with results delivered through GPUI's application state. The GPUI framework provides entities/views, an event-loop-integrated async executor and test support. [GPUI programming model](https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/README.md).

## Evidence and write gates

The existing [image guard](../../scripts/image_guard.py) and [migration guard](../../scripts/migration_guard.py) are read-only Python functions. They validate structures and exact artifacts; they explicitly do not establish the attached role, recovery or authorization. The minimum prototype approach is to invoke this existing helper with the development Python runtime; a private prototype bundle can include that runtime temporarily. Do not translate policy into button callbacks. A newcomer binary must eventually package its validator without requiring a system Python. Decide the distribution mechanism in a separate spike; if porting to Rust, replace the canonical implementation deliberately and prove equivalence with existing malformed/valid fixtures before enabling writes.

Session transitions should distinguish:

1. **Selected and identified:** user chooses the pictured role; observe its normal-mode identity and bind the transition to that physical connection. The shared bootloader volume name, VID/PID and silicon family cannot identify a half. If correlation becomes ambiguous, stop and ask for only the selected component to remain connected. A known factory readback hash can corroborate a supported initial role; unknown hashes do not become universal role recognition. [Recorded role limits](newcomer-installation.md#minimal-install-journey).
2. **Recovery ready:** the component's independent recovery route has applicable proven evidence; preserve its own readback privately, validate it and record backup coverage. Save the journal before continuing. A readable `CURRENT.UF2` excludes some chip regions and must not be called a full-chip backup. [Migration policy and restore bounds](../../scripts/migration_guard.py).
3. **Ready to install:** exact role/layout/protocol-compatible manifest, artifact hash, structural guard, expected bootloader and retained-range policy pass. An explicit action shows the selected component and exact version/change. Recheck connection and identity immediately before copying; disconnect or stale observations invalidate readiness.
4. **Transfer completed:** file copy and flush returned successfully. This state is distinct from firmware verified. Never automatically retry an uncertain write. Reconcile actual device/readback state first.
5. **Verified:** expected running identity/version, applicable exact readback and functional checks pass. Present what was observed separately from what remains to test. A successful build, UI simulation or file copy cannot fulfill hardware acceptance.
6. **Restore offered:** use the same identified device, proven independent recovery route and exact private saved image. Restore remains an explicit bounded action, followed by verification. Host-retained previous images are manual rollback, not onboard A/B rollback.

The journal lives in application support, contains session/phase, artifact hashes and observations, and resumes safely after app restart. Device identifiers and factory images stay local. No telemetry is needed. An interrupted transfer resumes at inspect/reconcile, never at replay-write. Writes remain unavailable if the role, image, recovery path or current observation is ambiguous.

## First useful prototype

Build a separate desktop package so GPUI dependencies never enter the embedded firmware dependency graph. First deliver only:

- A real GPUI/gpuikit window with Connect, Identify, Save recovery and Check views, plus a visible “Read-only companion” label.
- macOS USB/mounted-volume observation; snapshot reconciliation handles missed or duplicate events. No reset commands, DFU entry commands or write actions.
- Explicit role selection, shared-bootloader ambiguity handling, observed unplug/replug status and a local session journal.
- Bootloader metadata/readback inspection and private local backup when the owner has put the selected device into its known read-only bootloader route. Offer to open the backup folder.
- Replay fixtures for “cable was not removed,” multiple identical drives, late mount, drive removed during backup, wrong readback, cancellation and application restart. UI simulation and discovery tests are host verification only.

Prototype acceptance: launch on this Mac, observe an owner-directed unplug/replug without asking them to type “done,” and show a trustworthy local backup/coverage result. No flash occurs. Before a writer is added, independently review the canonical guards and session gates; test one exact approved device sequence with the user. Windows/Linux packaging and actual hardware acceptance follow separately. The final supported app should ship as an ordinary OS download with its resources/validator included and an offline recovery guide; newcomer installation should require no developer tools.

This is several distinct pieces of work: the small read-only UI, reliable role/discovery and journaling, guarded transfer/restore, then three-OS distribution and device acceptance. The graphics framework can accelerate the first piece; it does not remove the remaining engineering or the physical recovery sequence. Once routine update entry works reliably from production firmware, routine updates should become much shorter than the current bring-up trials. Initial factory migration remains a separate, prominently explained operation.
