# Factory battery ADC forensic comparison

The saved factory LEFT image does not show a hidden VDD-reference override. The available evidence points to internal 0.6 V reference and gain 1/6, the same nominal 3.6 V ADC full-scale used by RMK. This is static analysis, not a factory register capture or physical calibration.

## Verified image evidence

Private evidence: `.evidence/backlight-pwm-diagnosis/factory-left.bin` and `factory-left-disassembly.txt`; factory bytes remain private. The image base is `0x27000`.

- The logical GPIO table at `0x493c8` has 17 entries; D15 maps P0.04 and D16 maps P0.05. Battery initialization at `0x2c9cc` selects D15/D16. This matches the published [NocFree hardware guide](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting).
- Initialization calls the resolution setter with 12 at `0x2c608`–`0x2c60a`.
- The reset path at `0x27264`–`0x2726a` zeroes `0x200067c0`–`0x2000de18` using the byte-fill routine at `0x45584`. This covers gain `0x2000ddd4`, reference `0x2000ddd8`, acquisition `0x2000dddc`, and burst `0x2000ddd0`.
- The ADC wrapper loads these values at `0x44964`–`0x4498a` and writes CH[0].CONFIG at `0x44992`. Each exact state address occurs once in the image, in that wrapper's literal pool. No direct reference/gain setter was found. This does not exclude every computed-address write or another execution-time modification.

The wrapper matches [Adafruit's primary Arduino SAADC implementation](https://github.com/adafruit/Adafruit_nRF52_Arduino/blob/master/cores/nRF5/wiring_analog_nRF52.c): reset-zero gain/reference select gain 1/6 and internal reference; zero acquisition selects 3 µs. Its explicit VDD-reference API changes both state fields, but no equivalent linked setter was identified in this factory image. [Nordic's nRF52833 specification](https://comp.anu.edu.au/courses/comp2300/assets/manuals/nRF52833_PS_v1.7.pdf), SAADC CH[n].CONFIG and acquisition sections, supplies the register encodings and sampling-network model. The document is manufacturer-authored; this accessible copy is university-hosted.

## Conversion and measurements

Factory instructions `0x2c8d6`–`0x2c8fc` apply approximately `count × 3300 / 4095 × 130 / 100`. This software constant is not evidence of an actual 3.3 V ADC reference. Applying it to the same counts would reduce the current estimate, worsening the observed discrepancy. The [vendor battery section](https://github.com/NocFreeKB/NocFree-and-zmk#5-battery-measurement-parameters) documents 130/100 but explicitly requires target-hardware calibration and supplies no complete divider schematic.

Observed RMK registers `[2,0,0x20000,3]` select 12-bit, oversampling bypass, internal reference/gain 1/6, 10 µs, AIN2. The 40 µs trial's count3180 is close to earlier 10 µs counts3188/3192; those observations alone do not establish that acquisition time caused the discrepancy.

| Raw count | Nominal ADC V (3.6 V/4096) | Cell estimate ×1.3 | Cell estimate ×1.5 |
| --- | --- | --- | --- |
| 3180 | 2.795 | 3.633 | 4.192 |
| 3188 | 2.802 | 3.643 | 4.203 |
| 3192 | 2.805 | 3.647 | 4.208 |

The older owner-reported 4.17 V battery-terminal reading was not simultaneous with these raw samples. It would imply an effective multiplier around1.486–1.492 if conditions matched. Thus1.5 is a plausible calibration hypothesis, not a verified divider ratio. Sampling a scaled supply rail rather than the cell, board revision differences, actual resistor values, meter-node selection, or changing battery/load conditions remain distinguishable alternatives. Neither nominal arithmetic nor the stock constants establish charger operation, capacity percentage, or a safe replacement calibration. A same-time battery-terminal voltage and raw ADC observation, preferably across more than one voltage, is needed before adopting a new scale.

No device actions, firmware changes, or builds were performed for this forensic review.

## Photo and divider-enable review

The owner-supplied `IMG_6298.heic` and `IMG_6301.HEIC` show the left controller,
battery connector and power components. The relevant small resistor values and
charger part marking are not reliably readable. Visible component locations do
not establish which resistors form the battery divider, its ratio, or whether
the sensed node is before or after a power-path component. No component identity
or schematic was inferred from appearance alone. Photo conversions and numeric
extraction results remain in private evidence.

The factory enables D16 at `0x2c7ee`–`0x2c7fa`, stores a timestamp, then reads
after the elapsed-time comparison exceeds one tick at `0x2c724`–`0x2c72a`.
This is an asynchronous wait, not a second ADC acquisition setting. Its later
digital write lowers D16 before voltage conversion. Our 10 ms enable settling
already exceeds that apparent minimum; this comparison does not demonstrate a
settling defect. Actual factory scheduling delay is not measured. The factory
also disables SAADC after reading, whereas our HAL retains the peripheral while
stopping each sample; this difference does not by itself prove voltage error.

An owner-specific provisional calibration could use the previously reported
4.17 V/12% pair, with its missing producer timestamp and coarse percentage
explicitly acknowledged. It would be a one-point effective correction, not proof
of a 1.5 resistor ratio, a discharge curve, charging state, or right-half
calibration. This investigation leaves the working firmware unchanged.
