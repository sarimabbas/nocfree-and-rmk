# Left application-entry heartbeat candidate

Prepared 2026-10-01. **Not installed; new hardware trial approval pending.** This first instrumented image tests the application entry stage only. It does not type, initialize clocks, start USB, or run radio tasks. The failed full USB startup remains undiagnosed; see [previous trial](left-migration-trial.md).

## Single change and source evidence

Explicit feature `migration-entry-probe` implies the existing `migration-probe`. Only that feature replaces the default `__pre_init` hook with register-only assembly. Cortex-m-rt sets VTOR first, then calls this hook before data/BSS initialization. The hook writes POWER.GPREGRET `0x57`, issues DSB, preserves AIRCR.PRIGROUP and requests SYSRESETREQ, issues DSB, and waits for reset. It accesses no stack, initialized RAM, GPIO, clock, interrupt mask, watchdog or flash. It neither replaces the bootloader nor uses persistent MBR commands. The ordinary USB probe path remains unchanged when this feature is absent.

The exact upstream [bootloader main](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c) interprets GPREGRET `0x57` as a USB MSC+CDC update request. The [Nordic device header](https://github.com/NordicSemiconductor/nrfx/blob/v3.0.0/mdk/nrf52833.h) defines POWER.GPREGRET at `0x4000051c`; [cortex-m reset implementation](https://github.com/rust-embedded/cortex-m/blob/f259e379457008329659c3b1207f61316c35b6f0/cortex-m/src/peripheral/scb.rs) supplies the AIRCR reset sequence. Version-specific prerequisites, RAM/watchdog hazards and interpretation limits are in [the observation plan](startup-observation-plan.md).

Requested DFU can time out after three seconds without USB and re-enter this probe, forming a reset/DFU loop until USB mounts. It is not guaranteed to park indefinitely on battery. During a staged trial, stop and restore if the bootloader does not mount stably.

## Reproduce and artifact bounds

From `firmware/`:

```sh
cargo build --locked --release --bin recovery-probe --target thumbv7em-none-eabihf --no-default-features --features left,migration-entry-probe
```

The repeatable harness includes this image: `./scripts/check.sh --reclaimed-softdevice --usb-recovery-first`. The protected factory-layout full left still exceeds its slot; this feature does not resolve that build's size limit.

| Property | Value |
|---|---|
| Role/features | `left,migration-entry-probe` |
| BIN size/end | 14,536 bytes / `0x48c8` exclusive |
| UF2 range | `0x1000..0x5000` exclusive; 64 blocks |
| Touched pages | `0x1000`, `0x2000`, `0x3000`, `0x4000` |
| SP/reset vector | `0x20020000` / `0x1205` |
| Entry hook | `0x1268` |
| Recovery marker | `0x1200 = 0x87eeb07c` |
| BIN SHA-256 | `971709b24746fb79825accd50bea733d05d7163c50ec3a25fa4f3ec13614106a` |
| UF2 SHA-256 | `ecccaab35492df0bbb3b21cf3b7abd825165d992fcb8665837615a57ce421570` |

Generate ordinary nRF52833 application-family addressed UF2, with FF padding through only the last touched page. The migration guard binds it to the exact BIN, vectors, marker and absent S140 magic. Independent ELF PT_LOAD reconstruction matches BIN exactly. Compared with default Arm `objcopy -O binary`, only the four unallocated alignment-gap bytes at `0x4874..0x4878` differ: this package fills them with erased bytes (`0xff`) while objcopy defaults to zero. All file-backed bytes match. Independently inspected disassembly shows Reset writing VTOR `0x1000`, then calling the hook before BSS/data initialization. Those are software checks, not observed board execution.

The final harness passed all 42 Python safety tests, seven scanner tests, six production role/keymap builds and every diagnostic variant. Independent agents reviewed the source, generated early-entry instructions, exact ELF/BIN/UF2 binding, and original restore image. These remain software results.

Keep the exact original private left restore container SHA-256 `88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100`, independently validated by the existing restore policy. Its complete readable restoration and normal factory startup passed in the previous trial. Generated images and identifiers remain ignored.

## Proposed controlled hardware trial

1. Confirm the connected left factory application and enter its factory Fn+5 recovery. Require exact original full readback and bootloader identity before transfer. Leave right and receiver unchanged.
2. Install only this guarded addressed UF2. Use independent WIRED USB-first recovery and verify exact candidate/page-tail/readable-retained bytes. A mounted drive at this point proves recovery, not application entry.
3. Use a staged battery-first launch: remove USB in middle WIRED, verify disappearance, switch to Bluetooth with USB absent and wait ten seconds, then attach USB leaving Bluetooth. Record time and whether stable MSC appears. Do not issue an application reset command to manufacture this observation.
4. Verify the exact candidate readback if MSC appears; stop on unstable or absent enumeration. Interpret results with the limitations below.
5. Restore the original complete factory container, verify normal factory enumeration, re-enter factory Fn+5 for exact complete readback, then return to normal factory operation. No automatic candidate retry or next-stage installation is included.

## Interpretation limits

The intended signal is an application-induced return to update mode before Embassy initialization. A late USB drive following the controlled battery-first launch is **consistent with** reaching the hook, but is not proof by itself: USB-first marker recovery, invalid application-bank metadata, vendor changes or an incomplete power transition can also yield DFU. CURRENT.UF2 does not expose the bootloader settings bank. Treat the earlier full USB probe's failed enumeration as context, not a deterministic control measurement.

An absent drive leaves application handoff/reset/DFU behavior unresolved; it does not prove failure before a specific instruction. A drive appearing cannot validate HAL clocks, interrupts, USB runtime or BLE. Compare source-backed expectations and controlled observations before proposing a separately reviewed runtime/HAL stage image.

Independent recovery and full restore have device evidence from the earlier layout trial. Interrupted updates and this new reset-loop behavior remain untested; a failed write or vendor-specific behavior could still require opening the enclosure or a probe. No automatic rollback is claimed.
