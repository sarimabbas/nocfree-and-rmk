# RMK idle scanning on NocFree

RMK recommends interrupt-driven idle scanning through `embedded-hal-async::digital::Wait` and its `async_matrix` feature. Keep RMK's existing idle-sleep manager; interrupt-driven scanning is not another sleep state. Nordic System OFF and BLE subrating are outside this change.

## Sources and board adaptation

- [RMK low-power documentation](https://rmk.rs/main/docs/features/low_power) describes interrupt-driven scanning and BLE idle sleep. The documented subrating support has been tested on nRF52840, not this board's nRF52833.
- [NocFree hardware guide](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting) identifies PCA9555 key expanders and their shared active-low interrupt: LEFT P0.31 and RIGHT P0.05, with pull-ups.
- [TI PCA9555 datasheet](https://www.ti.com/lit/ds/symlink/pca9555.pdf), section 8.4.1, describes open-drain INT and input-read acknowledgement. Section 8.4.1.1 documents an erratum: another slave's read acknowledgement can clear INT when the command pointer remains at input register 0. Park each expander's pointer at register 2 immediately after reading it, before reading another slave. Send the command byte only; do not write output data.

NocFree uses a custom I²C scanner rather than RMK's direct GPIO matrix. Enabling `async_matrix` alone cannot change that scanner. The board adapter must wait on its expander interrupt while RMK continues to own debounce and key events. Active keys, unfinished debounce and failed input reads must keep scanning. A level-sensitive wait covers an input arriving while the interrupt is armed; a stuck-low interrupt must not create a tight loop.

## Verification boundaries

The `async-scanner` feature is opt-in during validation. Production builds retain the existing polling scanner until physical acceptance. Host tests and cross-builds establish software behavior and image layout, not electrical interrupt behavior, measured wake latency or battery endurance.

Before enabling it by default, test both halves for first-key input after idle, releases, simultaneous input, split modifiers, disconnect/reconnection and Companion recovery. TI also describes an interrupt acknowledgement edge case; successful host tests cannot establish an absolute no-loss guarantee on the physical board.

The existing 30-minute RMK battery sleep policy and USB-powered awake override remain separate from this scanner change. The owner's immediate typing after an idle interval confirms the observed typing result; without an independent sleep-entry observation or current measurement it does not prove sleep entry or power savings.

Software checks on 2026-10-04 passed: 23 input host tests, 78 Python safety tests, independent review, and release cross-build/image guards for LEFT, RIGHT and dongle, plus LEFT and RIGHT with `async-scanner`. The isolated interrupt candidates occupy 369,820 bytes (LEFT) and 228,500 bytes (RIGHT).

The subsequent combined trial installed interrupt scanning on both halves and status-light mixing on LEFT (371,348 bytes). Both application readbacks were exact and Companion recovery worked. The owner reported working typing and USB attach/detach, purple Dongle indication on battery, blinking blue while Bluetooth was unpaired and solid blue after pairing. RIGHT's status indicator and the dongle firmware remain unchanged. These observations are not instrumented latency, endurance or current measurements; 30-minute sleep-entry acceptance on this combined build remains pending.
