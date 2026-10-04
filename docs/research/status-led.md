# Blue status indicator preparation

The published [NocFree pin mapping](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting)
identifies LEFT P0.10 as an active-low blue LED. LEFT P0.09 and RIGHT P0.17
share charge-status circuitry; this candidate does not configure those pins.
No green channel or right blue pin is documented. GPIO mapping and visual
behavior still require a hardware trial; no green capability is assumed.

## Minimal behavior

| RMK state | Blue output |
| --- | --- |
| Bluetooth selected, ordinary host slot advertising | Blink, 500 ms on / 500 ms off |
| Bluetooth selected, ordinary host slot connected | Steady on |
| Sleeping, inactive, wired, dongle, or off | Off |

Advertising covers both new pairing and reconnection. The current RMK
connection event does not distinguish them, so the candidate does not invent
an independent pairing state or duplicate the profile manager. The factory
[manual](https://www.nocfree.com/pages/nocfree-and-manual) also uses flashing
blue for Bluetooth pairing.

RMK owns the pattern, routing decision, sleep decision, and timing. Board code
only supplies the documented output pin and polarity. The service subscribes
to connection and sleep events, never keyboard events. Timers run only while
blinking; steady/off modes wait for events. Nothing alters pairing shortcuts,
radio parameters, recovery, charging indicators, or the key scanner.

## Acceptance before enabling by default

Build and guard a LEFT candidate with the feature explicitly enabled; also
cross-build all roles without it. Compare code/RAM size. Check physical off
level, blue blink during advertising, solid after connection, off in wired and
dongle positions, and sleep-off / wake restoration. Check USB typing, split
modifiers, disconnect/release and Companion recovery. Measure LED current if
quantifying battery impact; GPIO drive alone does not establish its current.

The software candidate is prepared separately from the installed 30-minute
battery sleep trial. Do not install it or interrogate devices during that trial.

The candidate changes the RMK revision and enables an additional framework
feature; current RMK schema hashing may reset stored settings and bonds when
installed. A fresh full readback/rollback and pairing check are required for the
future LED trial. This preparation does not alter the installed sleep candidate.

P0.10 is an NFC-capable pin and Embassy exposes it only with
`nfc-pins-as-gpio`. The LED feature also requires the fork's `preserve-uicr`,
which prevents any configuration writes. Archived original and factory-restored
LEFT UICR readbacks both contain NFCPINS `0xfffffffe` (GPIO enabled). This is
archived evidence, not a new live read during the sleep test. A board with NFC
pins still protected would require investigation; the candidate never changes
UICR to enable the LED.

Software checkpoint: RMK `396238b1818b44e964a81c70df2efcc9c4d1e4b8`,
298 host tests pass. All three normal roles and the optional LEFT LED role
cross-build and pass image guards. The LED variant adds 600 binary bytes and
160 bytes of `.bss` compared with the same revision without the feature; both
fit application pages through `0x5c000`. These are compiled measurements, not
LED current or physical behavior measurements.

The owner requires distinct connected-state colors instead of mode-specific
blink patterns. Third-color hardware investigation remains open. Blue-only
preparation does not establish the final mode-color design.

## LED identification search (2026-10-04)

### Prepared LEFT optical mixing trial

`status-led-mixing` is separate from the blue-only feature and remains off by
default. It supplies LEFT P0.09 as a standard-drive open-drain output, initially
released. No operation drives this shared charge-status line high. Before
every requested sink, the board adapter checks the physical VBUS register and
releases instead if USB is present. RMK observes USB power changes to release
an already-active sink asynchronously; this is not an instantaneous electrical
interlock and requires hardware attach/detach acceptance.

| State | Requested indicator |
| --- | --- |
| Bluetooth advertising / connected | Blue blink / blue steady |
| Dongle slot connected, battery-powered | Blue and red steady; apparent purple unverified |
| Dongle slot connected, USB-powered | Blue steady; red released for charger |
| Wired, inactive, or asleep | Blue off; red released for charger |

This is an optical experiment, not a promise of three controllable colors.
The charger may add red while charging; firmware cannot suppress that red or
guarantee purple under USB power. Charge-only USB, mixed-light appearance,
brightness/current, sleep/wake, typing/release, and recovery must be tested.
RIGHT and dongle images do not enable the mixing feature. No green pin is
assumed and no bootloader or UICR change is part of this trial.

The binary is preparation only. Installation waits until the current battery
sleep check finishes and a fresh board-specific backup/rollback is prepared.

Mixing software checkpoint: RMK
`a3802774bbec18d004787d2c50618b6ea1942917`. All 300 framework host tests with
mixing enabled and five blue-only regression tests pass. The board input
harness passes eight tests and image tooling passes 78 tests. LEFT, RIGHT,
dongle, blue-only LEFT, and mixing LEFT cross-build and pass image guards.
The mixing binary is 369,812 bytes, adding 1,016 binary bytes and 208 bytes of
`.bss` over the feature-off LEFT build. Independent software review found no
blocker. None of these checks establishes optical color or electrical behavior.

English, Chinese, and Japanese searches for the AND schematic, parts list,
indicator model, teardown, green channel, and purple output did not identify
an LED manufacturer or part number. The public community repository tree has
no schematic or parts list. Its [hardware guide](https://github.com/NocFreeKB/NocFree-and-zmk#4-pins-required-for-zmk-porting)
still documents only blue and shared red outputs.

Certification indexes surfaced a separate AND filing, `2BFHX-NOCFREEANSI`,
dated June 2026. Attempts to retrieve its exhibits through the FCC and public
mirrors failed; this search does not establish what those unavailable exhibits
contain. The older `2BFHX-NOCFREE` filing concerns the Lite family and cannot
establish the AND indicator's components. Teardown searches also did not yield
an identifiable indicator package. These are search limitations, not proof
that a third LED die is absent.

The remaining useful evidence is an AND LED part number plus its wiring, or a
controlled optical/electrical observation of the documented red and blue
channels. A generic RGB LED datasheet cannot identify the fitted component.

## Combined trial observations, 2026-10-04

LEFT mixing and interrupt-driven scanning were installed together. Exact
readback and Companion recovery passed on both halves. The owner observed
purple in battery-powered Dongle mode, blinking blue when Bluetooth was
unpaired and steady blue after pairing. Typing and USB attach/detach were
reported working. RIGHT's indicator is deliberately unchanged: LEFT selects
the host mode, while RIGHT follows the split link. Sleep entry and electrical
current remain unmeasured; these optical observations do not establish charger
current or battery endurance.

## Blue-only simplification (2026-10-04)

The current indicator replaces the experimental red mixing described above.
Firmware drives only the verified LEFT blue pin. Bluetooth and dongle both
blink at 500 ms while searching, stay on for 30 seconds after connecting, then
turn off. Mode/profile changes restart the indication; unchanged events do not.
Wired and sleep switch blue off. Red remains controlled by the charging circuit;
physical color overlap has no mode meaning. RIGHT indication is unchanged.

The mixed output adapter, shared red GPIO control, USB-power LED watcher and
dongle double pulse were removed. Independent review and all 623 framework host
tests passed, along with the 23 input harness tests. All three roles cross-built
and passed their image guards. Physical indication acceptance remains separate.
