# Companion Check pairing / explicit repair protocol

This is a reserved 32-byte Vial raw-HID custom-get packet. No firmware operations,
bootloader changes or device identities belong in this document. Regular Vial
packets continue to relay unchanged.

Prefix bytes 0..8: `08 7e 04 01 4e 43 50 52` (NCPR version 1).

Request:
- Byte 8: operation 0=query, 1=explicit begin repair.
- Byte 9: addressed role 1=left, 2=dongle.
- Query: bytes 10..32 all zero; no mutation.
- Begin: byte 10 target peer address type 0=public, 1=random; bytes 11..17
  target peer identity address, six raw BLE bytes, not all zero; bytes 17..32 zero.
- Desktop obtains reciprocal target identities from both preflight replies before
  offering repair, then explicitly sends each begin once. No destructive retries.

Reply:
- Prefix is canonical version1.
- 8: result 0=valid/accepted, 1=version mismatch, 2=invalid request,
  3=unsupported capability, 4=left not in dongle policy, 5=busy.
- 9: role, 1=left / 2=dongle.
- 10: capability bit0=explicit dedicated dongle pairing command supported.
- 11: state 0=idle, 1=searching, 2=explicit repair pending/pairing,
  3=encrypted link established, 4=repair failed (including persistence error).
- 12: left selected policy 0=unknown/automatic/off, 1=wired, 2=Bluetooth,
  3=dongle; dongle always0.
- 13: 1=completion requires encryption.
- 14: right split link, left only: 0=unknown, 1=disconnected, 2=connected.
- 15: local address type, 0=public / 1=random / 255=uninitialized.
- 16..22: local identity address6.
- 22: encrypted peer address type, 0=public / 1=random / 255=not connected.
- 23..29: encrypted peer identity address6 (zero when not connected).
- 29..32: reserved zero.

Check is automatic and nonmutating. A bare BLE connection is insufficient:
Desktop requires both fresh state3 replies, reciprocal type/address comparison,
and independently checks the right split-link state. Begin immediately reports
state2 rather than stale connected state. LEFT only clears NUM_BLE_PROFILE through
ProfileManager, guarded again at execution; ordinary Bluetooth hosts are untouched.
DONGLE handles role2 locally before relay, including with no keyboard link. Its
manager clears only bond slot0 and restarts discovery. Replacement is restricted
to the explicitly selected peer type/address; other seeking keyboards are ignored.
Stored links reconnect without clearing. An authentication failure no longer
silently drops a stored dongle bond. Failed storage removal reports state4.

Firmware host suite: 322/322 isolated nextest tests passed, including strict packet
validation, local handling without link, duplicates busy, dedicated slot action,
actual encryption latch, exact identity filters, and right split snapshot. This
is host validation, not a hardware pairing result.

Companion ends Check pairing on its successful connection summary. It does not require a completion button or redirect to Backup firmware.
