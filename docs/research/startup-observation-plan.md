# Closed-enclosure startup observation

Reviewed 2026-10-01. Research only: no device access, image creation, or writes. The earlier lower-layout image's exact readback and independent recovery passed, while application execution remains unobserved. A silent USB device cannot locate the failure.

## Recommendation

Use separate, minimal stage-return images before introducing a watchdog or relying on RAM surviving the bootloader. Each stage image retains the guarded `0x1000` application origin, `0x1200` recovery marker, absent S140 magic, and existing independent WIRED/USB recovery route. It writes `GPREGRET=0x57`, executes a data synchronization barrier, requests Cortex-M system reset, and waits. Do not call persistent MBR commands, write flash, configure pins, or normalize interrupt masks during this experiment.

Stage A returns from `pre_init`, before data/BSS initialization and before Embassy. Stage B returns at entry to application main, before Embassy clock initialization. Stage C returns immediately after `embassy_nrf::init` completes. Stage D is the existing USB diagnostic with captured state reported through CDC if it enumerates. An observed application-induced return to DFU in A establishes pre-init execution; B additionally establishes runtime startup; C establishes that HAL initialization returned. Failure to observe C while A/B pass narrows the investigation, but does not identify a particular clock. These are distinct guarded artifacts, not an auto-sequencing bootloader.

An ordinary cold USB boot also enters DFU without executing any candidate. Therefore host observation must span the controlled battery-first application launch and subsequent USB connection, with exact candidate readback and identity correlation. An MSC device appearing alone does not prove a stage ran. The closed enclosure provides no direct Reset signal observation, so distinguish this ambiguity in the trial record. An independently tested warm-launch route would improve the stage experiment, but must not be inferred from file-copy completion.

## Existing bootloader behavior

The installed version reports upstream base [0147d71 main.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/src/main.c); vendor changes are unknown. Its `0x57` GPREGRET value requests USB MSC+CDC independently of reset reason. `0x6d` skips DFU. The retained fast-start marker is `0x4ee5677e` at `0x20007f7c`, cleared before application entry. The recovery word requests DFU unless the valid-app fast-start path applies. Requested USB DFU has a three-second timeout, cancelled when mounted; battery-only reset may therefore return to the application repeatedly instead of parking in DFU.

[bootloader.c](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/lib/sdk11/components/libraries/bootloader_dfu/bootloader.c) feeds all eight watchdog reload registers while waiting in DFU if WDT is running. Timeout cancellation checks `tud_mounted()`. This supports a watchdog escape design, but does not prove the vendor binary executes that loop soon enough for an arbitrary deadline.

## Register observations and optional watchdog escape

Addresses below come from [Nordic nRF52833 CMSIS header](https://github.com/NordicSemiconductor/nrfx/blob/v3.0.0/mdk/nrf52833.h). Read CPU masks with `MRS`; PRIMASK, BASEPRI, and CONTROL are not memory-mapped registers. Read snapshots without clearing reset reasons or clock events.

| Item | Address or operation | Purpose |
| --- | --- | --- |
| RESETREAS | `0x40000400` | Record accumulated reset flags |
| GPREGRET | `0x4000051c` | Existing DFU request |
| GPREGRET2 | `0x40000520` | Potential small retained diagnostic tag; vendor use unverified |
| USBREGSTATUS | `0x40000438` | VBUS/regulator state |
| HFCLKSTARTED / LFCLKSTARTED | `0x40000100` / `0x40000104` | Clock event snapshot |
| HFCLKSTAT / LFCLKSTAT | `0x4000040c` / `0x40000418` | Clock source/run snapshot |
| VTOR | `0xe000ed08` | Runtime vector table |
| WDT TASKS_START | `0x40010000` | Start configured watchdog |
| WDT RUNSTATUS | `0x40010400` | Detect an inherited running watchdog |
| WDT CRV / RREN / CONFIG | `0x40010504` / `0x40010508` / `0x4001050c` | Deadline, reload channels, sleep behavior |
| WDT RR[0] | `0x40010600` | Reload with `0x6e524635` |

The [Nordic WDT specification](https://docs.nordicsemi.com/bundle/ps_nrf52833/page/wdt.html) states that timeout resets hardware even without an interrupt handler. Its period is `(CRV+1)/32768` seconds; starting it forces LFRC on if another LFCLK source is not running. Configuration locks once started. Configure counting during sleep, disable timeout interrupt generation, and do not reload during the suspect initialization path. Set the DFU request before that path. WDT reset gives the existing bootloader an opportunity to expose USB; it cannot establish that USB will be attached or enumerate before the bootloader's timeout.

**A watchdog is not observationally neutral:** it starts LFCLK before Embassy, potentially changing the very initialization failure being diagnosed. A success under watchdog instrumentation does not establish that the original clock path was sound. A running inherited watchdog cannot simply be reconfigured; inspect it and reject unexpected state rather than assuming writes take effect. Avoid short deadlines which reset the bootloader before USB or its feed loop starts.

A watchdog-protected image needs a separate controlled test showing that this unit re-enters stable DFU and accepts restoration under watchdog conditions. Until then, only the already-tested physical recovery sequence is demonstrated. Do not advertise a guaranteed timeout-return route.

## Why retained RAM is insufficient by itself

The exact upstream [nrf52833 linker script](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/linker/nrf52833.ld) places ordinary bootloader RAM at `0x20008000..0x20020000`, the double-reset word at `0x20007f7c`, and OTA NOINIT at `0x20007f80..0x20008000`. [nrf_common.ld](https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/0147d71e73b9a2c217f56dbc9877d07bb45d6467/linker/nrf_common.ld) initializes ordinary data/BSS and places the stack at RAM's top. Consequently an application NOLOAD slot inside ordinary application RAM can be overwritten by bootloader data, BSS, heap, or stack despite surviving the application runtime's BSS clearing.

A small candidate slot at `0x20007000` is outside those upstream regions and the first SRAM word used by the MBR fallback. This is an assumption for review, not a verified vendor reservation. No vendor linker map or RAM-preservation observation exists. Application pre-init must use volatile writes to a separately reserved NOLOAD record, with version, stage, complementary magic/checksum, and no initialized statics. Preserve original snapshots rather than overwriting them on each reset. Invalid records mean unknown evidence, not "application never ran."

The current bootloader's CURRENT.UF2 exposes flash, not arbitrary SRAM. A later reader must run without power cycling, use the same reservation, and report the record through working USB. Uploading a working observer can itself traverse bootloader code which overwrites the record. Restoring factory firmware cannot be assumed to provide a reader. Power loss destroys the evidence. A retained record alone therefore cannot distinguish pre-init, clock stall, and USB stall from the host in this closed-enclosure trial.

## Acceptance boundaries

Before proposing a trial: inspect the generated Reset/pre-init instructions, verify vector/marker/image guards, check the stage-return code needs no clocks or IRQs, and independently review the exact artifact and restore image. No known LED or backlight pin should be used as a progress indicator without verified electrical polarity and power gating. The read-only source analysis supplies no such evidence.

The smallest safe next experiment is a pre-init stage-return image with the existing physical recovery fallback. It can establish early execution under a controlled launch, but source review cannot guarantee an externally visible return on the vendor bootloader. Establish this first; only then expand to runtime, clocks, USB, and optional retained records/watchdog. Record firmware build success separately from every device observation.
