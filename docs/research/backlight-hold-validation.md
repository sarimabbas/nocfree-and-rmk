# Backlight hold-repeat candidate validation

The active-high left and right packages passed independent offline image and known pairing-stack checks. Hold-repeat behavior is not installed or hardware validated by this review. The owner previously confirmed visible dimming and left typing with the separate 400 Hz firmware; that observation does not validate this candidate.

## Image checks

The independent parser reconstructs the exact ELF32 physical LOAD bytes and zero-filled BIN gaps, then parses every UF2 block. Checks cover contiguous addresses, numbering, nRF52833 family `0x621e937a`, application range starting at `0x1000` and ending before settings at `0x65000`, RAM/vector bounds, the recovery marker at `0x1200`, absence of S140 magic, FF final-page padding and package-manifest hash bindings. The parser imports neither the exporter nor project image guards.

| Role | BIN bytes | UF2 range | ELF SHA-256 |
|---|---:|---|---|
| left | 383036 | 0x1000–0x5f000 | `90765bf5a1d7a6970f28aaacd4fc58e4d771fc716d8c85cd6f26b49f1f6d33b6` |
| right | 217828 | 0x1000–0x37000 | `b33935a214b77005e7c5f95ac66bba317caad125a0208e70696f679ba0ec7a27` |

## Known pairing stack chain

Fresh exact-image Trouble `SecretKey::dh_key` emulation passed ten immutable ECDH vectors per candidate, including output and callee-saved register checks. ARM disassembly supplied prologue sizes and actual direct BL links through the known pairing foreground chain. The Cortex-M runtime main, `main` and Executor outer frames total 32 bytes. No prior vector results were reused as new-image evidence.

| Role | Static gap | Known foreground depth | Remaining before IRQ/other paths | Margin change vs working 400 Hz |
|---|---:|---:|---:|---:|
| left | 35184 | 26528 | 8656 | -24 |
| right | 59960 | 21592 | 38368 | 0 |

The left known-chain margin passes the existing 8,192-byte review floor. This narrow foreground estimate is not a whole-program stack upper bound, interrupt nesting allowance, WCET or latency measurement.

## Evidence and acceptance limits

Private repeatable evidence is retained under `.evidence/backlight-hold-package-review/`: independent parser, exact-image disassembly, new wrapper-vector runs and hash-bound review JSON. The review performs no device operations and does not identify a connected board, prove recovery or authorize flashing. Only active-high packages were independently parsed in this pass.

The later owner observations below cover tap, hold, release and typing. Persistence, lighting after split reconnect and loss/release/wake acceptance remain separate observations.

## Owner-assisted left result

The scoped left-only installation passed exact application/padding/gap readback and USB-first recovery. Settings reinitialized as disclosed. Normal USB startup was observed at the approved left location. The owner reported “works perfectly” after checking one-step tap, held up/down brightness ramps, stopping after midpoint release, off endpoint and typing `qwert` while holding F6. This establishes those functional owner checks, not a measured input-latency or loss bound. The right and receiver firmware were unchanged.

## Right owner trial

The approved right-only update passed exact application and page-tail readback, unchanged protected-gap verification and independent USB-first recovery. Runtime settings changed as disclosed. The left and receiver firmware stayed unchanged. With right USB unplugged, the owner confirmed synchronized brightening/dimming, held ramping and stop after midpoint release, responsive right typing during a left brightness hold, and cross-half Shift. The owner reported a slight right-side lighting delay. These are functional observations, not measured latency or loss bounds. The owner subsequently confirmed that restarting only the right restores the left’s current brightness without another brightness-key press. Saved brightness across a left restart remains pending.
