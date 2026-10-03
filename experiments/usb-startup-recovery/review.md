# USB startup recovery: independent review

This is a compile-only experiment. No device validation or installation approval is implied.

The requested final interface is Companion recovery for all three roles, with no keyboard recovery chords and no factory bootloader modification. The existing working chord shim must remain available until the replacement is measured and physically verified.

## Verified source constraints

- Current application entry runs `__pre_init` before initialized RAM. The pinned Embassy nRF USB driver uses initialized atomic wakers, interrupt handlers and async state; it cannot simply be called from that hook.
- The existing nRF52833 fork correctly lets hardware finish SET_ADDRESS. Preserve pinned revision `4e3d8c790cc1ca615bca524af816bc5d3c244bae`; a new USB stage must not recreate the previously diagnosed SET_ADDRESS failure.
- `embassy_nrf::init` takes peripheral singletons and initializes GPIOTE and RTC time state. A separately linked stage may have separate globals, but those hardware peripherals remain configured after an assembly jump.
- `UsbDevice::disable` invokes the driver's `disable`; that implementation only clears USBD ENABLE. It is not an audit of NVIC pending interrupts, RTC1, GPIOTE, clock requests or active DMA buffers.
- Cortex-M `asm::bootload` loads MSP and the reset handler. It does not reset peripherals or clear NVIC state. Calling it alone does not make the next application equivalent to a hardware reset.
- Existing reclaimed application region is `0x1000..0x65000`. Existing protected receiver region is `0x27000..0x65000`. Storage remains `0x65000..0x6d000`. Reserve actual ELF page spans, not a guessed stage allowance.
- The right role currently has no runtime USB device. Do not describe an existing right runtime command as the proposed standalone rescue stage.

Primary implementation references: [USB driver](https://github.com/sarimabbas/embassy-nrf-nocfree/blob/4e3d8c790cc1ca615bca524af816bc5d3c244bae/src/usb/mod.rs), [HAL initialization](https://github.com/sarimabbas/embassy-nrf-nocfree/blob/4e3d8c790cc1ca615bca524af816bc5d3c244bae/src/lib.rs), and [Cortex-M bootload](https://github.com/rust-embedded/cortex-m/blob/v0.7.7/cortex-m/src/asm.rs).

## Physical limitation

A battery-powered MCU that remains running does not re-enter its startup stage merely because USB is unplugged and reconnected. A USB-only receiver cold-starts on reconnection; the halves require a proven reset source. Neither a successful compilation nor a host handshake test proves this source exists.

For a crashed half, either the owner must perform a true power cycle, or independently verified watchdog/reset behavior must bring execution back to the stage. A watchdog adds runtime behavior and must not be silently introduced as a size experiment. Test a deliberate early fault and an interrupt-disabled hang separately; a panic handler alone does not cover both. A brief startup window can expire before Companion connects, so the measured protocol must specify host readiness and window duration rather than promising arbitrary later recovery.

The pinned Embassy watchdog driver explicitly states that a started watchdog cannot be stopped. Before adopting it, prove what happens across entry to the unmodified factory bootloader: a watchdog that remains active without being fed can turn recovery into a reset loop. Do not use a watchdog to paper over a startup-only design without this proof.

## Required handoff review

The current opt-in experiment instead initializes the HAL once, temporarily reborrows USBD for the rescue device before MPSL/RMK, then returns its ownership for production USB. This avoids both a second HAL initialization and a separate vector-table jump. Its teardown still requires review: the driver constructor clears pending NVIC state but does not clear USBD event registers. Mask/unpend/disable is not evidence that stale events, shortcuts and endpoint state are reset before production enumeration.

The pinned driver's DMA copy helpers busy-wait synchronously for END before their async caller yields. Cancellation at a yield therefore does not abandon one of those copy buffers. However, a peripheral stall inside that synchronous loop also prevents the two-second async timeout from executing. That timeout is not a hard bound against driver/hardware lockup.

The HAL's `HfclkSource::ExternalXtal` starts the crystal and busy-waits without a deadline. Selecting it unconditionally would change battery-only startup even when the USB stage immediately skips. The current experiment preserves default HAL clock configuration and acquires a stage-owned crystal request only with VBUS present. `StageClock` observes a preceding HFCLKRUN request, issues its own request only when absent, polls for external/running clock at most 100 times with one-millisecond waits, and releases only its own request on timeout or normal teardown. Clock-source measurement on the actual battery-only board remains acceptance work.

The latest teardown clears endpoint enable bits, SHORTS, all USBD event registers, EVENTCAUSE and EPDATASTATUS write-one-to-clear aggregate sources, and masks/unpends USBD. A new production USB reset establishes the shared driver's ready-endpoint state. Whether the 25-millisecond disconnect is sufficient for reliable host re-enumeration remains hardware acceptance, particularly on the currently connected dock.

If a separate linked startup image is pursued later, choose and review exactly one handoff:

1. Hardware/software reset with a one-shot, reset-reason-bound bypass request. Consume the request before launching RMK, avoid vendor `GPREGRET=0x57`, and prove that the factory bootloader preserves the chosen state. Stale RAM cannot constitute an indefinitely reusable bypass.
2. Explicit peripheral teardown followed by fresh RMK runtime entry. Prove USB disconnection, no live EasyDMA buffers, interrupt masking and clearing, stopped RTC/time state, clock ownership cleanup, valid application vectors and initialized RAM. A call to `bootload` is only the last step.

Do not select the more elaborate handoff merely to avoid measuring the simpler candidate. Neither is established by this document.

## Framework request characterization

Run `cargo test --manifest-path experiments/usb-startup-recovery/Cargo.toml`. Seven tests exercise the actual pinned Embassy USB 0.6.0 `DfuState` implementation and the production request filter with a harmless recording handler. They passed on the host. They demonstrate that `WILL_DETACH` invokes the handler before returning `Accepted`; a reset in that handler precedes `ControlPipe::accept`, so a successful host acknowledgment must not be promised. The underlying class handler accepts a wrong interface index and nonempty payload; the actual `usb_rescue_scope.rs` filter now rejects wrong indices, nonzero DETACH lengths and nonempty bodies before framework dispatch. The tests confirm rejected requests never reach recovery and valid DETACH does. Direct direction validation is absent in the handler; the normal USB device dispatch routes direction beforehand.

These are framework characterization tests. They do not prove target enumeration, teardown, timeout enforcement or recovery.

## Meaningful repeatable acceptance

Host policy tests should use the actual shared policy module, after the protocol is chosen. Exercise no request, wrong role/version/token, truncated request, request during enumeration, disconnect mid-request, repeated request, deadline expiry and stalled progress. No generic serial byte or arbitrary device may trigger reset. Confirm recovery is requested only after the complete validated request and its USB status/acknowledgment boundary; immediate reset from a control callback can disconnect before the host receives acknowledgment.

Build all three role images and report stage ELF physical LOAD span, static RAM, stack allowance, each role's shifted application span and storage separation. Guard combined vectors and page padding. These are structural checks only.

On hardware, record ordinary cold start without Companion, recovery with Companion already waiting, unplug/replug while battery remains on, deliberate pre-RMK fault, normal USB-to-RMK enumeration, typing, disconnect release, simultaneous input and wake latency. Measure the added normal startup delay. Preserve exact original bootloader, settings and working rollback images throughout.
