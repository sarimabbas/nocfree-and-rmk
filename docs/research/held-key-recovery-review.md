# Held-key startup recovery: independent review

Reviewed 2026-10-03. Scope: `experiments/held-key-recovery`, `docs/controls.md`,
the startup research, and the single host-harness addition to `scripts/check.sh`.
No device operations were performed. This is an offline policy prototype, not an
integrated bootloader or a hardware recovery result.

## Findings and verification

- Physical ANSI masks match the current scanner and default keymap: left Escape
  bit 0 plus Fn bit 40; right Backspace bit 14 plus Fn bit 42. This is source
  agreement, not an independently measured startup-key observation.
- Register configuration and active-low snapshot interpretation match
  `nocfree-input`: input-only direction, explicit inversion clearing, complete
  little-endian reads at addresses 0x20/0x22/0x24. A failed or partial transfer
  cannot manufacture a successful chord.
- No USB, missing initial chord, sampled release, or USB disappearance returns
  to ordinary startup. Later keys cannot turn an initially absent chord into
  recovery. Other held keys do not prevent an intentionally held chord.
- `RECOVERY_IO_ERROR` is explicitly not a DFU request. Integration must preserve
  ordinary upstream application-validity handling: a broken bus does not itself
  park a valid application in recovery; an invalid application still receives
  the upstream recovery path.
- Deadline checks occur between callbacks. The 200 ms policy is nominal, not a
  hard wall-clock bound: a started transfer may consume another 4 ms or a wait
  another 10 ms. The frozen-clock iteration cap permits at most 69 transfers and
  21 waits, or 486 ms of those callback budgets, plus bounded clock/USB-check
  overhead. None of these bounds survives an adapter callback that hangs.
- Independently ran `experiments/held-key-recovery/check.sh` after revisions with
  strict C warnings, AddressSanitizer and UndefinedBehaviorSanitizer. All host
  assertions passed, including both chords, incomplete/wrong chords, late chord,
  initial/configuration USB loss, release, 27 transfer faults, timeout, maximum
  permitted transfer duration, uint32 clock wrap, frozen clock and invalid input.
- Adding that harness to `scripts/check.sh` preserves its read-only purpose and
  stops the pipeline on a harness failure. It does not invoke a device writer.

## Required integration work

There is no nRF I2C/clock/VBUS adapter or upstream bootloader patch yet. Therefore
the host tests do not cover real stuck SCL/STOP handling, peripheral cleanup,
GPREGRET priority, USB enumeration, bootloader fit, or application handoff.
Cleanup must run on every exit, including timeout and configuration failure.
The deliberate chord must precede any application-skip branch; runtime keymaps
and radios must not participate.

The existing application marker still conflicts with normal USB startup. Do not
remove it on the strength of this host prototype. First establish independent
held-key entry on each actual half; only then test a separately guarded
marker-free application. A key held after a cable insertion cannot help if that
insertion never resets the battery-powered CPU. Genuine closed-case restart
after a simulated application hang remains required hardware acceptance.

The receiver is excluded and still needs independent emergency entry. Proposed
controls are accurately labelled as not installed; they are not current user
instructions.

## Bootloader self-update boundary

The researched upstream bootloader-family update path concerns how a replacement
bootloader is installed. It does not replace the held-key entry mechanism and
does not prove vendor compatibility, candidate execution, or recovery after a
bad replacement. Its staging can overlap application/settings space. Existing
application readbacks are insufficient original-bootloader/MBR/UICR restoration
evidence. No bootloader installation is approved by this review.

The research document was reconciled with the implemented timing and I/O-error
policy and re-read after that change. Review outcome: suitable as an offline
policy checkpoint. Not suitable
for a device installation claim or removal of the current recovery safeguard.
