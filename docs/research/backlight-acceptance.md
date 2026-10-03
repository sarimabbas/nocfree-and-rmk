# Backlight acceptance

Initial independent review plan, 2026-10-02. The first-candidate review below is historical; see [400 Hz results](backlight-400hz-validation.md) for owner-observed left dimming and [hold behavior](backlight-hold.md) for the follow-up. Proposed checks below are not themselves hardware results.

## Electrical gate and first trial

The [vendor pin map](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#4-pins-required-for-zmk-porting) identifies P0.20 on each half, but does not prove physical polarity, current limits, transistor topology or PWM frequency. The factory API's low endpoint is source evidence, not proof that low turns the LEDs off.

**A low electrical duty is not a conservative brightness test when polarity is unknown:** it can illuminate active-low hardware almost fully. Prefer factory source/waveform evidence for polarity first. If that is unavailable, describe a brief, owner-observed polarity trial explicitly as an experiment; do not advertise it as electrically proven safe. A 50% waveform has equal asserted time for either polarity, but observing it alone cannot identify polarity. A short comparison above/below 50% identifies the direction of change; test one half first, stop immediately on abnormal heat, resets or visibly unexpected output, then verify the off endpoint before leaving lighting running. No automatic full-brightness sweep while polarity is unresolved. Exact durations and electrical settings belong in the reviewed trial package.

Use ordinary GPIO drive strength; do not compensate for unknown circuitry with high drive. Configure the known off idle level after identifying polarity. Check output at startup, reset, update entry and sleep, since PWM disable is not synonymous with LED off.

## Source review gates

- Allocate P0.20 and one unused PWM peripheral exactly once. Current board main reserves TIMER0/RTC0/PPI resources for MPSL/SDC; it does not currently allocate a PWM peripheral. Check the actual final role configurations for conflicts.
- Keep brightness state/action handling in RMK. Process a tap once; the follow-up deliberately repeats physical Up/Down holds on RMK's deadline scheduler and stops on release. Other actions stay single-press. Saturate and make absolute state replay idempotent. Fn+F5/F6 must retain ordinary function keys in Mac mode.
- Keep key processing free of awaited lighting queue/radio/flash operations. Coalesce latest state rather than accumulating unlimited changes; preserve key events ahead of optional lighting traffic. A bounded event subscriber can still block the publisher if it stops draining.
- Review sleep/disconnect cancellation: output must reach verified off, reconnect must apply current authority state, and restarting tasks must not retain a stale subscriber or miss the latest snapshot. Snapshot capture and event subscription need an explicit ordering that cannot lose an intervening brightness change.
- Append split message variants, test postcard bytes for existing variants, and audit size constants/MTUs and mixed-version behavior. Do not reuse indicator messages as brightness. Matching half versions are required if the protocol is incompatible.
- Delay/coalesce persistence after the last change, write only changed settings and retry failed writes without holding key processing. A stale delayed snapshot must not overwrite a newer level. Keep persistence framework-owned; independently record whether sleep/wake changes are temporary or saved.
- Check RMK schema hash before the trial. Feature or commit changes have previously reset this board's settings/bonds. Disclose any reset and preserve a fresh backup; do not promise pairing survives or bypass the schema check to preserve incompatible state.

Pinned [Embassy PWM source](https://github.com/sarimabbas/embassy-nrf-nocfree/blob/9640cd7af9d2e5f9fe9860aae7108e4a04f96c8c/src/pwm.rs#L840-L884) matters: `SimplePwm::disable` retains the last output; `set_duty` busy-waits for DMA sequence completion while enabled. Running that call in a separate async task still blocks the same executor during the call. Bound update frequency, avoid identical rewrites, and measure the actual worst case. `DutyCycle::normal(v)` outputs high when counter >= v; `inverted(v)` outputs high when counter < v. Verify endpoints against those definitions rather than assuming the names mean positive duty.

## Repeatable software gate

Run Python image/serial guards and `nocfree-input` fault tests before changing scanner/transport behavior. Run `scripts/check.sh --reclaimed-softdevice` with the documented toolchain, covering left/right/receiver and default/Mac keymaps, then separately report the protected receiver build. The protected full-left layout is already oversized; do not turn that known limitation into a reclaimed-layout failure or silently omit it. Run candidate-specific tests for saturation, press/release, replay/rejoin, delayed-save races and unchanged split serialization. Cross-build success is not hardware acceptance.

## Minimal owner acceptance sequence

| Check | Required observation |
| --- | --- |
| One-half polarity trial | Direction identified; off is physically off; no unexpected resets/output. Record each half separately. |
| Controls/endpoints | F5/F6 decrease/increase monotonically, repeated endpoints saturate, one step per press, hold/release is stable, Fn keys behave ordinarily. |
| Both-half state | Levels agree; change while right is disconnected, reconnect it and verify current level without another key press. Repeat wake/rejoin. |
| Persistence | Change level, wait the save delay, restart normally; saved level returns. A quick restart before the delay follows the documented policy. Pairing outcome matches schema disclosure. |
| Input under load | Type known text on both halves while repeatedly alternating brightness. Hold left Shift while typing right; release a right key during split disconnect/rejoin and verify no stuck input. |
| Wake and routes | Test first key after idle, simultaneous input and brightness across wired, direct BLE and receiver. No visible lost/stuck/reordered input; record timed wake/latency measurements separately from subjective observations. |

Do not claim “zero missed keys” or “no lag” from a short visual test. Compare instrumented input/wake timings with the working baseline and count transmitted/received events in a repeatable capture before claiming a performance bound. Existing disconnect/release-recovery host tests cover scanner behavior, not real radio delivery under lighting load. Windows/Linux acceptance remains separate from macOS observations.

## Initial candidate review

The private candidate reviewed on 2026-10-02 uses direct framework LightAction dispatch into a bounded latest-value Watch, explicit polarity selection, PWM0/P0.20 and 8 kHz PWM. No resource conflict was found in the board main; the PWM buffer pointer is refreshed by `set_duty` before its DMA START after moves. The source's nominal update spin is approximately one 125 µs PWM period, not a measured timing bound. Registering a split Watch receiver before taking the reconnect snapshot retains changes made while that snapshot is sent. Appending Backlight preserves older variant indices; an old right image may discard the new message, so typing alone cannot prove synchronized lighting.

The final candidate fixes both requested issues: it caches the applied effective PWM level and records recognized action intent even when the default level has not changed. An explicit startup Off therefore wins a delayed nonzero storage read. Its six backlight tests cover saturation/toggle, unsupported actions, coalesced snapshots/rejoin, an old split discriminant plus absolute-message round trip, delayed storage saves and startup Off precedence. The async persistence test also checks that sleep does not postpone the save or persist transient zero. Tests do not establish physical PWM polarity, actual task timing or radio delivery.

The board currently configures `split_central_sleep_timeout_seconds = 0`. RMK's sleep manager then parks before processing idle or explicit sleep requests. Added sleep hooks do not establish automatic idle-off or host-suspend-off in that configuration. Keep this limitation explicit, or change the configuration separately and validate wake/input behavior.

A separate integration fix changes macro splice validation to reject `MACRO_MAX_NUM == 0` with `checked_sub(1)?`, then reject an out-of-range slot before segment access. This resolves the pinned zero-macro configuration's underflow while retaining valid-slot behavior. No blocking source issue remained in the final review; hardware acceptance and build/test results must still be reported separately.
