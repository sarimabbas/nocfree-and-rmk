# Exact left-image startup audit

Offline inspection on 2026-10-02. No rebuild, device write, serial access, reset or port operation was performed. This checks saved candidate artifacts, not live CPU register values. Packaging manifests record pre-trial approval status; hardware observations are documented separately in the trial records.

## Result

The four saved images have correct chip-specific USBD vector entries, explicitly set VTOR to their own linked vector table, share the same RAM origin and stack top, and use the same runtime startup sequence. **No concrete vector, VTOR or static RAM placement mismatch was found.** None explicitly normalizes inherited PRIMASK, BASEPRI, FAULTMASK, CONTROL or MSP during Reset/pre-init. That is a shared property, not a demonstrated regression introduced by migration.

The major confirmed differences are flash origin (`0x27000` versus `0x1000`) and application/interface behavior (RMK typing versus a minimal CDC startup probe). These are not a one-variable comparison: firmware structure, descriptor construction and bootloader dispatch path differ together.

## Reproducible artifact identity

SHA-256 was computed directly from saved files and checked against `dist/<name>/prepackage.json`; binaries and ELF files remain ignored/private.

| Saved directory | Source commit | BIN SHA-256 | ELF SHA-256 |
|---|---|---|---|
| input-probe-left-mac | `2f881225d014920f199d128d4810ecfec8c1e375` | `09614472e29110fd46d1b7d02ad5f2aca058b25526a582fe3c60f5b911d1e38e` | `f507633a6719999bd8a32114d6588c7dc77178e35b44b8f7bf1d13d888403874` |
| migration-probe-left | `1121d39c66d518b3d50e19c637c538c3885008bd` | `bca78013b3a45473ee13c77cfd0e00b2d4f6f917479a66bf6edda56ea1c2d10c` | `d8fd1d89a93854634f04bc87c4b9652c41a269c4368712636f5852c7c0f3287f` |
| migration-usb-configured-serial-probe-left | `391ae896867c2887f38fd50c3b2997a1f05e4aa8` | `97c5d3177b6c6e80fe70fbd5eb17d9cf97fc6450fc8994a61c4f5e60b42c5513` | `06f606285bb11d18c51b9ed7d665ced5c279cab90bf40d21a9170d9ec44b0cf9` |
| migration-usb-reset-serial-probe-left | `589128bd9084c2872bcdbdc8607b8cafa98cdf1b` | `618e65704c38a79f9b93aae99bdafcb2b92198d2505a3e6b9c6b492385e06318` | `fea1e32f3a522abff6bd72baf7405b20de16874b216a55d15b83a52c15c983ee` |

Tool: `/Library/Developer/CommandLineTools/usr/bin/llvm-objdump`. Use `-t candidate.elf` for symbols and `-d --demangle candidate.elf` for instructions. Vector words were independently unpacked little-endian directly from `application.bin`. Temporary complete disassemblies are ignored under `.evidence/startup-binary-audit/`.

## Vectors and Reset

The locked `nrf-pac 0.4.0` chip source `src/chips/nrf52833/pac.rs` declares `USBD = 39`. Cortex-M external IRQ39 is vector index `16 + 39 = 55`, byte offset `0xdc`. Each BIN entry resolves to its ELF global function `USBD`, not `DefaultHandler`:

| Image | Initial SP | Reset vector | USBD vector | USBD symbol / size | VTOR value written |
|---|---|---|---|---|---|
| Working Mac typing | `0x20020000` | `0x27205` | `0x27305` | `0x27304` / `0xe4` | `0x27000` |
| Plain migration | `0x20020000` | `0x1205` | `0x12b9` | `0x12b8` / `0xe4` | `0x1000` |
| Configured callback | `0x20020000` | `0x1205` | `0x12b9` | `0x12b8` / `0xe4` | `0x1000` |
| Reset callback | `0x20020000` | `0x1205` | `0x12b9` | `0x12b8` / `0xe4` | `0x1000` |

Odd vector values correctly encode Thumb entry. Both source binaries bind `USBD => usb::InterruptHandler<USBD>` and `CLOCK_POWER => usb::vbus_detect::InterruptHandler`; the typing image additionally binds TWISPI0 for the scanner. See [working source](https://github.com/sarimabbas/nocfree-and-rmk/blob/2f881225d014920f199d128d4810ecfec8c1e375/firmware/src/bin/input-probe.rs) and [reset diagnostic source](https://github.com/sarimabbas/nocfree-and-rmk/blob/589128bd9084c2872bcdbdc8607b8cafa98cdf1b/firmware/src/bin/recovery-probe.rs).

All Reset functions are `0x44` bytes. Their first instructions load literal `0xe000ed08` (VTOR), load the image's vector-table address and store it. They then call `__pre_init`, zero BSS, copy data, enable FPU with DSB/ISB, and call main. VTOR is written before RAM initialization. This matches the enabled `cortex-m-rt/set-vtor` feature in Cargo.toml and locked runtime `cortex-m-rt 0.7.7` [startup source](https://docs.rs/crate/cortex-m-rt/0.7.7/source/src/lib.rs). None enables `set-sp`; the initial MSP remains a bootloader handoff obligation.

Working pre-init at `0x30c90` and reset diagnostic pre-init at `0x3688` are the same no-op prologue/return (`push {r7,lr}; mov r7,sp; pop {r7,pc}`). The plain/configured variants likewise contain the runtime default, rather than an interrupt-state cleanup hook.

A complete disassembly instruction scan found no `msr basepri`, `msr faultmask`, `msr control`, `msr msp` or unconditional startup `cpsie i`. PRIMASK operations are framework critical-section save/restore and panic paths. In particular, the NVIC critical-section implementation's `cpsie i` is conditional on the saved PRIMASK being clear; it does not demonstrate startup normalization. Its other instructions save/restore NVIC enable banks. Merely finding a `cpsie` instruction somewhere in an image would therefore be insufficient evidence of corrected boot handoff.

## RAM bounds

All images use RAM `0x20008000..0x20020000`, retaining the low 32 KiB boot-marker region. ELF `.data` and `.bss` lie inside this range and do not statically overlap that reserved region or the initial stack top:

| Image | .data | .bss | Data load address |
|---|---|---|---|
| Working Mac typing | `0x20008000..0x20008054` | `0x20008058..0x2000a8e0` | `0x35cf0` |
| Plain migration | `0x20008000..0x20008050` | `0x20008050..0x20008788` | `0x4848` |
| Configured callback | `0x20008000..0x20008050` | `0x20008050..0x20008788` | `0x4968` |
| Reset callback | `0x20008000..0x20008050` | `0x20008050..0x20008788` | `0x4960` |

All starts/ends shown are four-byte aligned; the working image's four-byte padding before BSS is ordinary linker alignment. This is placement evidence, not measured dynamic stack high-water or DMA correctness. [Memory layouts](../../firmware/memory-factory.x), [migration layout](../../firmware/memory-sdc.x).

## What remains unknown

- Live PRIMASK/BASEPRI/FAULTMASK/CONTROL/MSP/NVIC state delivered by this modified vendor bootloader, and whether its two application paths deliver different state.
- Whether the Nordic MBR's forwarding behavior interacts differently with an image whose VTOR is `0x1000`. Correct linked vectors and VTOR alone cannot settle bootloader dispatch.
- Control transfer progress after USB reset, descriptor/EP0 buffers, address assignment and configuration. The successful reset callback proves the framework consumed a reset event, not that the USBD ISR executed: `Bus::poll` can directly read a latched USBRESET event during an initial poll or a poll caused by another waker. The observation remains consistent with zero USBD ISR executions. These static vectors cannot establish live interrupt priority, masks, forwarding or wake delivery.
- Descriptor/interface differences: working input firmware initializes RMK's USB builder after scanner/keymap initialization; the migration probe builds an explicit CDC composite device and its callback variants deliberately terminate at selected USB stages. Both request the external HFXO before constructing the same Embassy nRF driver and hardware VBUS detector. The pair cannot isolate attachment timing or prove that a Voyager cleanup patch is required. Their executor polling also differs: the working typing path joins USB with scanner work driven by a 1 ms timer, whereas the recovery probe has no pre-configuration periodic timer. Such unrelated timer wakes can poll pending USB events; working typing therefore does not itself establish correct USBD interrupt wake delivery or isolate flash layout as the cause.

The next diagnostic should distinguish live handoff state/control progress rather than changing VTOR or moving RAM on the basis of an absent mismatch. This audit supplies no justification for replacing the bootloader or copying GD32-specific Voyager startup code.
