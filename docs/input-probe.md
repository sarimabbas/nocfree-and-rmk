# Right-half USB input bring-up candidate

Status: installed in an owner-authorized right-only trial on 2026-10-01. USB identity, version greeting, keyboard HID enumeration, software update entry and exact application readback passed on macOS. Physical typing, key mapping, cold recovery of this candidate and wake/disconnect acceptance remain pending. Left and receiver remain factory firmware. The right is temporarily in MSC bootloader mode for the recovery checks.

## Observed trial results

The prior running recovery probe matched its saved complete readable flash hash before installation. The input diagnostic was then transferred through the tested application-only serial DFU route. It enumerated as `NocFree Input Probe Right`, returned the expected version greeting and exposed a keyboard HID interface. macOS showed Keyboard Setup Assistant; that is enumeration evidence, not a completed key test.

The candidate's 1200-baud CDC request successfully returned to the same MSC bootloader. Readback matched the exact application BIN, unchanged resident S140, unchanged readable flash above `0x36000` and the expected erased tail within the 15 application pages. Full readable UF2 SHA-256 is `1cf20d737280712a5edae5dc50e4946874db35b8f146a83fb3d9bb1e080eaff0`. Bootloader INFO remained identical; this is not a bootloader binary readback. Private records and readbacks are stored outside Git in `.evidence/trial-input-right/`.

## Build and exact candidate

From `firmware/`:

```sh
cargo build --locked --release --bin input-probe --target thumbv7em-none-eabihf \
  --no-default-features --features right,input-probe,usb-recovery-first
```

The local candidate has a 57,844-byte file-backed flash span `0x27000..0x351f4`; its UF2 pads through `0x35200`. Application-only serial DFU erases 15 pages, `0x27000..0x36000`, preserving S140 and all flash above that erase extent. Initial SP is `0x20020000`, reset vector `0x27205`, and the required marker is at `0x27200`. BIN extraction checked every ELF LOAD segment, rejected overlap/protected flash, and checked RAM segments before constructing the guarded UF2. Generated artifacts stay in ignored `dist/input-probe-right/`.

- UF2 SHA-256: `348700d3098a24b70abb69925bc326c9075757ce58aab4fce240863355be3eb9`.
- BIN SHA-256: `76557bbe0b825f2d880c53945c8e963f9a79ab81a7316af84d6e72526df3b541`.
- Serial ZIP SHA-256: `3d59e443c087a4eb8d029df2b053b5322584997acdf0554d3b24410dfd615f61`.

The pinned Adafruit package generator produced the ZIP. Its exact BIN/DAT/manifest, CRC, UF2 padding and page-rounded erase extent passed the serial-package guard:

```sh
python3 scripts/image_guard.py --image dist/input-probe-right/input-probe-right.uf2 \
  --serial-package dist/input-probe-right/serial-trial.zip
```

The repeatable harness passes 32 Python safety tests, seven Rust scanner tests, formatting, all three recovery-first migration roles and both factory-preserving diagnostics. Those are software results, not approval for any migration or device write.

## Scope and framework ownership

This diagnostic tests the 47 right-half inputs through USB, independently of the left and receiver. It retains the existing PCA9555 scanner and RMK debounce, keyboard processing and USB HID transport. It does not enable BLE, storage writes, battery ADC, backlight, mode selection or the production split topology. Its temporary local right-half keymap exists only to validate board input; the production left still owns the combined keymap.

At pinned RMK commit [`9607aedf`](https://github.com/rmk-rs/rmk/tree/9607aedf343b17dd6b27307583ae80c4f728fbbd), the public [`UsbTransport::builder` / `usb_builder`](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/usb/mod.rs) seam accepts an additional CDC class beside the framework HID interfaces. This preserves the tested 1200-baud update request independently of I2C scanning, without another board-local HID stack. The framework's native DFU-detach interface is gated on its dongle feature; enabling receiver behaviour just to obtain that interface is unnecessary.

Candidate identity is `4c4b:4651`, product `NocFree Input Probe Right`; these development identifiers are not registered production identifiers. Expected CDC greeting is `NocFree input probe 0.1.0 right factory\r\n`. Read that greeting and inspect the guarded artifact hash before a trial. Do not identify a board using a previously seen serial-port path.

## Recovery requirements

Use only the factory-preserving application layout at `0x27000`, with the existing recovery marker `0x87eeb07c` at `0x27200` and bootloader-preserved low RAM. The exact candidate must pass the address/vector/family guard, include no reclaimed-SoftDevice feature, and fit the existing protected ranges. Record its BIN/UF2/serial-package hashes and page-rounded erase extent before any write. Existing right-specific bootloader evidence and factory/probe restore images remain prerequisites; they do not establish left or receiver recovery.

Two application requests should remain available:

- CDC: deliberately select 1200 baud and deassert DTR to request the existing bootloader. Ordinary greeting reads use 115200 baud with a low-to-high DTR pulse; 1200 baud is not a monitoring setting.
- Keymap: hold the right Fn key, press and release main-row `0`, then release Fn. RMK's [`Bootloader` action](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/keyboard.rs) runs on the mapped key's release; no five-second hold is intended. This route depends on working scanning and is not a replacement for cold recovery.

Application-independent recovery was proven with the earlier marker-bearing recovery probe on this specific right half: unplug USB with switch OFF for five seconds, then reconnect USB while leaving the switch OFF. USB enumeration holds the existing bootloader. To start that earlier probe, turn ON without USB, wait five seconds, then attach USB. Retest both sequences with the exact input candidate after an authorized installation. Do not interpret OFF with USB attached as removing USB power.

## Trial sequence after explicit authorization

1. Identify the right half alone, verify its running recovery-probe greeting, and verify locally saved right factory/probe backups and candidate guards. Choose a plain capture window rather than a terminal, password field or application with shortcuts.
2. Install only the reviewed application package through the already tested right update route; read back the application and preserved readable ranges. Record exact mismatches as failures rather than relying only on USB enumeration.
3. Perform battery-first startup. Verify candidate USB identity, CDC greeting, and HID interfaces. Test one ordinary key and its release before exercising shortcuts or chords.
4. Sweep every right physical position independently in the order below. Capture HID usage press/release where available; screenshots or text alone cannot verify silent modifiers, Fn or system keys.
5. Exercise right Shift, Alt and GUI with ordinary keys, repeated press/release, six ordinary held keys, rolls and the Fn media layer. More than six ordinary simultaneous keys is outside the pinned RMK report capability. Fn itself changes layer and should not emit an ordinary key.
6. Test USB unplug/replug with an ordinary key held, release while unplugged, and repeat with right Shift. Verify the host has no stale held modifier and the reconnected device reconciles physical state. Separately test host suspend/wake and the first press/release after wake. Record failures; the current pinned USB writer has known suspend/retry questions, so these tests are required rather than assumed passing.
7. Test CDC update entry and cold recovery independently; confirm marker and preserved ranges after readback. Retest candidate startup. If any input/recovery check fails, use the already saved right recovery probe or factory restore image through the proven route; do not introduce bootloader or S140 changes to repair this trial.

Expected base positions (right scanner columns `0..46`):

```text
F7 F8 F9 F10 F11 F12 PrintScreen Home
7 8 9 0 - = Backspace PageUp
Y U I O P [ ] Backslash
H J K L ; Quote Enter Delete
N M Comma Dot Slash RightShift Up PageDown
Space RightGUI Fn RightAlt Left Down Right
```

Verify F7–F12 media-layer actions, main-row `0` recovery, and the remaining Fn positions as transparent base actions. Fn host-profile/dongle actions are intentionally absent from this USB-only diagnostic. Do not use the recovery gesture during an ordinary complete-key sweep; verify base `0` without Fn first.

## What a pass establishes

A successful trial establishes right input mapping, ordinary USB typing and the retained update route on the tested host. It does not establish radio behaviour, complete split operation, battery life, Windows/Linux support, numerical latency or universal absence of lost events. The measured high-volume, simultaneous input and cross-platform release gate remains [acceptance](acceptance.md).
