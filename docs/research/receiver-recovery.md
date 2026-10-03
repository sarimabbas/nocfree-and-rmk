# Receiver recovery research

Research checkpoint: 2026-10-02. Source and archive inspection only; this document records no receiver hardware test or write authorization. The owner previously preferred keeping the keyboard halves closed; their preference for the receiver enclosure has not been established.

## What the primary sources establish

The [vendor troubleshooting page](https://www.nocfree.com/pages/nocfree-and-troubleshooting) links a [Dongle firmware recovery guide](https://docs.google.com/document/d/1ie1Sjx0E7dMcdMRXochTKFrSFeYoO5rgx8PHVy379mM/edit). That guide restores the left keyboard to factory version 1.5.2, assigns Dongle DFU to the FN-layer 6 key, then requests dongle recovery over the factory wireless link with a five-second Fn+6 hold. It supplies no separate physical receiver reset sequence. Its old-version workflow is source evidence, not a recommendation to downgrade this working RMK keyboard.

The [vendor update guide](https://www.nocfree.com/blogs/news/nocfree-firmware-update-guide) likewise checks the factory Dongle DFU assignment. This depends on factory keyboard behavior and its proprietary radio protocol. The existing RMK left cannot issue that factory command. Restoring the left just to obtain receiver entry is outside this proposed source-only research and is not required merely to inventory the receiver.

The [community ZMK limitations](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/limitations.md) exclude the factory USB receiver. Its pinned tree has left/right board targets, with no receiver implementation. [Community recovery instructions](https://github.com/NocFreeKB/NocFree-and-zmk/blob/8bc5f6fe4531cadc62dc39aa92750fba90e009c4/docs/recovery.md) describe half-specific CDC entry and explicitly leave accessible double-tap reset unverified. Neither establishes recovery on this receiver.

The supplied `output_20260911_v2.4.5.zip` includes a factory dongle UF2 and application-only serial DFU ZIP. Static parsing confirms UF2 family `0x621e937a`, targets `0x27000..0x38f00`, SP `0x20020000`, and reset vector `0x34f29`. The DFU manifest requests S140 FWID `0x123`. These identify the packaged application, not the attached receiver's bootloader, exact partition boundaries, or independent recovery behavior. Private archive summaries remain under `.evidence/receiver-research/`; factory firmware and recovery executables are not redistributed.

## RMK receiver wiring and updates

The local receiver role uses the nRF52833 internal BLE controller as a central and USB HID output. It constructs RMK `Dongle`, `DongleRouter`, USB transport and storage. It does not configure a keyboard scanner, battery divider, backlight or undocumented receiver GPIO. Its LF clock uses calibrated RC; an oscillator, external radio, LED or reset pin must not be inferred from the halves.

Pinned RMK already supplies a [receiver USB DFU runtime interface](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/usb/dfu_detach.rs): DFU_DETACH requests the bootloader during the first 30 seconds after plug-in. [Its boot helper](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/boot.rs) writes `GPREGRET=0x57` only when `adafruit_bl` is enabled. The current local `receiver` feature selects `rmk/dongle` but omits `rmk/adafruit_bl`; without that feature, the helper performs an ordinary reset. Once this receiver's bootloader is identified, selecting the existing framework feature is the minimal update-path correction. No custom serial transport is needed.

This native runtime entry depends on working application USB. It is not independent recovery from a failed replacement. The halves' recovery-first marker is unsuitable for a permanently USB-powered receiver because enumeration holds it in DFU and prevents normal cold-plug operation. A final receiver candidate must omit `usb-recovery-first`; this removes the halves' proven independent entry mechanism and therefore requires separate receiver evidence.

## Device observation

Read-only owner-correlated USB inventory found the attached factory `NocFree_Dongle` with CDC and HID interfaces. The owner reports no visible external button or reset opening. No serial port was opened, command sent or firmware transferred. This confirms normal USB enumeration only; independent recovery remains unproven.

## Offline build checkpoint

Private compile-only experiments retained the current MTU 251, queue capacities, release optimization and low-RAM reservation. The protected receiver application fits at `0x27000..0x63bf4`, below the configured storage boundary `0x65000`, so this prototype does not need S140 reclamation. The reclaimed-layout comparison also builds. Neither establishes the attached receiver's actual partition layout or runtime behavior.

The protected receiver's audited foreground pairing chain uses 10,740 bytes of its 48,400-byte nominal stack gap, leaving 37,660 bytes before interrupts. The left with the required Rynk feature builds, but its audited chain leaves only 2,500 bytes before interrupts. These are binary-analysis results, not complete stack bounds or hardware acceptance. Both retained crypto wrappers pass ten published ECDH vectors in emulation; that does not validate the radio link. The left feature-expanded candidate needs further stack review before a trial.

## Bootloader evidence and owner recovery exception

The owner subsequently requested proceeding without a proven independent recovery route. This overrides the repository's recovery requirement for the receiver trial, but does not establish a recovery mechanism. Failure may require opening the receiver and using a debug probe. No receiver write has occurred under this exception.

Correlate the physically attached receiver because factory left/receiver descriptors can match. If an existing factory entry route is available, root can separately scope that reset request, read bootloader identity, and save this receiver's complete readable image and application restoration material. A CDC descriptor alone does not prove that a 1200-baud touch works.

Before any replacement, establish an application-independent recovery route on this exact receiver that also permits normal USB cold startup. Primary sources reviewed here do not establish such an enclosure-closed route. An accessible external reset control might provide one, but its existence and bootloader behavior remain unverified. Ordinary USB replug, a host bus reset, or RMK's runtime DFU cannot be assumed to substitute for it.

Without the owner's exception, no separate route would block flashing under the repository's recovery requirement. The exception does not waive target-specific bootloader evidence, factory backup or image address/vector/family checks. Keep the working RMK halves intact. Do not solve this by replacing the bootloader, reclaiming RAM, changing UICR, adding speculative GPIO code, or reusing a keyboard-half marker image.
