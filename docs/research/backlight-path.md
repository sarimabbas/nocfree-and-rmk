# Backlight path

Investigated 2026-10-01. This is source evidence and an implementation recommendation, not a backlight hardware test. No device commands, firmware writes or GPIO changes were performed for this review.

## What exists

The user wants Mac-mode F5/F6 to decrease/increase the NocFree keyboard backlight, with ordinary F5/F6 behind Fn. These actions control the keyboard, independently of host display brightness.

The [vendor-published guide](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#4-pins-required-for-zmk-porting) identifies P0.20 on both halves as the backlight PWM/GPIO output. Its factory API drives low at value 255, but explicitly leaves physical on/off polarity to verification. Schematics and current limits are not supplied. The factory split link carries lighting commands from left to right. The community ZMK implementation deliberately excludes backlight; it provides mapping evidence, not a reusable lighting implementation.

At pinned RMK `9607aedf343b17dd6b27307583ae80c4f728fbbd`, [LightAction](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk-types/src/action/light.rs) defines BacklightOn/Off/Toggle/Down/Up/Step, but documents them as unimplemented. [Keyboard processing](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/keyboard.rs#L1203) publishes the resolved ActionEvent before its action match; the Light branch only warns. Therefore mapping F5/F6 to BacklightDown/Up alone will do nothing.

A fresh GitHub API/source check found upstream main at `a5566977c51ee70bf14e955fdcee42ee6390b6ed`; [its Light branch](https://github.com/rmk-rs/rmk/blob/a5566977c51ee70bf14e955fdcee42ee6390b6ed/rmk/src/keyboard.rs#L1305) remains unsupported. Changing the RMK pin does not supply this feature. This observation is dated, not a claim about later upstream releases.

## Smallest framework implementation

Implement a generic, optional RMK backlight service, suitable for upstream contribution, rather than intercepting physical F5/F6 in board code. The existing public [ActionEvent](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/event/action.rs) is a possible seam: subscribe to resolved Action::Light, change a bounded brightness level once per press, and apply it through a tiny output interface. RMK continues to own key resolution and debounce. Alternatively dispatch LightAction into a framework-owned lighting channel directly; avoid having both routes process the same press.

Start with saturating Down/Up and Off/On/Toggle, a fixed documented step, and one brightness byte. Do not add RGB, breathing, effects, a plugin system or another keymap interpreter. The existing light enum is non-exhaustive, so unsupported actions must remain explicit. Initial persistence is unnecessary; later persistence/sleep policy belongs in RMK storage/sleep machinery.

The board configuration supplies PWM instance, P0.20, frequency and polarity to the framework driver; brightness decisions live in RMK. Under this repository's AGENTS.md, a board-local lighting controller is outside the permitted scanner/image-tool custom code. A framework extension or reviewed upstream patch is required before shipping this behavior; do not hide key handling inside the scanner.

[Embassy nRF PWM](https://github.com/embassy-rs/embassy/blob/3861d3088da30d40c777dc05d282352e68ec5511/embassy-nrf/src/pwm.rs) already offers a one-channel SimplePwm, duty/polarity configuration, and frequency adjustment. Reuse this hardware PWM rather than software toggling or key-task delays. Its disable operation preserves the last output duty; turning off requires an explicit verified off level before stopping PWM. Frequency, polarity and initial GPIO level remain board measurements, not values justified by the published pin alone.

## Both halves

The current independent USB diagnostic can validate the left output only. Its left F5/F6 cannot control the right diagnostic: neither radio nor split messaging is running. A right-only test would need local mapped actions or a framework test command; do not mistake independent light tests for synchronized split behavior.

Production left owns the combined keymap and brightness state. Add an explicit brightness-state message to the existing RMK [SplitMessage](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/mod.rs), publish state changes from left, and apply absolute levels on the right. Append the message variant and review postcard sizing/compatibility; paired halves need matching protocol images. Existing LedState(bool) and KeyboardIndicator(u8) do not represent brightness and must not be repurposed.

The [central driver](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/driver.rs#L210) already sends a connection snapshot when its session starts. Resend the current absolute brightness similarly on every split connection, then process subsequent changes. Absolute state makes duplicate/replayed commands harmless and fixes lights after right-half reconnect. Sending relative Up/Down commands alone would desynchronize after a dropped command. Lighting traffic must not block keyboard processing or crowd out key events; this remains a required acceptance check.

## Evidence still needed

Before enabling the output, confirm physical polarity and useful duty range on this hardware revision. Published endpoint behavior does not establish whether 255 means brightest or off, transistor topology, LED load current, safe drive strength, flicker-free frequency or battery cost. Factory image strings/available update files provide no schematic or measured PWM waveform. Keep these as unknowns rather than guessing electrical properties from the key legends.

Validation should cover off and both endpoints, monotonic steps, repeated saturation, one action per key press, Fn ordinary-function behavior, and continued typing while changing brightness. Full split acceptance additionally requires both halves to agree through disconnect/rejoin and sleep/wake, with held-key release recovery, simultaneous input and wake latency tested under lighting load. Run the repository host harness and all supported role cross-builds before transport changes. Successful host tests and cross-builds are software evidence only.
