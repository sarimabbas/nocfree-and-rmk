# USB-reset boundary candidate

Software preparation, 2026-10-01. This image is not approved or installed. The left remains on the reviewed configuration-callback diagnostic, recovered to MSC and verified exactly; factory restoration is deferred at the owner's request. The neutral control and enable callback passed, but the configuration callback did not return serial mode. The host showed the application VID/PID without a product string or interfaces. A bounded read-only macOS IOUSBHost log query produced no records; that absence is inconclusive.

## One next boundary

The `migration-usb-reset-serial-probe` feature registers the existing standard Embassy Handler and returns through the unchanged `0x4e`/DSB/system-reset helper from `Handler::reset()`. It changes no clocks, masks, USB driver code or runner behavior. Other handler methods use their defaults.

The pinned nRF driver's reset event path consumes USBRESET and initializes endpoint state before emitting Event::Reset. The USB framework then sets device state to Default, address zero and reset flags before invoking Handler::reset; it resets interface state afterward. A positive serial return therefore supports reaching this callback during host reset handling. It does not establish completion of all reset work, address assignment, configuration, CDC transfer or typing. A negative result leaves attachment/reset delivery unresolved and is not proof of an interrupt fault. [nRF bus reset path](https://github.com/embassy-rs/embassy/blob/3861d3088da30d40c777dc05d282352e68ec5511/embassy-nrf/src/usb/mod.rs), [USB framework callbacks](https://github.com/embassy-rs/embassy/blob/50c6aac5b9b3b0af18f5a7d61725cc8e4d1f7acb/embassy-usb/src/lib.rs).

The proposed separately approved trial can start from the current verified diagnostic, without rewriting factory first. Require fresh device identity and exact current readback before the transfer, the new image's own independent recovery/readback, and a guide-confirmed startup observation. Preserve the original factory restore throughout. Reuse the existing neutral-control observation for this controlled investigation; do not silently generalize it to another board or bootloader. A failed mode result stops further installs.

## Reducing physical work

The app exposes a deliberate paused state, and a thread heartbeat is configured in standby to process fresh completed physical steps without chat acknowledgments. Activate it when the next physical phase begins. It is polling, not an immediate callback, and the scheduled handoff still needs end-to-end validation.

Read-only `uhubctl` inventory found advertised per-port switching at the keyboard's leaf hub. Actual VBUS removal remains untested. A future port-only test must bind that exact keyboard leaf and its USB2/USB3 counterpart, confirm no unrelated peripheral would be switched, preserve the proper keyboard mode/power-source condition and compare the observed recovery behavior with the manual route. USB absence or an advertised port-power status alone is not voltage evidence. The upstream [uhubctl documentation](https://github.com/mvp/uhubctl#faq) explicitly notes hubs that only disconnect data. No dock port has been toggled.
