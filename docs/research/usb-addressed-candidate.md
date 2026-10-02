# Left USB address-stage diagnostic

Prepared 2026-10-02. Software preparation only; not installed or hardware validated. The heartbeat was deleted at the owner's request. The last verified USB-reset diagnostic remains the retained checkpoint; no firmware, serial, dock, switch, or cable operation was performed during this preparation.

## Question and interpretation

The previous reset callback established that software observed USBRESET. It did not establish interrupt delivery. This candidate changes only the standard Embassy callback: `Handler::addressed` requests the existing serial-only bootloader route using GPREGRET `0x4e`.

The pinned framework invokes this callback after dispatching a legal SET_ADDRESS request and before its status acknowledgement. A positive observation would establish that boundary, not address commitment, complete descriptors, or correct USBD wake delivery. A negative result leaves EP0 setup, descriptor DMA, and interrupt wakeups unresolved. See [the source audit](handoff-source-audit.md) for primary-source links, the scanner-driven polling confound, and why neither Voyager's CPU-mask changes nor periodic USB polling is yet justified.

## Exact candidate

Source commit: `2a8431a695e049714b1f2a5856383572dd221478`, clean tracked tree at packaging. Build: locked release, `thumbv7em-none-eabihf`, `recovery-probe`, features `left,migration-usb-addressed-serial-probe`.

| Property | Value |
| --- | --- |
| ELF SHA-256 | `f1fef03ec44708781616d99033bcc45b541b18539ee9e1689c7a4a10c25e805c` |
| BIN SHA-256 | `61078518540a6d2ff358e041ef7730d4bbe75349f0446a3c6bb487c7b8b192a8` |
| UF2 SHA-256 | `33aa167cadec59b40a9025e4d5a5c8a6f0722fcaa6b441b40ff3207b8d1980c8` |
| Linked binary | 14,768 bytes, `0x1000`–`0x49b0` exclusive |
| UF2 extent | 64 blocks, `0x1000`–`0x5000` exclusive; erased page tail |
| UF2 family | `0x621e937a` |
| Initial SP / Reset | `0x20020000` / `0x1205` |
| RAM envelope | `0x20008000`–`0x20020000` exclusive |

Local ignored package: `dist/migration-usb-addressed-serial-probe-left/`, with ELF, BIN, UF2 and `prepackage.json`. These generated files and the factory restore remain outside Git. The application marker is at `0x1200`; the old S140 detection signature is invalidated (the word at `0x3004` is code, `0x2803b1c8`, rather than the S140 magic). Packaging passed the canonical migration address/vector/family guard. This lower application layout replaces part of S140; it must never be mistaken for the protected `0x27000` typing diagnostic. Its address extent excludes MBR, bootloader and UICR, but those protections alone do not prove recovery on a connected device.

## Software checks

- `./scripts/check.sh --reclaimed-softdevice` passed before and after the change: 42 Python tests, five scanner tests, two doctests, formatting, six full-firmware role/keymap cross-builds and all diagnostic cross-builds.
- Fourteen invalid feature combinations were rejected: pairing this stage with each other stage, and five invalid role selections. The positive build passed after adding both linker-selection and Cargo rebuild guards.
- Independent source review passed after catching and correcting the missing linker-safety entry. Independent exact artifact review passed: reconstructed ELF load segments equal the BIN; every UF2 payload and erased page-tail byte matches; hashes, vectors, family, marker and restore guard pass. Disassembly confirms the addressed vtable slot reaches the `0x4e` GPREGRET write, DSB and system reset. The diagnostic linker extent and data/BSS RAM assertions match the linked image.

These are host/build results. The ordinary S140-preserving full left image still exceeds its available flash. No full RMK left startup, BLE, split, dongle, battery, backlight, or latency acceptance is established by this candidate.

## Morning handoff

No physical steps are required while the owner is asleep. On reconnection, verify the current left identity and its retained reset-diagnostic readback before proposing a trial of this exact candidate. Reuse the enclosed-board recovery route already proven on this particular left half; do not toggle dock power. A source build and saved restore are not substitutes for that device evidence.

Prepare a separate reviewed, manual-only operator policy for this image and obtain concrete device-specific trial approval before installation. Keep the right half and receiver unchanged. The existing factory restore remains available, but an intermediate factory restore is not necessary merely to change between reviewed diagnostic images when exact current readback and independent recovery gates pass.

The candidate trial should check independent recovery and exact installed/retained bytes, then battery-first startup and stable serial-only observation, followed by final recovery/readback. Advance observable steps automatically; prompt only for physical actions the app cannot infer. Retain the approved diagnostic checkpoint rather than automatically restore factory. If the address callback passes, the next question is progress through its acknowledgement/configuration boundary; if it fails, review direct interrupt/setup observation before adding a production workaround.
