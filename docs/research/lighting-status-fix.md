# Lighting and status fix checkpoint

2026-10-04 implementation:

- Companion: one derived status separates switch policy, active typing route,
  USB power, right split link and per-role recovery. Tooltips remove duplicate
  mode/connection wording. Unknown facts are explicit; cached battery survives
  transient reading gaps.
- RMK fork `33214e095a86e21bac6184ab9e4d3565ba72a849`: explicit PWM off,
  disconnect hold cancellation, right battery reconnect darkness, wireless
  searching feedback, 30-second connected indications, and Vial status v2.
- Nordic HAL fork `ec44c260354cfd31f3bb6f0a05f82e7f58e8cdff`: persistent DMA/STOP
  cancellation state, STOPPED handshake, disabled generator when off, and
  explicit actual-start tracking to avoid waiting on a never-started generator.
- Charger red is hardware-owned while USB-powered. Dongle double pulses remain
  distinguishable from Bluetooth's brief steady indication despite optical
  mixing. No unsafe push-pull control of the shared charger output.

Validation: 104 Companion tests plus two integration tests passed (one existing
ignored test); 327 isolated framework tests passed; 78 image/safety host tests
and 23 input harness tests passed. All three production roles cross-built and
passed their address/vector/family guards. LEFT is 372,788 bytes at 0x1000,
padded through 0x5d000; RIGHT is 229,612 bytes padded through 0x3a000; dongle is
248,532 bytes at 0x27000, padded through 0x64000. No bootloader/UICR changes.

Independent implementation and review passes covered the stop/start lifetime,
disconnect controls, telemetry compatibility and tooltip semantics. Actual
PWM shutdown/resume, optical patterns, current savings, typing and wake latency
remain separate hardware acceptance checks. Only the owner-authorized LEFT
candidate is scheduled for this unattended transfer; right and dongle stay
unchanged until their device-specific update steps.

Primary stop/start semantics: [Nordic PWM specification](https://docs.nordicsemi.com/r/bundle/ps_nrf52833/page/pwm.html?contentId=tF4489dcQq9LN2iwRSaljg).
