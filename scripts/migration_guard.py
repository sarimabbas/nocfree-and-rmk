"""Read-only gates for the separately approved left lower-layout trial.

These policies do not authorize transfer or establish connected-device identity.
The existing factory-preserving image guard remains a separate policy.
"""
import hashlib
import struct

START, END = 0x1000, 0x65000
RAM_START, RAM_END = 0x20008000, 0x20020000
FAMILY = 0x621E937A
RECOVERY_MARKER = 0x87EEB07C
S140_MAGIC = 0x51B1E5DB
LEFT_RESTORE_SHA256 = '88cb768f452682cb12025f2296ee151442349079405a7a11f454f752153e4100'


def _payload(data, start, end, family):
    if not data or len(data) % 512:
        raise ValueError('UF2 must contain complete 512-byte blocks')
    count = len(data) // 512
    blocks = {}
    for offset in range(0, len(data), 512):
        block = data[offset:offset + 512]
        magic0, magic1, flags, address, size, index, total, actual_family = struct.unpack_from('<8I', block)
        if (magic0, magic1, struct.unpack_from('<I', block, 508)[0]) != (0x0A324655, 0x9E5D5157, 0x0AB16F30):
            raise ValueError('invalid UF2 magic')
        if flags != 0x2000 or actual_family != family:
            raise ValueError('requires the policy-specific ordinary application family')
        if size != 256 or address % 256:
            raise ValueError('requires aligned 256-byte payloads')
        if total != count or index >= count or index in blocks:
            raise ValueError('invalid block numbering')
        if not start <= address < address + size <= end:
            raise ValueError('image would touch protected or unverified memory')
        blocks[index] = (address, block[32:288])
    ordered = sorted(blocks.values())
    if any(address != start + index * 256 for index, (address, _) in enumerate(ordered)):
        raise ValueError('requires unique contiguous payloads at the policy origin')
    return b''.join(payload for _, payload in ordered)


def _vectors(payload, offset, origin, executable_end, ram_start):
    if len(payload) < offset + 8:
        raise ValueError('missing application vectors')
    sp, pc = struct.unpack_from('<II', payload, offset)
    if not ram_start < sp <= RAM_END or sp % 8:
        raise ValueError('invalid application stack pointer')
    if not pc & 1 or not origin <= pc & ~1 < executable_end:
        raise ValueError('reset vector is not Thumb code within exact application coverage')
    return sp, pc


def inspect_migration(image, binary):
    """Bind a page-padded migration UF2 to its exact linked BIN; never flash."""
    payload = _payload(image, START, END, FAMILY)
    if not binary or len(binary) % 4 or len(binary) < 0x3008 - START:
        raise ValueError('exact aligned BIN must overwrite the entire old S140 magic word')
    padded_size = (len(binary) + 4095) // 4096 * 4096
    if START + padded_size > END or payload != binary + b'\xff' * (padded_size - len(binary)):
        raise ValueError('UF2 must match exact BIN with FF padding only through its final touched page')
    sp, pc = _vectors(payload, 0, START, START + len(binary), RAM_START)
    if struct.unpack_from('<I', payload, 0x200)[0] != RECOVERY_MARKER:
        raise ValueError('requires recovery marker at 0x1200')
    if struct.unpack_from('<I', payload, 0x3004 - START)[0] == S140_MAGIC:
        raise ValueError('old S140 magic must be absent at 0x3004')
    return dict(sha256=hashlib.sha256(image).hexdigest(),
                binary_sha256=hashlib.sha256(binary).hexdigest(), binary_size=len(binary),
                role='left', family_id=hex(FAMILY), start=hex(START),
                binary_end_exclusive=hex(START + len(binary)),
                end_exclusive=hex(START + len(payload)), blocks=len(image) // 512,
                touched_pages=[hex(address) for address in range(START, START + len(payload), 4096)],
                stack_pointer=hex(sp), reset_vector=hex(pc),
                status='migration structure valid; device identity, recovery and approval NOT verified')


def inspect_left_factory_restore(image):
    """Accept only the saved left factory container, including resident S140.

    This is not a full-chip backup: MBR, filesystem, bootloader and UICR are
    outside its readable coverage. Hash binding intentionally rejects even a
    reordered equivalent UF2; restore the original container unchanged.
    """
    payload = _payload(image, START, 0x6D000, 0x239A0029)
    if len(payload) != 0x6D000 - START:
        raise ValueError('requires complete saved factory readable coverage')
    if hashlib.sha256(image).hexdigest() != LEFT_RESTORE_SHA256:
        raise ValueError('restore must be the exact saved left factory UF2')
    if (struct.unpack_from('<I', payload, 0x3004 - START)[0] != S140_MAGIC
            or struct.unpack_from('<I', payload, 0x3008 - START)[0] != 0x27000
            or struct.unpack_from('<H', payload, 0x300C - START)[0] != 0x123
            or struct.unpack_from('<I', payload, 0x3014 - START)[0] != 7003000):
        raise ValueError('requires the saved S140 7.3.0 size and FWID')
    if struct.unpack_from('<I', payload, 0x27200 - START)[0] == RECOVERY_MARKER:
        raise ValueError('factory restore must not contain the development recovery marker')
    sp, pc = _vectors(payload, 0x27000 - START, 0x27000, END, 0x20000000)
    return dict(sha256=LEFT_RESTORE_SHA256, role='left', family_id='0x239a0029',
                start=hex(START), end_exclusive='0x6d000', blocks=len(image) // 512,
                stack_pointer=hex(sp), reset_vector=hex(pc), softdevice='S140 7.3.0',
                status='exact saved left restore valid; connected device identity and approval NOT verified')


def inspect_normal_startup(image, binary, marked_image, marked_binary):
    """Guard a marker-only transition after separate bootloader verification.

    Device/operator policy must prove that the held-key bootloader is installed.
    This function proves only that the already guarded application changes its
    four-byte bootloader marker; it does not authorize removal of recovery.
    """
    marked = inspect_migration(marked_image, marked_binary)
    if (int(marked['reset_vector'], 16) & ~1) < START + 0x204:
        raise ValueError('reset entry must lie beyond the removed marker')
    expected = marked_binary[:0x200] + b'\xff' * 4 + marked_binary[0x204:]
    if binary != expected:
        raise ValueError('normal startup requires exactly the marker-only BIN change')
    payload = _payload(image, START, END, FAMILY)
    padded_size = (len(binary) + 4095) // 4096 * 4096
    if payload != binary + b'\xff' * (padded_size - len(binary)):
        raise ValueError('normal startup UF2 must exactly match the marker-only BIN')
    result = dict(marked)
    result.update(sha256=hashlib.sha256(image).hexdigest(),
                  binary_sha256=hashlib.sha256(binary).hexdigest(),
                  marked_image_sha256=hashlib.sha256(marked_image).hexdigest(),
                  marked_binary_sha256=hashlib.sha256(marked_binary).hexdigest(),
                  status='marker-only transition valid; installed held-key bootloader evidence REQUIRED')
    return result


def inspect_application_shim(image, binary):
    """Validate left shim image structure; this does not prove its entry path."""
    payload = _payload(image, START, END, FAMILY)
    if not binary or len(binary) % 4 or len(binary) < 0x3008 - START:
        raise ValueError('exact aligned BIN must cover the old S140 magic word')
    padded_size = (len(binary) + 4095) // 4096 * 4096
    if START + padded_size > END or payload != binary + b'\xff' * (padded_size - len(binary)):
        raise ValueError('shim UF2 must match exact BIN with only final page FF padding')
    sp, pc = _vectors(payload, 0, START + 0x204, START + len(binary), RAM_START)
    if struct.unpack_from('<I', payload, 0x200)[0] != 0xFFFFFFFF:
        raise ValueError('shim reserved word at 0x1200 must be erased, not a recovery marker')
    if struct.unpack_from('<I', payload, 0x3004 - START)[0] == S140_MAGIC:
        raise ValueError('old S140 magic must be absent at 0x3004')
    return dict(sha256=hashlib.sha256(image).hexdigest(),
                binary_sha256=hashlib.sha256(binary).hexdigest(), binary_size=len(binary),
                role='left', family_id=hex(FAMILY), start=hex(START),
                binary_end_exclusive=hex(START + len(binary)),
                end_exclusive=hex(START + len(payload)), blocks=len(image) // 512,
                touched_pages=[hex(address) for address in range(START, START + len(payload), 4096)],
                stack_pointer=hex(sp), reset_vector=hex(pc),
                status='shim structure valid ONLY; pre-init entry, device identity, recovery and approval NOT verified')
