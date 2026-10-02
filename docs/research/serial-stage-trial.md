# Left serial-stage trial, 2026-10-01

The owner approved the two exact candidates recorded in [serial-stage-candidates.md](serial-stage-candidates.md), followed by restoration of the original left factory image. The Companion guided the physical cable/switch steps; the host controller separately gated each transfer on identity, address/vector/family guards and fresh readback. The app itself sent no firmware or serial commands.

## Verified observations

- Before installation, the complete exposed factory readback matched the original SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`, with S140 7.3.0 metadata.
- Each candidate independently returned to MSC recovery through the WIRED unplug/wait/reconnect sequence. Its binary and erased page tail matched exactly; exposed bytes at and above `0x4000` remained unchanged. The HAL candidate was checked again immediately before the second transfer.
- Each battery-first sequence included observed USB absence for five seconds, an owner acknowledgment of the Bluetooth switch position, ten seconds unplugged, then reconnection to the bound USB connection.
- Both candidates then enumerated as `239a:002a`. Three fresh observations per candidate showed actual CDC interface classes `[2, 10]`, no MSC interface and no mounted `CURRENT.UF2`. No serial port was opened.
- The complete saved original factory container was restored. A subsequent factory Fn+5 readback matched the original SHA-256 exactly, including S140. A final WIRED unplug/wait/reconnect sequence returned to normal `2886:8029` factory operation. The right Mac diagnostic remained enumerated; the receiver was untouched.

The first HAL transfer immediately left MSC for serial mode, so an immediate readback was unavailable. No transfer was repeated: its exact readback was obtained after independent physical MSC recovery instead.

## Host observation correction

The first serial-mode check falsely rejected the HAL result because the private IOService parser allowed descendant interface/driver entries with inherited VID/PID/location to overwrite the parent device. It reported `[10, 10]` instead of the actual CDC interfaces. The corrected parser selects `IOUSBHostDevice` parents and counts only `IOUSBHostInterface` descendants. Independent synthetic review confirmed that CDC `[2, 10]` is retained and an actual class-8 interface remains a rejection. The original failed observation was retained privately; both accepted calibrations used fresh live observations with the corrected parser. This changed host observation only.

## Interpretation and next boundary

Both stage-return candidates produce the intended distinguishable serial mode under this launch sequence. This supports execution through HAL initialization and USB construction, but a matched negative control has not excluded a vendor battery-start branch selecting serial mode independently. It is preliminary mode calibration, not independent proof of the exact hook.

USB polling, peripheral enable, interrupt delivery, application enumeration, input latency and radio behavior remain unvalidated. The next software investigation should focus on the later USB run/enable boundary and preserve these recovery gates; this trial does not authorize another image. The full left RMK application also retains its separate flash-budget and hardware acceptance requirements.

The physical guide worked across these observations, but progression between steps required the host controller to be invoked through chat. Its “Checking” screen therefore waits for the controller rather than completing an autonomous installer journey. Integrating bounded controller orchestration and an explicit paused state remains app work.
