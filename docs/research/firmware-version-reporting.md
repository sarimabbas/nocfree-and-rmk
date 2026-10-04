# Firmware version reporting

## Factory firmware

Read-only research on 2026-10-04. No native devices were opened, reset or flashed. A host parser test is not a successful firmware query.

The [official manual](https://www.nocfree.com/pages/nocfree-and-manual) documents Fn+I as “Check Firmware Version & Battery Level.” That is a useful manual fallback, but the manual does not specify the output format or a host protocol.

The actual host query is in the [official NocFree Link frontend](https://link.nocfree.com/assets/index-CFcin3o_.js), retrieved on 2026-10-04. Its SHA-256 is `f067c02a4688990eaf93915e001d91f9d19decd67b30acacc218d614dd0713d5`. The downloaded JavaScript stays in ignored local evidence. It is minified onto a single line; zero-based character offsets identify the inspected definitions:

| Definition | Character offset | Observation |
| --- | ---: | --- |
| Command and response enums | 291968 | `READ_VERSION` request 81 (`0x51`), reply 209 (`0xd1`) |
| `sg` request builder | 293969 | Empty version-request payload |
| `rg` and `xh` response parsing | 293818 | Optional status, then three binary bytes rendered as major.minor.patch |
| `Ue.connect` | 297391 | Vendor-class WebUSB interface discovery and activation |
| `sendFrameAndWaitResponse` | 304079 | Version query uses the existing ordered request/response queue |
| `rn.readVersion` | 338663 | Version-only GET with a 3000 ms response deadline |

The frame is `[0xff, 0xfe, opcode, payload_length, payload..., 0xfe, 0xff]`. The complete request is `ff fe 51 00 fe ff`. A successful response with status and version 2.4.5 is `ff fe d1 04 00 02 04 05 fe ff`. Status values 0 through 6 are defined; only 0 is success. The vendor parser's optional-status logic is ambiguous for a statusless major version between 0 and 6, and it does not reject nonzero status before constructing a version. Companion must reject those ambiguous/error forms rather than copy that permissiveness. The pure `factory_version` parser requires the complete frame, correct opcode/declared length/trailer, and an explicit success status plus three version bytes. Additional trailing payload bytes are allowed, as the vendor client reads the first three version bytes.

This is **WebUSB vendor bulk transport, not VIA/Vial HID**. The frontend selects configuration 1 if absent, discovers an interface alternate with class `0xff`, claims that interface, selects alternate 0, and derives IN and OUT endpoint numbers from that interface's endpoint descriptors. No fixed interface or endpoint number is hard-coded in the primary source. It activates that interface with a class/interface control transfer: request `0x22`, value 1, index equal to the discovered interface number; it sends value 0 when closing. The host implementation should discover the endpoints from a uniquely identified factory device and never guess a universal endpoint ID. The activation is session setup, not a firmware-version opcode, so it must be isolated from setters and DFU. Only the documented empty `0x51` request is needed to query version.

The frontend explicitly excludes product name `nocfree_dongle` from configuration. A live factory-left version query is therefore the strongest supported path; querying factory right independently or obtaining separate dongle versions is not demonstrated by the frontend and needs hardware evidence. Do not assume the single reply reports all three parts.

### Fields that are not the application version

- `INFO_UF2.TXT` from the original backups reports bootloader `0.9.2-39-g0147d71` and SoftDevice `S140 7.3.0`. Those must never be displayed as factory application version.
- All nine supplied v2.4.5 application UF2s contain the same plausible TinyUSB template device descriptor `bcdDevice = 0x0100`, with string indexes 1/2/3. Descriptor scanning is static evidence, not live descriptor confirmation; the factory startup may replace template VID/PID. In any case `0x0100` does not identify 2.4.5.
- USB serial is a device identity, not a release string. Product strings identify role/layout, not a factory release.
- The official v2.4.5 recovery package catalog says dongle behavior was reverted to v2.4.3. A package filename does not establish the dongle's internal application version.

### Exact-image fallback

A normalized application-payload hash can identify an official release only against an established catalog. Parse UF2 addresses and hash the actual contiguous application bytes (origin `0x27000`), rather than comparing container hashes, storage or bootloader bytes. The supplied official archive has SHA-256 `c11381390b421f6ee3c41bcc090513d392368d09c85b93db31a1dc242427c38a`; its file catalog is already recorded in `factory-images.json`.

The original saved LEFT, RIGHT and dongle application ranges **do not match** their corresponding supplied v2.4.5 UF2 payloads. They must not be labeled 2.4.5 from the supplied archive. Additional static analysis of the original saved LEFT application finds its version-reply construction at `0x2ee6c`: it stores little-endian word `0x00000200` into the four-byte payload, writes `ff fe`, emits reply opcode `0xd1` at `0x2ee82`, and branches to the shared length-four/payload/trailer writer at `0x2ed7c`. This corresponds to success status 0 followed by version **2.0.0**. That is static evidence for this saved left image, not a live observation of the current board; no version is assigned to the original right or dongle from this finding. A supported live query or exact known-image match must bind any displayed version to the connected application. No proprietary firmware bytes, recovery executables or local identifiers are included in this document or the application.

Recommendation: implement the official version-only factory-left WebUSB GET, retaining unknown on unsupported reply/transport. Cache only observations bound to the specific role/device and invalidate them on firmware changes. A later exact-image catalog can recognize historical factory releases without pretending the downloaded archive identifies the board's current firmware. Keep version per role internally; a compact global status may summarize the connected keyboard's observed left application version.

### Companion factory GET implementation

`factory_version::matches` accepts only the recorded factory ANSI-left VID/PID/product tuple (including the alternate underscore product spelling); it excludes factory right, dongle, RMK and unrelated products. `factory_version::read` re-enumerates before opening and requires one matching device with the same native connection ID. It uses the existing active configuration, so it never changes a composite keyboard's global configuration. It rejects multiple vendor interfaces, multiple endpoint pairs or non-bulk endpoints. It claims only the vendor interface, activates it, submits only `READ_VERSION`, and deactivates it on query success/error/timeout before dropping the handle.

Enumeration/open/claim/read share a 2.8-second Tokio deadline; interface teardown has a 200 ms USB timeout. Partial reply frames accumulate within a 261-byte bound, with exact length/opcode/status/trailer checks. Unknown responses fail closed. No native getter has yet been exercised against factory hardware in this implementation: the owner's keyboard remains RMK, and no restoration is undertaken solely to obtain a version. The exact identity gates intentionally support the observed ANSI factory left only until other layouts have equivalent evidence.

## RMK project version

The normal firmware manufacturer descriptor is `NocFree RMK;fw=<CARGO_PKG_VERSION>`. This is the project's application version, not the upstream RMK engine version. Companion accepts canonical SemVer only on the exact recorded RMK VID/PID/product identities. It observes metadata without opening an RMK interface, and discards observations when their USB device instance disappears or the UI's connection snapshot changes. The compact label prefers the connected left application, then dongle, then right; it does not claim the parts share a version.

RMK's documented configurable serial field initially seemed suitable, but the pinned Nordic USB transport replaces it with the chip-derived serial in `UsbTransportBuilder::new`. The existing manufacturer field reaches USB unchanged, avoiding an RMK revision, custom command, or storage/bond migration. Product names and unique serials are preserved. BLE manufacturer information may truncate longer future strings; Companion uses the full USB descriptor, never that BLE field.

All three production Mac role cross-builds passed and their binaries contain the stamp. No device was written. Existing installed RMK builds lack this metadata and correctly remain unknown until an application update. Factory's native GET implementation is source-backed and fixture-tested, but is not yet a live factory-device observation. Unsupported factory right/dongle querying remains unknown.
