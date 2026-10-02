# USB callback diagnostics after serial-stage calibration

Software preparation, 2026-10-01. These images are not approved or installed. The [serial-stage trial](serial-stage-trial.md) restored the left factory image exactly; the right remains on its existing Mac diagnostic and the receiver is untouched.

## Boundaries and control

The previous pair produced CDC-only recovery both after HAL initialization and after USB construction. Before attributing that signal to an exact hook, calibrate a matched **HAL neutral request**: retain the HAL-serial initialization and reset path, changing only the GPREGRET request from `0x4e` to `0x00`. The existing recovery marker remains. Ordinary MSC rather than serial mode is the expected comparison result, based on the installed bootloader's reported upstream source. Vendor behavior and the physical startup remain device observations to test, not assumptions to silently accept.

The next two candidates register a standard Embassy `Handler` before USB device construction and keep the existing `device.run()`/CDC/update futures intact:

- **Enable callback:** request serial recovery only when `enabled(true)` runs. Embassy calls this after the asynchronous bus-enable future returns. The nRF implementation awaits peripheral READY and power readiness; USB removal can also complete the enable future without connecting its pullup. A return therefore marks enable completion, not necessarily attachment.
- **Configuration callback:** request serial recovery only when `configured(true)` runs. The framework calls this while handling SET_CONFIGURATION after enabling endpoints, before accepting the request and completing its status response. A return marks host request handling, not stable enumeration, CDC transfer or usable input.

False/disable/unconfigure callbacks do nothing. The candidates do not change clocks, normalize interrupt masks, add a custom driver, write flash logs, replace the bootloader or run radio tasks. Primary pinned source evidence and exact callback order are recorded in [usb-startup-boundaries.md](usb-startup-boundaries.md).

## Hardware acceptance for a separately approved trial

Require exact original left readback before the first transfer, each candidate's own independent WIRED MSC recovery and exact binary/page-tail/retained-range readback, and the guide-confirmed battery-first launch. Never repeat an uncertain transfer automatically.

The neutral control must produce stable ordinary MSC with the expected CDC and storage interfaces. Unexpected serial mode or ambiguous behavior invalidates attribution: restore the factory image and stop. Only after that calibration should serial mode from a callback be interpreted as evidence consistent with reaching that callback. Enable completion without a configuration callback narrows investigation to attachment/control handling; absence at the enable boundary leaves READY/power/wakeup unresolved. Neither establishes a root cause.

After the bounded trial, restore the complete original exposed factory container, including S140, verify its exact readback and confirm normal factory USB operation. No new image is authorized by this document.

## Exact packaged candidates

All artifacts were built from clean source commit `391ae896867c2887f38fd50c3b2997a1f05e4aa8`; binaries remain local under ignored `dist/`. Each uses the lower migration origin, standard nRF52833 UF2 family `0x621e937a`, stack `0x20020000`, reset vector `0x1205`, and retained recovery marker at `0x1200`. Canonical address/vector/family and exact BIN-to-UF2 padding guards passed. These structural checks do not authorize installation.

| Candidate | BIN bytes | Touched interval | UF2 SHA-256 |
| --- | ---: | --- | --- |
| `hal-neutral` | 8328 | `0x1000..0x4000` | `3d69e50c5f2a8793e0d3c14e63e6a5e3fce169b7e765aa9cf50a820edc24a14e` |
| `usb-enabled-serial` | 14776 | `0x1000..0x5000` | `9c53b0b7010ec49127e4bd580fb457b50c892332499050b846edfdb7c9ed7da5` |
| `usb-configured-serial` | 14776 | `0x1000..0x5000` | `8cc21df8aae676c1bf3f5dc44731ec1b289ef1e5f067f4c63c225315434f365a` |

The rebuilt HAL-serial reference is byte-identical to the previously hardware-tested binary and UF2. The neutral binary differs from that reference at exactly one byte, address `0x13ca`: request `0x4e` becomes `0x00`. Callback images touch four pages, extending one page beyond the previous three-page trial; their untouched-range check must start at `0x5000`, not `0x4000`. Neither reaches the bootloader or other protected regions.

The repeatable `./scripts/check.sh --reclaimed-softdevice` passed before changes and after integration: 42 Python tests, five scanner tests, two doctests, all six role/keymap builds and the diagnostic matrix. These are software checks; the factory-preserving full left RMK build's previously recorded flash overflow remains a separate unresolved limitation.

All 28 pairs of distinct startup-stage features and 12 wrong-role combinations for the new stages were rejected for the expected compile-time guard reason. Independent artifact review reconstructed ELF load segments into the exact BIN, checked every UF2 payload/padding byte and validated the original restore image. Optimized callback code conditionally returns on false and writes `0x4e` followed by DSB/system reset on true; handler vtable placement selects the intended enabled or configured slot. The ordinary asynchronous READY/power/waker/control paths remain linked. Data/BSS stay within the migration RAM bounds. No actionable source or artifact findings remained.

## Approved trial progress, 2026-10-01

After explicit owner approval, all three candidates were installed in order and each passed independent WIRED MSC recovery with exact binary, page tail and retained-range readback. The neutral battery-first control returned stable MSC with interface classes `[2, 10, 8]` and exact candidate bytes. The enable callback returned stable serial-only recovery with classes `[2, 10]`. The configured callback did not return serial: three settled checks failed that expected mode; a separate read-only snapshot showed the application VID/PID `4c4b:4653`, no product string and no interface descriptors. No serial port was opened. The guide confirmed absence deadlines and switch acknowledgments for each launch.

These observations support the reset-request discriminator and narrow the remaining issue toward host enumeration/control handling after enable. They do not prove the precise failed request or a root cause. A subsequent factory restoration was planned, but the owner explicitly requested retaining this diagnostic between investigations to reduce repeated transfers. It remains installed. A fresh subsequent MSC readback again matched the configured candidate, page tail and retained range exactly. The app had not attested a complete new timed power-cycle sequence, so this check establishes current readable recovery state, not another independent power-cycle proof. The existing original factory restore remains available but has not been performed in this trial.

Read-only `uhubctl` inventory on this Mac found that the hub port hosting the left candidate advertises per-port power switching. No port was toggled. Advertising switching does not establish a real VBUS cutoff: the upstream [uhubctl documentation](https://github.com/mvp/uhubctl#faq) requires physical verification and notes that some hubs only disconnect data. Battery power and the left mode switch remain separate constraints. Power automation must bind the exact keyboard leaf port and avoid switching a hub's upstream port or unrelated peripherals.
