# Full left Mac RMK trial candidate

Prepared 2026-10-02. Built, guarded and independently reviewed; not installed or hardware validated. The prior configuration-return diagnostic still failed after the source-supported SET_ADDRESS correction, and was recovered/read back exactly. This candidate tests the actual left RMK path, including its scanner, MPSL/SDC, USB, BLE and storage startup. It must not be represented as a known fix or attributed solely to scanner wakeups.

## Exact artifact

Clean source `387b4e8a440dabaac9146b36d07dbc7f67959a15`; patched framework `9640cd7af9d2e5f9fe9860aae7108e4a04f96c8c`. Locked release build `nocfree-rmk`, target `thumbv7em-none-eabihf`, features `left,reclaimed-softdevice,usb-recovery-first,mac-keymap`. No newly implemented board USB driver or timer workaround.

| Artifact | SHA-256 |
| --- | --- |
| ELF | `d3ac2feb4266872e8f82bc7694d0da738e1d8976b33e63ef6ad503d1e2a8b2d3` |
| BIN | `a3178f6ca010c780fa09a951620ae94602e1a7ea356b02d8f326ea424a37ba1a` |
| UF2 | `fff02b987342238d889653698e9f3f85598a8b08962dddf44cf9b65b75887e72` |

Ignored local package `dist/full-left-fixed-mac-recovery/` contains the exact artifacts and manifest. BIN is 340,188 bytes through `0x540dc`; UF2 is 1,344 blocks through `0x55000`, with 3,876 erased tail bytes. Family `0x621e937a`, SP `0x20020000`, Reset `0x1205`, USBD vector `0x13f1`, recovery marker `0x87eeb07c` at `0x1200`. Independent ELF reconstruction, UF2/BIN comparison and canonical migration guard pass. RAM allocations preserve the low 32 KiB, with static allocations ending at `0x20017c30`, leaving 33,744 bytes below SP; this does not measure peak stack or runtime radio correctness.

The repeated harness passed all six production role/keymap builds and existing diagnostics with the corrected dependency. This additional full-left recovery-marker variant cross-built successfully. No hardware acceptance for split, pairing, battery, backlight, latency or missed/released keys is established.

## Proposed write scope and acceptance policy

A new full-image approval must cover both the larger installation and runtime storage. Installation writes application pages `0x1000`–`0x55000` exclusive. RMK storage at `0x65000` uses eight 4 KiB sectors through `0x6d000`, and can erase/write them during startup. The gap `0x55000`–`0x65000` must remain unchanged. MBR, bootloader and UICR are outside permitted writes. The linker comment about preserving filesystem describes image coverage only; it must not be used to claim runtime storage remains untouched.

Before installation, require a fresh unique same-device bootloader identity and exact retained configured-diagnostic baseline, save the entire exposed readback, reapply image/restore guards, and pin the above scope in a new operator policy. No automatic transfer/retry, dock action, serial-port open, right/receiver write, or heartbeat. Keep the saved factory restore and current diagnostic available; no automatic factory restoration.

Use two distinct readback gates:

1. Initial USB-first recovery, before battery launch: require exact full BIN and erased tail through `0x55000`, and **all** remaining exposed bytes `0x55000`–`0x6d000` unchanged against the preinstall snapshot. This verifies the full image's own independent recovery instead of assuming the tiny diagnostic result generalizes.
2. After owner-assisted startup and typing observation: return independently to recovery and require exact application bytes and unchanged protected gap. Save and classify changes only in the allowed RMK storage interval; do not report changed storage as retained original. Stop if any other exposed byte changed or identity mismatches.

A successful startup needs stable expected full RMK HID interfaces and owner-confirmed left-key typing. A build, callback return or host device identity alone is not sufficient. The unchanged right diagnostic cannot validate a full split link. Retain the approved candidate checkpoint only after final recovery/readback passes; any additional startup to leave typing enabled is a physical handoff after those checks.

## Approved trial checkpoint: installed and independently recovered

On 2026-10-02 the owner explicitly approved the full application and runtime storage scope above. A fresh same-device bootloader identity and exact retained configured-diagnostic baseline passed before the one-shot full-image transfer. Companion recorded a genuine USB-first physical cycle; the full image's recovery readback matches its exact binary and erased tail, and the protected gap is unchanged. The initial strict all-remaining-bytes gate nevertheless **failed**: exactly 20 bytes changed at `0x65000`–`0x65014`, before the planned battery-first observation.

Two independent offline reviews decoded those bytes as a valid sequential-storage page marker, item header and RMK StorageConfig record. Length and data checksums verify, and the schema hash matches the pinned RMK commit and this candidate's exact features. Pinned RMK `Storage::new` writes that schema when its configuration is absent or incompatible. This supports execution reaching RMK storage initialization; a transfer-triggered automatic launch is plausible, but its timing was not captured. This is not USB, typing, BLE or split acceptance.

The failure remains preserved in the private evidence. A separately reviewed, read-only exception accepts only the exact observed readback and install-baseline hashes, same session/source/candidate, and these exact 20 storage bytes; it admits no arbitrary storage change or new write permission. Nine mocked exception checks passed. A fresh same-device readback then matched that bounded exception. Storage copies and the original physical acknowledgement are preserved; no fresh physical acknowledgement or timestamp was fabricated.

Companion advanced to the approved battery-first observation after that review. USB enumeration, owner typing and final recovery/readback are still pending at this checkpoint. No factory restoration, additional firmware transfer, right/receiver write, dock action, serial-port open or heartbeat occurred. Private evidence and device identifiers remain outside Git under `.evidence/full-left-trial/`.
