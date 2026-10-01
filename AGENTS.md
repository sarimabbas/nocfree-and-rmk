# NocFree AND RMK

Keep RMK responsible for key behavior, debounce, BLE, USB, and storage. Custom code belongs only at the board scanner seam or in the image safety tools. Do not duplicate framework behavior.

Use primary source evidence for pin, memory, and radio claims. Record assumptions separately from verified device observations. Never call a host test or a successful cross-build hardware validation.

Before changing transport or scanner behavior, run the repeatable host harness and cross-build all supported roles. Treat disconnect, release recovery, simultaneous input, and wake latency as required hardware acceptance tests.

Do not flash, erase, unlock, change bootloaders, write UICR, or use probe-rs recover. Firmware images must pass the address/vector/family guard and require device-specific bootloader evidence and a proven recovery route. No generic `cargo run` flash runner.

Use subagents for independent implementation and test/review passes on substantial changes. Give each agent file ownership; root integrates and commits reviewed checkpoints.

Checkpoint with concise Conventional Commits. Do not commit factory firmware, recovery executables, credentials, local device identifiers, or generated binaries.
