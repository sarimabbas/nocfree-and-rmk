# 400 Hz backlight candidate validation

The active-high left and right packages passed independent offline image checks. This supports a scoped hardware trial; it does not establish that the new frequency produces visible dimming.

## Image checks

An independent Python parser read ELF32 program headers directly, reconstructed the zero-filled GNU binary gaps, and compared every physical LOAD byte with the BIN. It then parsed each UF2 block and checked contiguous addresses, block numbering, family `0x621e937a`, Thumb reset vector, RAM stack bounds, recovery marker at `0x1200`, absence of the old S140 magic, and FF padding through the final 4 KiB page. Package manifest hashes match the files. No exporter or project image-guard module was imported.

| Role | BIN bytes | UF2 range | ELF SHA-256 |
|---|---:|---|---|
| left | 382468 | 0x1000–0x5f000 | `f29d88232baff359b21985d1ed7fffb31398087106e2e4d961a86262c61283da` |
| right | 217812 | 0x1000–0x37000 | `1e1cfe6348b2f7d48339a0986ed61d94ee886880994dc9dc8c775effebca190b` |

Both ranges finish before settings at `0x65000`. Numeric image structure does not establish device identity, prove a recovery route, or authorize a write.

## Known pairing stack chain

The exact linked Trouble `SecretKey::dh_key` wrapper was emulated against ten existing immutable ECDH vectors for each package. Outputs and callee-saved registers passed; observed wrapper peak remained 1,820 bytes. Actual ARM disassembly supplied prologue sizes and direct BL links from the nearest Embassy TaskStorage polling function through the known pairing path. The outer `main`, Cortex-M runtime main and Executor frames total 32 bytes.

| Role | Static gap | Known foreground depth | Remaining before IRQ/other paths | Change in remaining margin vs 8 kHz |
|---|---:|---:|---:|---:|
| left | 35200 | 26520 | 8680 | -96 |
| right | 59960 | 21592 | 38368 | -96 |

The left known-chain margin is 8,680 bytes, above the existing 8,192-byte review budget. This is a narrow foreground chain estimate. It is not a whole-program stack bound, interrupt nesting allowance, WCET analysis, latency measurement, or hardware validation.

## Evidence and remaining acceptance

Private repeatable evidence is retained under `.evidence/backlight-400hz-review/`: the independent parser, exact-image disassemblies, vector results, and hash-bound review JSON. No device was accessed by this review. Only the active-high packages were independently parsed in this pass; root separately reports successful host tests and the broader role/keymap build matrix. Those root checks are software evidence, not observations by this reviewer.

The next physical observation must distinguish nonzero brightness levels and confirm off, ordinary typing and the retained recovery path. Right-half lighting, split synchronization, persisted brightness and reconnect behavior remain separate hardware acceptance items.

## Root software checks

Root reran the safety/scanner harness (47 Python safety tests and seven Rust scanner tests) and all 277 RMK tests. `scripts/check.sh --reclaimed-softdevice --usb-recovery-first --backlight-active-high` passed all role/keymap and probe cross-builds. Both active-low halves with both keymaps, feature-off halves with both keymaps, and the actual protected receiver configuration also cross-built successfully. The receiver has no lighting feature. This does not resolve the known protected-layout full-left size overflow or establish any new hardware result.

The framework revision changes RMK's storage schema hash. The next half update can reinitialize settings and bonds; preserve a fresh readback and expect pairing again. No incompatible settings restoration or schema bypass is included.

## Owner-assisted left result

The installed left image matched the exact candidate, FF page padding and untouched application gap. USB-first recovery returned the readable update drive. Settings reinitialized as expected. After normal battery-first startup, USB identity matched the left location; the owner typed `qwert` and reported that three F6 presses visibly increased brightness and three F5 presses visibly decreased it back to off. This validates the requested left-only dimming loop. It does not validate every nonzero level, waveform, right lighting, synchronization, persistence or latency.

On a later Fn+Escape entry, the owner observed that the left backlight turned off in recovery. A fresh readback again matched the working 400 Hz application and padding and saved its current settings before the hold trial.
