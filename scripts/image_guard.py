#!/usr/bin/env python3
"""Read-only validation of application-only nRF52833 UF2 images."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

FAMILY = 0x621E937A
# Preserve factory SoftDevice, inferred filesystem, bootloader and UICR.
START, END = 0x27000, 0x65000


def inspect(data):
    if not data or len(data) % 512:
        raise ValueError('UF2 must contain complete 512-byte blocks')
    blocks = {}
    total = len(data) // 512
    for offset in range(0, len(data), 512):
        block = data[offset:offset + 512]
        magic0, magic1, flags, address, size, index, count, family = struct.unpack_from('<8I', block)
        if (magic0, magic1, struct.unpack_from('<I', block, 508)[0]) != (0x0A324655, 0x9E5D5157, 0x0AB16F30):
            raise ValueError('invalid UF2 magic')
        if flags != 0x2000 or family != FAMILY:
            raise ValueError('requires ordinary nRF52833 family-tagged flash blocks')
        if size != 256 or address % 256:
            raise ValueError('requires aligned 256-byte payloads')
        if count != total or index >= total or index in blocks:
            raise ValueError('invalid block numbering')
        if not START <= address < address + size <= END:
            raise ValueError('image would touch protected or unverified memory')
        blocks[index] = (address, block[32:32 + size])
    ordered = sorted(blocks.values())
    if ordered[0][0] != START:
        raise ValueError('application must start at factory vector address 0x27000')
    for i, (address, _) in enumerate(ordered):
        if address != START + i * 256:
            raise ValueError('overlapping or non-contiguous image')
    sp, pc = struct.unpack_from('<II', ordered[0][1])
    end = START + total * 256
    if not 0x20000000 < sp <= 0x20020000 or sp % 8:
        raise ValueError('invalid nRF52833 initial stack pointer')
    if not pc & 1 or not START <= pc & ~1 < end:
        raise ValueError('reset vector is not Thumb code inside this image')
    return dict(sha256=hashlib.sha256(data).hexdigest(), family_id=hex(FAMILY),
                start=hex(START), end_exclusive=hex(end), blocks=total,
                stack_pointer=hex(sp), reset_vector=hex(pc),
                status='structurally valid; device compatibility and recovery NOT verified')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', '-i', type=Path, required=True, help='Application UF2 to inspect; never written to a device')
    args = parser.parse_args()
    try:
        print(json.dumps(inspect(args.image.read_bytes()), indent=2))
    except (OSError, ValueError) as error:
        parser.exit(1, f'Rejected: {error}\n')


if __name__ == '__main__':
    main()
