#!/usr/bin/env python3
"""Read-only validation of application-only nRF52833 UF2 images."""
import argparse
import binascii
import hashlib
import io
import json
from pathlib import Path
import struct
import zipfile
import zlib

FAMILY = 0x621E937A
# Preserve factory SoftDevice, inferred filesystem, bootloader and UICR.
START, END = 0x27000, 0x65000
# Match the application linker; preserve the bootloader's warm-reset RAM.
RAM_START, RAM_END = 0x20008000, 0x20020000


def inspect(data, require_recovery_marker=False):
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
    if not RAM_START < sp <= RAM_END or sp % 8:
        raise ValueError('initial stack pointer must use application RAM, preserving bootloader RAM')
    if not pc & 1 or not START <= pc & ~1 < end:
        raise ValueError('reset vector is not Thumb code inside this image')
    if require_recovery_marker:
        if len(ordered) < 3 or struct.unpack_from('<I', ordered[2][1])[0] != 0x87EEB07C:
            raise ValueError('requires Adafruit recovery marker at application base + 0x200')
    return dict(sha256=hashlib.sha256(data).hexdigest(), family_id=hex(FAMILY),
                start=hex(START), end_exclusive=hex(end), blocks=total,
                stack_pointer=hex(sp), reset_vector=hex(pc),
                status='Address, vector and family checks passed')


def inspect_serial_package(package, image):
    """Bind an application-only legacy DFU package to a guarded recovery UF2.

    START erases application pages before DATA, so validate the actual ZIP,
    including the application bytes and init packet.
    """
    guarded = inspect(image, require_recovery_marker=True)
    return _inspect_serial_package(package, image, guarded)


def inspect_receiver_serial_package(package, image):
    """Validate a protected receiver package without recovery-first boot behavior.

    USB-only receivers must start their application to expose RMK DFU detach.
    """
    guarded = inspect(image)
    for offset in range(0, len(image), 512):
        if (struct.unpack_from('<I', image, offset + 12)[0] == START + 0x200
                and struct.unpack_from('<I', image, offset + 32)[0] == 0x87EEB07C):
            raise ValueError('receiver must not contain the recovery-first marker')
    return _inspect_serial_package(package, image, guarded)


def _inspect_serial_package(package, image, guarded):
    try:
        with zipfile.ZipFile(io.BytesIO(package)) as archive:
            names = archive.namelist()
            if len(names) != 3 or len(set(names)) != 3 or 'manifest.json' not in names:
                raise ValueError('serial package must contain exactly three unique application files')
            if any('/' in name or '\\' in name for name in names):
                raise ValueError('serial package paths are not allowed')
            if any(info.file_size > END - START for info in archive.infolist()):
                raise ValueError('serial package member exceeds the application slot')
            if any(info.flag_bits & 1 for info in archive.infolist()):
                raise ValueError('encrypted serial package members are not allowed')
            document = json.loads(archive.read('manifest.json'))
            manifest = document['manifest']
            if set(document) != {'manifest'} or set(manifest) != {'application', 'dfu_version'} or manifest['dfu_version'] != 0.5:
                raise ValueError('requires a legacy application-only manifest')
            application = manifest['application']
            if set(application) != {'bin_file', 'dat_file', 'init_packet_data'}:
                raise ValueError('unexpected application manifest fields')
            binary_name, dat_name = application['bin_file'], application['dat_file']
            if binary_name == dat_name or set(names) != {'manifest.json', binary_name, dat_name}:
                raise ValueError('manifest must identify distinct BIN and DAT files')
            binary, packet = archive.read(binary_name), archive.read(dat_name)
            if not binary or len(binary) % 4 or len(packet) != 14:
                raise ValueError('requires a four-byte aligned application and 14-byte init packet')
            crc = binascii.crc_hqx(binary, 0xffff)
            expected = (0x52, 0xffff, 0xffffffff, 1, 0x123, crc)
            if struct.unpack('<HHIHHH', packet) != expected:
                raise ValueError('init packet does not match the preserved S140 application policy or CRC')
            metadata = dict(device_type=0x52, device_revision=0xffff,
                            application_version=0xffffffff, softdevice_req=[0x123], firmware_crc16=crc)
            if application['init_packet_data'] != metadata:
                raise ValueError('manifest metadata disagrees with the application init packet')
    except (zipfile.BadZipFile, KeyError, TypeError, AttributeError, NotImplementedError,
            RuntimeError, EOFError, zlib.error, json.JSONDecodeError, UnicodeDecodeError) as error:
        raise ValueError('invalid serial application package') from error
    payloads = sorted((struct.unpack_from('<I', image, offset + 12)[0],
                       image[offset + 32:offset + 288]) for offset in range(0, len(image), 512))
    payload = b''.join(data for _, data in payloads)
    padded_size = (len(binary) + 255) // 256 * 256
    if len(payload) != padded_size or payload != binary + b'\xff' * (padded_size - len(binary)):
        raise ValueError('serial application does not match the exact guarded UF2 payload')
    if int(guarded['reset_vector'], 16) & ~1 >= START + len(binary):
        raise ValueError('reset vector points outside the exact serial application')
    erase_end = START + (len(binary) + 4095) // 4096 * 4096
    if erase_end > END:
        raise ValueError('serial application erase would touch protected memory')
    return dict(package_sha256=hashlib.sha256(package).hexdigest(),
                binary_sha256=hashlib.sha256(binary).hexdigest(), binary_size=len(binary),
                erase_start=hex(START), erase_end_exclusive=hex(erase_end), uf2=guarded,
                status='Application package matches the checked UF2')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', '-i', type=Path, required=True, help='Application UF2 to inspect; never written to a device')
    parser.add_argument('--require-recovery-marker', '-r', action='store_true',
                        help='Require the recovery marker at the image startup offset')
    parser.add_argument('--serial-package', '-p', type=Path,
                        help='Check a legacy application DFU ZIP against this UF2')
    args = parser.parse_args()
    try:
        data = args.image.read_bytes()
        result = (inspect_serial_package(args.serial_package.read_bytes(), data) if args.serial_package
                  else inspect(data, args.require_recovery_marker))
        print(json.dumps(result, indent=2))
    except (OSError, ValueError) as error:
        parser.exit(1, f'Rejected: {error}\n')


if __name__ == '__main__':
    main()
