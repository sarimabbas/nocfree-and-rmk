# Installation for newcomers

Research and proposal, 2026-10-01. This agent did not operate devices or write firmware. The owner has approved the controlled left USB diagnostic trial; approval is not a passing hardware result. The full keyboard, radio transports, receiver recovery and SoftDevice migration remain separate gates. Consult the current root-maintained [left trial](../left-input-probe.md), [input diagnostic](../input-probe.md) and [acceptance record](../acceptance.md) before publishing instructions.

## Recommendation

Ship prebuilt, versioned firmware with a short illustrated guide and a small guarded install helper when the actual recovery paths are settled. New users should not need Rust, Git, an Arm compiler, Python or a debug probe for a supported routine update. Keep developer builds separate from installation.

UF2 is the simplest eventual distribution format: the existing Adafruit implementation exposes a removable drive and accepts application UF2 copies. Serial DFU instead uses its modified legacy `adafruit-nrfutil`, which adds dependencies. This is upstream capability, not proof that a particular NocFree role/package has passed an MSC installation trial. Current diagnostic installation evidence is primarily serial DFU; the previously tested right factory restore used MSC. Validate the actual production UF2 route before recommending it. [Adafruit bootloader documentation](https://github.com/adafruit/Adafruit_nRF52_Bootloader#readme), [right trial record](../recovery-probe.md).

A raw drag-and-drop guide cannot enforce our guards or stop a user copying the right image to the left. Use manual UF2 as the documented recovery fallback. A future helper should select and validate the image, preserve a local restore package, and perform the same proven copy/verification steps. It should reuse the image-safety module; it does not require new firmware behavior or a replacement bootloader.

## What one release should contain

Publish one ordinary-user download for the validated board/layout and compatible three-role firmware version. Use unmistakable filenames such as `nocfree-and-ansi-<version>-left.uf2`, `...-right.uf2` and `...-receiver.uf2`. Do not label a USB-only diagnostic as complete keyboard firmware. Do not put experimental layouts or bootloader binaries beside the recommended files.

Include a machine-readable manifest and an offline-readable illustrated guide. The manifest should bind each exact artifact hash to its role, ANSI layout, source commit, pinned RMK revision, application layout, family, flash/erase bounds, marker requirement, compatible bootloader evidence and expected running-firmware identity/version. Record the compatible split/receiver protocol version so updates do not accidentally mix incompatible roles. Reuse existing guard output for memory facts; avoid a second independent validator.

Keep factory firmware, device readbacks, pairing/settings, credentials, serial numbers and Bluetooth addresses out of public release bundles and Git. A user's restore package belongs on their own computer. Their `CURRENT.UF2` is not a full-chip backup: the observed coverage excludes MBR, factory filesystem, bootloader and UICR. A helper must explain that boundary and preserve excluded ranges rather than implying it can repair bootloader damage. [Migration evidence](migration-safety.md).

## Minimal install journey

1. **Prepare.** Show the supported hardware/layout and tested feature list. Ask the user to keep a second working input device available and connect exactly one NocFree component with a data cable. Choose the physical role with a picture.
2. **Identify and save recovery.** Inspect normal USB identity without resetting arbitrary serial ports. Bind the selected role to the observed device, then use only its validated update-entry sequence. Read bootloader identity and save/validate its own backup locally. Display the exact recovery steps and confirm that a restore package is ready before enabling the write.
3. **Install.** Show a plain confirmation: selected half/receiver, current version, new version and whether this is an initial migration. Validate provenance, manifest, image bounds/vectors/family and the selected layout before any write. Write one device at a time; do not run bulk auto-flashing.
4. **Verify.** Wait for the expected firmware identity/version, follow the verified startup sequence, and check representative presses/releases. Where supported, use bootloader readback to verify exact application bytes and retained readable ranges. A completed file copy or disappearing drive is insufficient evidence of success.
5. **Continue or restore.** Install the remaining roles in the documented order, pair them, and run a short guided functional check. On failure, offer the exact local restore image and the already tested independent recovery sequence. Keep the previous working release for later manual rollback.

Role detection deserves special care. Observed bootloader product, volume name and VID/PID are shared across halves (`NocFree &`, `239a:0029`). The generic nRF52833 UF2 family also does not distinguish left, right or receiver: the UF2 specification defines family matching, not this keyboard's role compatibility. Never infer a role from the drive name or silicon family. Normal-mode identities can help, but a transition must be correlated to the selected physical device using verified stable identifiers/topology; unplug/reconnect can invalidate that correlation. If identity is ambiguous, stop and ask the user to disconnect other devices and identify the selected component. Keep any identifier locally. [UF2 specification](https://github.com/microsoft/uf2/blob/master/README.md), [device observations](../device-observations.md).

## macOS, Windows and Linux

The same UF2 images can be distributed for all three hosts; the installer UI and permission/driver behavior need separate testing. Proposed guide differences are small:

| Host | Manual recovery interface | Required installation validation |
| --- | --- | --- |
| macOS | Finder removable drive | Actual copy, USB re-enumeration, startup/recovery, safe helper distribution |
| Windows | File Explorer removable drive | Actual copy, drive removal/re-enumeration, CDC driver if serial fallback is used |
| Linux | Desktop file manager removable drive | Actual copy, mount permissions and serial permissions if fallback is used |

These are planned UX surfaces, not Windows/Linux hardware validation. Do not prescribe disabling OS protections or replacing USB drivers speculatively. Explain a specific permission problem only when reproduced on the tested configuration.

Avoid making a web installer the only path. Chrome's Web Serial API supports desktop macOS, Windows and Linux and requires user selection of a serial device, so a browser wizard is a possible later convenience. It still needs a correct legacy DFU implementation, permissions, role checks and recovery handling; it does not provide this project's existing safety guarantees automatically. A download plus file-manager fallback also works without a browser API. [Chrome Web Serial documentation](https://developer.chrome.com/docs/capabilities/serial).

## Release checks and outstanding gates

Build release artifacts from a tagged, reviewed checkpoint; run the repeatable host harness, cross-build every supported role and guard the exact published artifacts. Publish SHA-256 hashes and a signed provenance record covering the firmware and manifest. GitHub artifact attestations can bind an artifact to its producing workflow and repository; they do not prove device compatibility. Native helper signing and OS acceptance are separate work if an executable is eventually shipped. Checksums alone detect changes but do not establish publisher identity. [GitHub attestations](https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations).

Before a newcomer release, complete role-specific installation/recovery/restore tests, the separate migration guard and migration hardware trial, receiver recovery without a battery-start assumption, all requested transport/features and all three host acceptance runs. Test an interrupted update only with an independently available recovery path; the current marker convention does not establish arbitrary power-loss safety. Current marker diagnostics intentionally enter recovery on USB-first startup, which makes them an awkward everyday install experience. Do not conceal that sequence or advertise automatic rollback; current rollback retains an image on the host. [Update foundation](update-foundation.md), [acceptance gate](../acceptance.md).

The next justified packaging work is a release manifest plus guide built from successful trials. Implement the helper only after the release path is stable, starting with identification, backup and guard checks before adding a write action. No complex installer, browser flasher or bootloader change is needed during this bring-up stage.
