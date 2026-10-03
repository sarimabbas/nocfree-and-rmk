"""Offline guard for the measured NocFree left bootloader self-update layout.

This does not authorize a transfer or prove that the connected device can recover.
The UICR record is verification metadata for the pinned self-update parser; it is
not permission to program UICR. Staging overwrites 0x63000..0x6d000, including RMK
settings, which must be backed up and restored separately.
"""
import hashlib
import struct

START, END, CONFIG = 0x74000, 0x7E000, 0x7D800
FAMILY = 0xD663823C
UICR = 0x10001000
CF2 = {204: 0x80000, 205: 0x20000, 208: 0x239A0029,
       209: 0x621E937A, 210: 32}


def validate_binary(binary, original_upper, original_uicr):
    """Require the measured full boot code region and unchanged board config."""
    if len(binary) != END - START or len(original_upper) != 0xC000 or len(original_uicr) != 0x308:
        raise ValueError('requires full bootloader and exact original readback sizes')
    offset = CONFIG - START
    if binary[offset:] != original_upper[offset:END - START]:
        raise ValueError('CF2 configuration must match the measured board exactly')
    magic0, magic1, used, total = struct.unpack_from('<4I', binary, offset)
    if (magic0, magic1, used, total) != (0x1E9E10F1, 0x20227A79, 5, 100):
        raise ValueError('unexpected CF2 configuration')
    entries = dict(struct.unpack_from('<II', binary, offset + 16 + i * 8) for i in range(used))
    if entries != CF2:
        raise ValueError('wrong measured left board identity or memory configuration')
    if struct.unpack_from('<II', original_uicr, 0x14) != (START, END):
        raise ValueError('original UICR bootloader/MBR parameter addresses differ')
    sp, pc = struct.unpack_from('<II', binary)
    if not 0x20000000 < sp <= 0x20020000 or sp % 8:
        raise ValueError('invalid bootloader stack vector')
    if not pc & 1 or not START <= (pc & ~1) < CONFIG:
        raise ValueError('invalid bootloader reset vector')


def inspect(image, binary, original_upper, original_uicr):
    """Accept complete ordered boot code plus one exact UICR metadata record."""
    validate_binary(binary, original_upper, original_uicr)
    addresses = list(range(START, END, 256)) + [UICR]
    if len(image) != len(addresses) * 512:
        raise ValueError('incomplete or extra self-update blocks')
    for index, address in enumerate(addresses):
        block = image[index * 512:(index + 1) * 512]
        expected = (0x0A324655, 0x9E5D5157, 0x2000, address, 256,
                    index, len(addresses), FAMILY)
        if struct.unpack_from('<8I', block) != expected or struct.unpack_from('<I', block, 508)[0] != 0x0AB16F30:
            raise ValueError('wrong address, order, family, flags or block header')
        payload = original_uicr[:256] if address == UICR else binary[address - START:address - START + 256]
        if block[32:288] != payload or block[288:508] != bytes(220):
            raise ValueError('payload or reserved bytes differ from exact inputs')
    return {'schema': 1, 'role': 'left', 'sha256': hashlib.sha256(image).hexdigest(),
            'binary_sha256': hashlib.sha256(binary).hexdigest(),
            'original_upper_sha256': hashlib.sha256(original_upper).hexdigest(),
            'original_uicr_sha256': hashlib.sha256(original_uicr).hexdigest(),
            'bootloader_range': [hex(START), hex(END)],
            'staging_range': ['0x63000', '0x6d000'],
            'uicr_record': 'verification metadata only; programming forbidden',
            'status': 'structure valid; device evidence and installation review still required'}


def package(binary, original_upper, original_uicr):
    """Construct only an offline, fully guarded self-update container."""
    validate_binary(binary, original_upper, original_uicr)
    addresses = list(range(START, END, 256)) + [UICR]
    blocks = []
    for index, address in enumerate(addresses):
        block = bytearray(512)
        struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                         address, 256, index, len(addresses), FAMILY)
        block[32:288] = original_uicr[:256] if address == UICR else binary[address - START:address - START + 256]
        struct.pack_into('<I', block, 508, 0x0AB16F30)
        blocks.append(block)
    image = b''.join(blocks)
    inspect(image, binary, original_upper, original_uicr)
    return image
