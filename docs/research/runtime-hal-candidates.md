# Left runtime and HAL stage-return candidates

Prepared 2026-10-01. **Neither image is installed or approved for a new hardware trial.** The left remains restored factory firmware, the right its Mac diagnostic, and the receiver untouched. The [entry-only trial](migration-entry-trial.md) supplied observations consistent with early entry, not proof of execution or a diagnosis of the silent full USB probe.

## Two separate stopping points

`migration-runtime-probe` returns at the first poll of the existing async main task, before creating HAL configuration. This includes Cortex-M RAM/FPU initialization and Embassy executor/task startup. It does not test clock initialization.

`migration-hal-probe` returns immediately after the existing `embassy_nrf::init` call and before USB driver construction. It keeps the same ExternalXtal HFXO, default LFCLK, HAL configuration and executor path. This stage tests the whole initialization boundary, including HF/LF clock waits, time driver and GPIOTE setup; it cannot distinguish a particular clock from other initialization failures.

The helper writes volatile POWER.GPREGRET `0x57`, executes DSB and calls the existing Cortex-M system-reset implementation. No logging, watchdog, GPIO indicator, interrupt-mask normalization, flash logging or persistent MBR commands were added. The ordinary USB and early-entry variants retain their behavior. Compile guards forbid simultaneous stage features, wrong roles and using the input diagnostic with the reclaimed layout.

Source prerequisites and GPREGRET/AIRCR evidence are recorded in [the observation plan](startup-observation-plan.md). Requested DFU can timeout without USB and form a reset loop until USB mounts; it is not guaranteed to park indefinitely on battery.

## Linked erased extent and runtime checks

The stage images compile to less than the span needed to overwrite the old S140 detection word. `migration-diagnostic.x` therefore places an explicit read-only erased word at `max(text end, 0x3004)` **after `.text`**, followed by rodata and data load bytes. This makes the actual ELF cover `0x3004`; the migration guard is unchanged and no unlinked tail is appended to satisfy it.

Independent review caught an initial `INSERT AFTER .data` placement which changed Cortex-M's external `__edata` symbol to a flash address. That draft was never installed or packaged for a trial. Corrected placement preserves RAM data/BSS bounds, and new linker assertions enforce both ranges. Fresh corrected ELF disassembly verifies Reset copies exactly 80 bytes of initialized RAM data and zeros only its proper BSS range. Independent reconstruction of file-backed ELF LOAD segments, with erased-byte gaps, must match each exact BIN before approval.

| Property | Runtime | HAL |
|---|---|---|
| Features | `left,migration-runtime-probe` | `left,migration-hal-probe` |
| BIN size/end | 8,296 / `0x3068` exclusive | 8,328 / `0x3088` exclusive |
| RAM data range | `0x20008000..0x20008050` | same |
| Data load address | `0x3018` | `0x3038` |
| RAM BSS range | `0x20008050..0x200082f0` | `0x20008050..0x200082f4` |
| BIN SHA-256 | `97b5d7294ff92afc60082153b02e39db5e4cdd39b01661a5f891050d0c2d7ada` | `3ea5e0ba29f7dd1ce807f2ef276c0a18cf3c70681bdf5ee33aee6c86b4b065a8` |
| UF2 SHA-256 | `4cf93bb6cd472ba5400f886414ce984cbc313a0323afabdc7e39773dfe7769a4` | `f229a224135a5f2d2b73a24382d2705e88ef650140f649dc893cd354999ba2f8` |

Both use ordinary nRF52833 application-family UF2 with 48 contiguous blocks covering only `0x1000..0x4000`. Only pages `0x1000`, `0x2000`, `0x3000` are touched. Both SP=`0x20020000`, reset vector=`0x1205`, recovery marker=`0x1200:87eeb07c`, old S140 word=`0x3004:ffffffff`. Page tails are FF padded. Generated binaries and private package builders remain ignored under `dist/` and `.evidence/`.

## Repeatable software verification

From `firmware/`, build each with `cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf --no-default-features --features left,migration-runtime-probe` or `left,migration-hal-probe`. Run `./scripts/check.sh --reclaimed-softdevice --usb-recovery-first` for host tests, every supported production role/keymap and every diagnostic. The default-layout full left remains oversized. Use `inspect_migration(uf2, bin)` for both candidates and the separately pinned `inspect_left_factory_restore` for the original saved factory container.

Final software verification passed: 42 Python safety tests, seven Rust scanner tests, all six production role/keymap builds and every diagnostic including both new stages. Independent review verified the corrected Reset/data/BSS instructions, exact ELF-to-BIN/UF2 bindings, vectors, recovery marker, erased extent and saved restore artifact. These are software checks, not hardware startup validation.

## Proposed single controlled trial

After new approval, verify the connected left factory identity, correlated Fn+5 bootloader and exact original full readback. Validate both candidate artifacts and the saved original restore SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100` before any transfer.

1. Install runtime stage. Verify independent recovery, exact BIN/page tail and unchanged readable addresses from `0x4000` onward. Confirm USB disappearance in middle WIRED before switching to Bluetooth while unplugged. Wait ten seconds, attach USB and record stable DFU presence/absence.
2. Proceed to the HAL image only if the runtime stage returns to stable, exactly verified DFU and the observed sequence was valid. Otherwise restore factory and stop. Verify the current runtime image again immediately before replacing it; never identify a half solely from the shared bootloader volume name.
3. Install HAL stage and repeat the same recovery/readback and controlled launch observation. Stop on any unexpected behavior. Each stage writes the same three pages; compare retained readable bytes to the original factory pretrial image.
4. Restore the original complete factory UF2 once at the end (or at the first failed gate). Require factory enumeration, exact complete readable restore hash and S140 metadata, then final normal factory startup. No further stage image, full RMK, right or receiver write is included.

A mounted drive is only consistent with reaching a return boundary: marker-driven DFU, bank validity (not readable through CURRENT.UF2), vendor behavior or incomplete power transitions remain alternatives. Equal addressed spans, block counts, SP/reset vectors and controlled copy/launch sequences reduce avoidable differences but do not expose bank settings. If runtime returns and HAL does not, the result narrows the stopping point toward HAL initialization; it is not a proven clock diagnosis. If both return while the full USB probe does not, USB driver/run/wakeup stages remain to investigate.

Previous trials prove this unit's independent recovery and exact full readable factory restore. New stage reset loops and interrupted updates still have no hardware guarantee; an exceptional failure could require opening the enclosure or a debug probe. No automatic rollback is claimed.
