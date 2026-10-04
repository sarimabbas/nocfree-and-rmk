# Factory layout descriptors

Source: official `output_20260911_v2.4.5` firmware archive supplied by the owner. Inspection decoded UF2 payloads in address order and read embedded ASCII strings; no firmware was executed or installed.

| Image | Embedded keyboard product name | UF2 SHA-256 |
| --- | --- | --- |
| Left ANSI | `NocFree & ANSI` | `0c035c6599f10ecaaf26d453abb4f1bdc284018c280aa2b80d39fdfe1e950170` |
| Left ISO | `NocFree & ISO` | `a493d429dc6765384daf96e7c6008d73580b423c9c8133bcc4bc31a9630817b1` |
| Left JP | `NocFree & JP` | `9cb01a3edf385b0cd3146d4cd01caf2cb18502ab76e50935ce22942441961ccd` |
| Left KR | `NocFree & KR` | `457dc18fe21d988c49ae5d33f8f909d9c552dabb0cec684f95e0f66e039f9585` |

Every right image contains `NocFree nRF52833 Right`. ANSI and ISO right files are identical; JP and KR files differ, so the shared product name does not imply identical firmware. The dongle image contains `NocFree_Dongle` and `NocFree nRF52833 Left`.

Owner observations established keyboard USB VID/PID `2886:8029` with `NocFree & ANSI` or legacy `NocFree _ ANSI`; right USB identity is `239a:80d8` with `NocFree nRF52833 Right`. Static strings above support separating the layout suffix from detection. Enumeration of ISO, JP, and KR hardware has not been observed here.

Companion requires the recorded keyboard VID/PID, exact `NocFree & ` or `NocFree _ ` prefix, and a nonempty uppercase layout token. Other uppercase layout tokens are deliberately accepted as a family assumption, not a claim of hardware validation. The shared keyboard identity does not establish left versus dongle; the existing guided single-device identification remains required. Version querying uses the same product-family predicate. Backup and restore operate on the owner's board firmware; layout detection does not select or substitute a factory image.
