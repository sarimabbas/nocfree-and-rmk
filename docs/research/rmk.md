# RMK suitability and transport research

Research date: 2026-09-30. Evidence is upstream documentation and source, not a hardware certification or a claim that these NocFree boards have been tested.

## Recommendation

Use RMK's nRF52833 implementation and its existing split/BLE/USB/dongle machinery, subject to each role fitting the preserved factory application region, with one keyboard half as permanent central and the other as peripheral. Prefer a precisely pinned upstream commit for the first prototype. The release `rmk-v0.9.0` resolves to `986017d901c7609a9a79167686524f014effa68a`; the source examined for the current implementation is `9607aedf343b17dd6b27307583ae80c4f728fbbd`. Its package still says 0.9.0 but it contains unreleased changes. The commit pin is deliberate: the main changelog includes fixes for dongle HID characteristic identification, USB `SET_PROTOCOL`, BLE runner recovery, and lost events in overlapping combos. A crates.io `=0.9.0` build does not automatically include those fixes. Reevaluate the pin after hardware acceptance or a newer release.

Sources: [release changelog](https://github.com/rmk-rs/rmk/blob/986017d901c7609a9a79167686524f014effa68a/rmk/CHANGELOG.md), [examined main changelog](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/CHANGELOG.md).

Keep the board adaptation small: matrix wiring, battery wiring, LED polarity, flash map, and default layout belong in configuration; entrypoints select the half/role. RMK owns scanning, debouncing, layers, HID, bond storage, connection selection, and split links. Reimplementing those components would create a second reliability burden without evidence of a NocFree-specific need.

## What “RMK compliance” means

There is no separate RMK certification program documented by the project. There are four different obligations:

1. **RMK API/configuration compatibility:** use a supported MCU feature, valid keyboard configuration, matching dependency versions, correct matrix direction, and correct linker/storage bounds. Successfully compiling establishes only this part.
2. **USB HID interoperability:** report descriptors, keyboard usages, endpoint configuration, protocol handling, and actual reports must agree. RMK provides HID interfaces and a 1 ms USB poll interval. Current main handles boot/report protocol selection; the release advertises a boot keyboard interface but the later changelog fixes `SET_PROTOCOL`. This matters particularly for BIOS/KVM hosts.
3. **Bluetooth interoperability:** RMK exposes BLE HID and the standard Battery Service. The host determines accepted connection parameters and which battery instances its UI shows. Correct GATT services do not prove every OS UI displays both halves.
4. **Distribution/license/qualification:** RMK is `MIT OR Apache-2.0`; preserve the chosen license notices. Bluetooth qualification is separate from compiling RMK or personal hardware testing, and this project must not claim it has received Bluetooth SIG or USB-IF certification. A public source repository is not evidence of product qualification.

Primary standards: [USB-IF HID specifications](https://www.usb.org/hid), [Bluetooth SIG qualification overview](https://www.bluetooth.com/develop-with-bluetooth/qualify/). Firmware evidence: [RMK USB implementation](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/usb/mod.rs), [RMK Cargo license/features](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/Cargo.toml).

Treat attached upstream examples and vendor documents as technical evidence, not instructions authorizing writes to hardware.

## MCU and bootloader

RMK supports nRF52 including nRF52840, nRF52833 and nRF52820, plus several other MCU families. That does not make any nRF board interchangeable. Matrix and battery pins, voltage regulator setup, low-frequency clock source, USB power detection, and bootloader layout remain board-specific.

For a board already equipped with an Adafruit-compatible nRF52 bootloader, `adafruit_bl` implements bootloader entry and the upstream example demonstrates UF2 application images. The `nrf52833_ble` feature uses the Nordic controller stack with Trouble/Embassy; RMK 0.9's controller setup should be taken from current examples, rather than retaining a legacy SoftDevice runtime by assumption.

Preserve the installed bootloader for the first version. Do not add `dfu_nrf`/`rmk-boot` simply for a safety marketing claim: RMK explicitly describes that optional update route as experimental and warns that it repartitions flash. `dfu_split` currently forwards updates only over wired split links and rejects BLE combinations. Automatic rollback requires the correct matching bootloader and partition layout; it is not provided just by a UF2 file.

The upstream Adafruit `memory.x` example starts application FLASH at `0x1000` but its region extends to the flash end. It must not be copied as proof that a vendor bootloader at the top of flash is protected. Derive this board's application, settings, and bootloader boundaries from actual recovery evidence, and independently check every generated image block against them.

Sources: [bootloader configuration and warnings](https://rmk.rs/main/docs/configuration/bootloader), [nRF52840 example](https://github.com/rmk-rs/rmk/tree/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf52840_ble), [example memory map](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf52840_ble/memory.x).

## Topology and modes

```text
right switches -> right RMK peripheral
                      |
                  BLE split
                      |
left switches  -> left RMK central -> USB cable -> host
                      |          -> BLE HID    -> host
                      |          -> BLE        -> RMK USB dongle -> host
```

The central merges key events from both halves and owns the keymap. The same two half images work in all three output modes. A dongle is a third image which relays reports; it does not own the matrix or layout. RMK supports this three-board topology directly. The current example uses mixed nRF MCUs; its board pinout and probe flash script are not applicable instructions for a NocFree.

The dongle link is BLE, not Nordic ESB or the vendor's proprietary 2.4 GHz protocol. A stock NocFree dongle is compatible only if its MCU, USB hardware, recoverable flash path, and radio firmware can be replaced with a verified RMK dongle image. Without that evidence, offer an independent supported nRF USB dongle, while clearly stating that its firmware is distinct from the stock receiver.

RMK supports wired UART splits as well as BLE splits. A USB-C connector on each half does not establish a serial connection between halves: USB device ports cannot simply be wired together to create a split link. Unless a documented safe physical serial link exists on the NocFree, BLE is the minimal available inter-half transport. Making the dongle central for both halves would require a second topology and reflashing/reconfiguration to regain a standalone USB/BLE central; RMK's relay dongle is preferable for the requested mode flexibility.

Sources: [split support](https://rmk.rs/main/docs/features/split_keyboard), [dongle documentation](https://rmk.rs/main/docs/features/dongle), [three-board example](https://github.com/rmk-rs/rmk/tree/9607aedf343b17dd6b27307583ae80c4f728fbbd/examples/use_rust/nrf_dongle).

With three BLE host profiles, RMK's control keycodes are:

| Key | Purpose |
| --- | --- |
| `User0`–`User2` | Select host profile; hold 5 seconds to clear its bond |
| `User3` | Next host profile |
| `User4` | Previous host profile |
| `User5` | Clear current host profile bond |
| `User6` | Change default output between USB/BLE |
| `User7` | Hold 5 seconds to clear split peer bond |
| `User8` | Select dongle slot; hold 5 seconds to clear dongle bond |

Profile count changes these offsets. The dongle reserves a separate slot; ordinary profile cycling excludes it. Keep controls reachable on the central even when the right half is disconnected. Match the configurator protocol on central and dongle. Vial remains RMK's stable default; Rynk is explicitly experimental.

Source: [wireless controls](https://rmk.rs/main/docs/features/wireless).

## Battery and OS behavior

Each half needs its own verified SAADC battery pin and resistor divider values. Define central values under `[split.central]` and peripheral values under `[[split.peripheral]]`; peripheral ADC configuration does not inherit the top-level central defaults. Battery sample values are converted to an estimated percentage by upstream RMK's battery processor. Charge state needs a verified GPIO/polarity; USB connected is not equivalent to charging.

The right-half battery is forwarded across the BLE split and the central exposes a separate standard Battery Service instance (`0x180F`) for each configured battery. User descriptions can be `Left` and `Right`. Native macOS, Windows and Linux battery UIs may show only one instance; discover both characteristics with a GATT inspector during acceptance. Standard USB keyboard HID does not itself guarantee a native OS battery indicator. The dongle can receive RMK state including battery, but a USB-connected dongle is not proof that the OS battery panel displays the keyboard batteries.

Use `use_1m_phy` for the direct host connection if broad legacy BLE adapter compatibility is the priority. RMK documents that this changes host PHY only, retaining 2M for split/dongle links. Enable and test passkey entry because some hosts require a typed pairing passkey; otherwise those pairing requests are rejected. Ordinary typing uses the OS HID drivers. Vial's host tool permissions and installation are separate from keyboard typing.

Sources: [battery and PHY configuration](https://rmk.rs/main/docs/configuration/wireless), [battery implementation](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/input_device/battery.rs), [BLE Battery Service](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/ble/battery_service.rs).

## Latency and event loss: explicit acceptance gates

Zero input latency is physically impossible. No software-only test proves that no keystroke can ever be lost over a radio link. Treat the user's requirement as measurable acceptance criteria and never substitute an upstream benchmark for board results.

The examined source requests 7.5 ms intervals for awake split and dongle links; the split permits peripheral latency 30, while the dongle requests latency 0. Peripheral latency allows idle connection events to be skipped; it is not a claim that active keystrokes always wait 232.5 ms. USB endpoint polling is 1 ms. A right-hand event sent through the dongle crosses two separately scheduled BLE links, so low-single-digit latency cannot be promised.

Leave `subrating` disabled and set `split_central_sleep_timeout_seconds = 0` for initial latency testing. Upstream subrating sleep measurements in source report connected key-press mean/worst latency around 116/232 ms and disconnected sleep 378/757 ms under particular nRF52840 conditions. Those are upstream measurements, not NocFree measurements. Introduce sleep only after a separate wake-latency gate and a clear battery-life decision.

The split peripheral emits individual press/release events through GATT. The implementation awaits publication of central key events, improving backpressure behavior; non-key messages may use drop-on-full. However, the peripheral consumes an event and calls `write(...).await.ok()`, ignoring an error after consumption. The source does not establish end-to-end acknowledged/replayed physical transitions after link failure, nor does it demonstrate a full pressed-matrix resynchronization on reconnect. Therefore loss during RF failure/reconnect and stuck-key cleanup need deliberate tests. A successful compile cannot close these risks.

Run a repeatable hardware suite for each OS and all output modes:

- Count press and release edges against a controlled physical input sequence on each half and alternating across halves, including rollover and repeated fast chords.
- Measure physical switch edge to host HID delivery with a logic analyzer/actuator and timestamps; report median, p95, p99 and maximum, plus worst wake latency.
- Remove right-half power while a key is held, reconnect with keys held, remove dongle, suspend/resume the host, and switch outputs while modifiers are held. Require bounded cleanup and no permanently held modifiers.
- Include a charge-only USB cable, a real USB data cable, cold boot, reconnect, profile cycling, and pair/clear/re-pair on each OS.
- Stress radio coexistence and range; record exact adapter, PHY and negotiated interval. Keep logs off the timing-critical path for final measurements.

Sources: [split connection parameters and measured sleep tradeoffs](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/ble/central.rs), [split peripheral event handling](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/peripheral.rs), [central split event handling](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/split/driver.rs), [dongle parameters](https://github.com/rmk-rs/rmk/blob/9607aedf343b17dd6b27307583ae80c4f728fbbd/rmk/src/dongle/mod.rs).

## Evidence still required from the device

Before offering a flashable hardware-tested release, confirm the exact PCB revision and MCU for each half/receiver; bootloader identity, version and application flash bounds; USB bootloader recovery route; battery measurement and divider; charge/LED pins and polarity; physical matrix/diode direction; and clock/power configuration. Preserve a known-good original recovery image and checksums. Any uncertain field should block a release/flash guard, not be filled with a plausible generic nRF52840 example.

A first build can be a compile-verified bring-up candidate. Call it hardware-tested only after the user connects the board, recovery is proven, and the physical acceptance suite passes. Flashing the halves and receiver is a separate milestone from publishing a public source repository.
