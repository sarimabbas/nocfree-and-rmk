# Backlight PWM diagnosis

Investigated 2026-10-02 after the owner tested the left active-high candidate. The investigation used saved binary readbacks and owner observations. No additional device writes, commands or GPIO operations were performed.

## Observed and inferred

The owner reports that the key backlights start off, become apparently full brightness with F6, and need three F5 presses to turn off after exactly three F6 presses. This establishes that brightness actions advance and decrement logical state. It does not measure duty cycle, waveform, light output or LED-driver bandwidth.

The saved factory applications provide a stronger frequency reference than a generic Arduino default: **both halves explicitly request 400 Hz**. The candidate's 8 kHz choice is twenty times higher and was an implementation assumption, not a measured factory setting.

## Factory binary evidence

Read-only disassembly of each half's own saved CURRENT.UF2 application finds:

| Half | Startup request | Frequency setter | Peripheral |
| --- | --- | --- | --- |
| Left | `0x2c646`: load argument 400; `0x2c64a`: call | `0x2b854` | PWM2, base `0x40022000` |
| Right | `0x2aed0`: load argument 400; `0x2aed4`: call | `0x29a10` | PWM2, base `0x40022000` |

The setters configure Arduino pin alias 5 as an output, matching the [vendor backlight mapping to P0.20](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/README.md#4-pins-required-for-zmk-porting). Their divider-selection code first tries 16 MHz / requested frequency. At 400 Hz that gives 40,000, exceeding the 15-bit counter range. It selects the next option, 8 MHz / 400 = 20,000, writes prescaler 1 to offset `0x50c` and countertop 20,000 to offset `0x508`. The PWM initialization writes MODE 0 (up counter) and DECODER 2 (individual channels).

Those addresses and encodings match Nordic's [nRF52833 register definitions](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/343ab5fe7fb3d5657d3176883dfb188d65b79f06/cores/nRF5/nordic/nrfx/mdk/nrf52833.h) and [bitfield definitions](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/343ab5fe7fb3d5657d3176883dfb188d65b79f06/cores/nRF5/nordic/nrfx/mdk/nrf52833_bitfields.h): prescaler 1 is 8 MHz, MODE 0 is edge-aligned up counting, and DECODER 2 maps each half-word to its channel. This verifies the binary's requested configuration; it is not an oscilloscope measurement or proof of all runtime paths.

The duty path scales an 8-bit argument against the configured countertop and clears bit 15 in the sequence half-word. This agrees with the vendor statement that endpoint 255 drives the pin low. Physical LED polarity is established by the owner trial more directly than by the API's naming.

For comparison only, current [Adafruit HardwarePWM source](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/343ab5fe7fb3d5657d3176883dfb188d65b79f06/cores/nRF5/HardwarePWM.cpp) initializes a maximum of 255 and a 16 MHz clock. Its [analogWrite wrapper](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/343ab5fe7fb3d5657d3176883dfb188d65b79f06/cores/nRF5/wiring_analog.cpp) uses eight-bit resolution by default. Those defaults alone would imply about 62.7 kHz, but do **not** describe this factory application: it has an explicit frequency setter and 400 Hz call. The exact Arduino package/version used to compile the factory image remains unverified.

## Ranked explanations and next experiment

1. **Frequency-dependent hardware response:** strongest actionable lead because the factory deliberately uses 400 Hz and the trial uses 8 kHz. A driver or filtering response could turn short pulses into near-binary brightness. Its circuitry and bandwidth are unknown; do not infer a particular driver IC or schematic.
2. **Unexpected actual waveform or duty:** still possible. Logical brightness changes do not prove the PWM pin produces the requested waveform. Verify the adapter's register/sequence behavior and, when available, measure P0.20.
3. **Perception and linear brightness scale:** possible, but less persuasive if levels 1–3 all look equally bright. A nonlinear curve may improve the useful steps later; it should not hide a frequency or waveform defect.

A bounded next experiment should preserve active-high polarity and the same action/state logic while matching the factory's 400 Hz configuration. **Do not silently lower frequency with the present synchronous adapter:** [Embassy SimplePwm::set_duty](https://github.com/sarimabbas/embassy-nrf-nocfree/blob/9640cd7af9d2e5f9fe9860aae7108e4a04f96c8c/src/pwm.rs#L850-L884) spins for sequence completion while enabled. At 400 Hz one nominal PWM period is 2.5 ms, rather than the 125 µs assumption at 8 kHz. Prefer an asynchronous hardware update path before that trial, and validate typing and radio responsiveness alongside the visual result. No implementation or installation is authorized by this research document.

Private evidence is retained under `.evidence/backlight-pwm-diagnosis/factory-*`; binary backups and disassembly are not published.

## Candidate change

The next software candidate keeps the same active-high polarity, level scale and key actions. It changes the PWM setting to Div2 / countertop 20,000 (400 Hz) and uses an asynchronous duty setter in the pinned Embassy fork. Its normal DMA wait yields cooperatively instead of occupying the executor until SEQEND. This is self-waking polling during lighting changes, not an interrupt-backed sleep.

The buffered PWM driver owns a unique static RAM buffer. It retains pending-transfer state even if an update future is cancelled or forgotten, and cooperatively waits for that transfer before changing the buffer again. Moving or dropping the driver cannot free or move this DMA source. This avoids both the normal synchronous wait and a blocking cancellation guard. The previous synchronous API remains unchanged. No hardware latency bound has been measured.

Nordic's [nRF52833 PWM specification](https://docs.nordicsemi.com/r/bundle/ps_nrf52833/page/pwm.html?contentId=tF4489dcQq9LN2iwRSaljg) explicitly states that a sequence played once continues generating PWM with its last loaded value. Missing sequence looping is therefore ruled out as an explanation for this adapter.

The visual acceptance loop is unchanged: reach off with F5, press F6 exactly three times, compare the three nonzero brightness levels, then press F5 three times. The current image passes logical step counting and fails visible dimming. A host test cannot reproduce or establish LED-driver response; a new owner trial must re-run this loop and check typing during brightness changes. This candidate does not change the right half or receiver, and has not been installed.
