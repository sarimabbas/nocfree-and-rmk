"""Check keyboard-half UF2 images, linked binaries."""
import hashlib
import struct

START, END = 0x1000, 0x65000
RAM_START, RAM_END = 0x20008000, 0x20020000
FAMILY = 0x621E937A
RECOVERY_MARKER = 0x87EEB07C
S140_MAGIC = 0x51B1E5DB


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
                status='Address, vector and family checks passed')


def inspect_startup_image(image, binary, role='left'):
    """Check a keyboard half's startup image and linked binary."""
    if role not in ('left', 'right'):
        raise ValueError('startup image requires a keyboard-half role')
    payload = _payload(image, START, END, FAMILY)
    if not binary or len(binary) % 4 or len(binary) < 0x3008 - START:
        raise ValueError('exact aligned BIN must cover the old S140 magic word')
    padded_size = (len(binary) + 4095) // 4096 * 4096
    if START + padded_size > END or payload != binary + b'\xff' * (padded_size - len(binary)):
        raise ValueError('UF2 must match exact BIN with only final page FF padding')
    sp, pc = _vectors(payload, 0, START + 0x204, START + len(binary), RAM_START)
    if struct.unpack_from('<I', payload, 0x200)[0] != 0xFFFFFFFF:
        raise ValueError('Reserved word at 0x1200 must be erased, not a recovery marker')
    if struct.unpack_from('<I', payload, 0x3004 - START)[0] == S140_MAGIC:
        raise ValueError('old S140 magic must be absent at 0x3004')
    return dict(sha256=hashlib.sha256(image).hexdigest(),
                binary_sha256=hashlib.sha256(binary).hexdigest(), binary_size=len(binary),
                role=role, family_id=hex(FAMILY), start=hex(START),
                binary_end_exclusive=hex(START + len(binary)),
                end_exclusive=hex(START + len(payload)), blocks=len(image) // 512,
                touched_pages=[hex(address) for address in range(START, START + len(payload), 4096)],
                stack_pointer=hex(sp), reset_vector=hex(pc),
                status='Address, vector and family checks passed')
