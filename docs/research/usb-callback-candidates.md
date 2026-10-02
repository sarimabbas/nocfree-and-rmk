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
