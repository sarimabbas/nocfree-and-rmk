# Bootloader inspection probe: independent review

Source review, 2026-10-03. Scope: `bootloader-inspect.rs`, its Cargo feature/bin
definition and the actual pinned Embassy initialization path. No device access or
firmware transfer was performed by this review.

## Explicit probe behavior

The application accepts only exact LF-terminated `READ_BOOT_V1`. Its 32-byte
command buffer detects overflow and resets at the delimiter. Input is processed
in bounded 64-byte packets. No host-supplied address, length, write, reset,
bootloader-jump or erase command exists. CDC itself has no custom line-coding or
1200-baud reset handler. Disconnect aborts the stream and resets command state
before a new connection.

Reads are fixed aligned 32-bit volatile accesses to MBR 0..0x1000, upper flash
0x74000..0x80000 and UICR 0x10001000..0x10001308 (exclusive ends). These cover
13506 words, framed by role-specific header and completion marker. No FICR unique
identifier address is read. Raw bootloader/UICR evidence must remain private.
UICR/customer words can contain board-specific information; absence of FICR reads
does not make the capture public-safe.

Address zero is not inherently forbidden for this volatile operation. The
[Rust 1.93.1 pointer source](https://github.com/rust-lang/rust/blob/1.93.1/library/core/src/ptr/mod.rs#L1971-L2004)
permits fixed addresses, including zero, outside Rust allocations when hardware
semantics define the read and it does not trap or modify Rust-allocated memory.
Every address here is u32-aligned and every u32 bit pattern is valid. Actual target
read permissions still require device observation; a fault must not be confused
with a complete backup.

The binary is restricted to exactly one half role, reclaimed application layout
and the existing recovery-first marker. The receiver is explicitly excluded.
The compile-time feature and linker marker do not themselves prove recovery or
authorize an installation. No scanner, BLE task or storage task is started.

## Initialization issue found and source correction

The first version called `embassy_nrf::init(Config::default())`, changing only the
HF clock source. That was not a read-only initialization path. The actual pinned
fork at 1b5fc39 defaults to `Debug::Allowed`, which can write UICR.APPROTECT on
newer chip variants. More importantly, its nRF52 initializer unconditionally
attempts UICR PSELRESET1/PSELRESET2 and NFCPINS updates and resets when a write is
performed. Selecting `Debug::NotConfigured` alone does not suppress those pin
configuration writes.

This was reported to root before any probe installation. A correction must
explicitly prevent all UICR/NVMC initialization writes and corresponding reset
behavior, verified against the exact source and candidate build. Merely observing
that existing boards probably already contain the requested values is insufficient
for a read-only inspection contract.

The proposed HAL correction was independently reviewed in the private source
checkout. Its nRF52-only `preserve-uicr` feature modifies the central masked-write
helper: already matching values return `Noop`; all changes return `Failed`.
The entire NVMC-programming branch is conditionally compiled out with that feature.
No `Written` result is possible, so those init attempts cannot trigger the
write-driven reset. Behavior without the feature is unchanged. The probe also
selects `Debug::NotConfigured`, preserving both UICR protection and the software
APPROTECT state. The dependency was then pinned to immutable revision
`4e3d8c790cc1ca615bca524af816bc5d3c244bae`, and `bootloader-inspect` explicitly enables
`embassy-nrf/preserve-uicr`. A modified private checkout alone was not treated as
build provenance.

The fresh left-role ELF was independently inspected. Its retained strings identify
the left inspection role. Its inlined HAL initializer loads PSELRESET1,
PSELRESET2 and NFCPINS, then changes to the CLOCK register base; those init loads
have no UICR store or write-driven reset branch. The ELF does not retain NVMC
programming or `sys_reset` helper symbols, and inspection found no NVMC programming
base constant. This binary/source evidence addresses the identified initialization
path; it is not a hardware read-permission or recovery observation.

The exported left candidate was re-inspected after the lockfile was finalized,
rather than assuming an earlier build remained identical. Its ELF, BIN and UF2
SHA-256 values and probe-source hash are bound in a private `source-review.json`.
The BIN/UF2 hashes match the image guard report and the recovery marker is present
at application offset 0x200. That report explicitly provides no device identity,
hardware acceptance or flashing authorization.

## Collector

The host collector validates the selected role's inspection VID/PID before
opening its serial port at 115200 baud. It issues only the fixed read command.
The parser requires exact role/version framing, every aligned address in order,
valid hexadecimal words, completion and no trailing captured records. Two full
captures must match before private regional files and hash manifest are saved.
Partial responses, mismatches and malformed data are not backups. Read-only
collection does not make these bytes a flash package or prove restoration.

Independently ran all six parser tests: complete fixed readback, wrong role,
missing/duplicate/out-of-order records, invalid words/addresses, truncated/extra
transcript and error response. All passed. The collector stops reading at its
completion frame; this validates the captured transcript, not an unbounded future
serial stream. Actual USB operation remains a device observation.

Review outcome: the source correction and pinned left-role binary inspection
address the discovered UICR path. Address/vector/family
validation, fresh role-specific backup and the proven marker recovery remain
required before a separately approved trial. No probe has been installed by this
review.
