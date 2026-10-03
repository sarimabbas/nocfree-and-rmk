"""Serial DFU safety: package contents must match an independently guarded UF2."""
import binascii
import copy
import importlib.util
import io
import json
from pathlib import Path
import struct
import unittest
import warnings
import zipfile

spec = importlib.util.spec_from_file_location('image_guard', Path(__file__).parents[1] / 'scripts/image_guard.py')
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def application(length=516, pc=None):
    data = bytearray(b'\x55' * length)
    if length >= 8:
        struct.pack_into('<II', data, 0, guard.RAM_END, pc or guard.START + 0x101)
    if length >= 516:
        struct.pack_into('<I', data, 512, 0x87EEB07C)
    return bytes(data)


def uf2(binary):
    padded = binary + b'\xff' * (-len(binary) % 256)
    blocks = []
    for index in range(len(padded) // 256):
        block = bytearray(512)
        struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                         guard.START + index * 256, 256, index, len(padded) // 256, guard.FAMILY)
        block[32:288] = padded[index * 256:(index + 1) * 256]
        struct.pack_into('<I', block, 508, 0x0AB16F30)
        blocks.append(bytes(block))
    return b''.join(reversed(blocks))


def package(binary, mutate=None, dat=None, extra=()):
    crc = binascii.crc_hqx(binary, 0xffff)
    manifest = {'manifest': {'application': {
        'bin_file': 'application.bin', 'dat_file': 'application.dat',
        'init_packet_data': {'application_version': 0xffffffff, 'device_revision': 0xffff,
                             'device_type': 82, 'firmware_crc16': crc, 'softdevice_req': [0x123]}},
        'dfu_version': 0.5}}
    if mutate:
        mutate(manifest)
    stream = io.BytesIO()
    with warnings.catch_warnings():
        warnings.simplefilter('ignore', UserWarning)
        with zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('application.bin', binary)
            archive.writestr('application.dat', dat if dat is not None else struct.pack('<HHIHHH', 82, 0xffff, 0xffffffff, 1, 0x123, crc))
            archive.writestr('manifest.json', json.dumps(manifest))
            for name, contents in extra:
                archive.writestr(name, contents)
    return stream.getvalue()


class SerialPackageTests(unittest.TestCase):
    def setUp(self):
        self.binary = application()
        self.image = uf2(self.binary)

    def reject(self, archive, image=None):
        with self.assertRaises(ValueError):
            guard.inspect_serial_package(archive, self.image if image is None else image)

    def test_valid_package_matches_shuffled_guarded_uf2(self):
        result = guard.inspect_serial_package(package(self.binary), self.image)
        self.assertIsInstance(result, dict)
        self.assertEqual(result['binary_size'], len(self.binary))
        self.assertEqual(result['erase_start'], hex(guard.START))
        self.assertEqual(result['erase_end_exclusive'], hex(guard.START + 4096))

    def test_valid_page_boundary_and_partial_page(self):
        for length in (4096, 4100, guard.END - guard.START):
            with self.subTest(length=length):
                binary = application(length)
                guard.inspect_serial_package(package(binary), uf2(binary))

    def test_swapped_binary_even_with_valid_crc_is_rejected(self):
        swapped = bytearray(self.binary)
        swapped[100] ^= 1
        self.reject(package(bytes(swapped)))

    def test_non_ff_padding_is_rejected(self):
        image = bytearray(self.image)
        # Fixture stores last-address block first; byte four is first padding byte.
        image[32 + 4] = 0
        self.reject(package(self.binary), bytes(image))

    def test_extra_uf2_payload_block_is_rejected(self):
        self.reject(package(self.binary), uf2(self.binary + b'\xff' * 256))

    def test_reset_vector_cannot_point_into_only_padding(self):
        binary = application(pc=guard.START + 601)
        guard.inspect(uf2(binary))  # UF2 alone regards this padded address as loaded.
        self.reject(package(binary), uf2(binary))

    def test_bad_uf2_family_is_rejected_before_package_acceptance(self):
        image = bytearray(self.image)
        struct.pack_into('<I', image, 28, 0xADA52840)
        self.reject(package(self.binary), bytes(image))

    def test_empty_unaligned_and_over_budget_binary_are_rejected(self):
        for binary in (b'', application(515), application(guard.END - guard.START + 4)):
            with self.subTest(length=len(binary)):
                self.reject(package(binary), uf2(binary))

    def test_crc_mismatch_and_extra_dat_bytes_are_rejected(self):
        crc = binascii.crc_hqx(self.binary, 0xffff)
        dat = struct.pack('<HHIHHH', 82, 0xffff, 0xffffffff, 1, 0x123, crc)
        self.reject(package(self.binary, dat=dat[:-2] + struct.pack('<H', crc ^ 1)))
        self.reject(package(self.binary, dat=dat + b'\x00\x00'))
        self.reject(package(self.binary, dat=dat[:-1]))

    def test_each_protected_dat_field_is_rejected(self):
        fields = [82, 0xffff, 0xffffffff, 1, 0x123, binascii.crc_hqx(self.binary, 0xffff)]
        for index in range(5):
            changed = copy.copy(fields)
            changed[index] ^= 1
            with self.subTest(field=index):
                self.reject(package(self.binary, dat=struct.pack('<HHIHHH', *changed)))

    def test_non_application_manifests_are_rejected(self):
        for kind in ('softdevice', 'bootloader', 'softdevice_bootloader'):
            def mutate(document):
                document['manifest'][kind] = {'bin_file': 'application.bin', 'dat_file': 'application.dat'}
            with self.subTest(kind=kind):
                self.reject(package(self.binary, mutate=mutate))

    def test_wrong_version_and_missing_application_are_rejected(self):
        self.reject(package(self.binary, mutate=lambda m: m['manifest'].update(dfu_version=0.6)))
        self.reject(package(self.binary, mutate=lambda m: m['manifest'].pop('application')))

    def test_manifest_init_metadata_must_agree_with_dat(self):
        changes = {'application_version': 1, 'device_revision': 1, 'device_type': 1,
                   'firmware_crc16': 1, 'softdevice_req': [0x124]}
        for field, value in changes.items():
            with self.subTest(field=field):
                self.reject(package(self.binary, mutate=lambda m: m['manifest']['application']['init_packet_data'].update({field: value})))

    def test_extra_files_duplicate_names_and_paths_are_rejected(self):
        for name in ('other.bin', 'application.bin', '../application.bin', 'nested/application.dat', '/application.bin'):
            with self.subTest(name=name):
                self.reject(package(self.binary, extra=[(name, b'unsafe')]))

    def test_manifest_references_cannot_escape_or_alias_files(self):
        for field, value in (('bin_file', '../application.bin'), ('dat_file', '/application.dat'),
                             ('bin_file', 'application.dat'), ('dat_file', 'manifest.json')):
            with self.subTest(field=field, value=value):
                self.reject(package(self.binary, mutate=lambda m: m['manifest']['application'].update({field: value})))

    def test_malformed_zip_and_json_are_rejected(self):
        self.reject(b'not a zip')
        self.reject(package(self.binary)[:-30])
        stream = io.BytesIO()
        with zipfile.ZipFile(io.BytesIO(package(self.binary))) as original, zipfile.ZipFile(stream, 'w') as broken:
            for name in original.namelist():
                broken.writestr(name, b'{' if name == 'manifest.json' else original.read(name))
        self.reject(stream.getvalue())


class ReceiverSerialPackageTests(unittest.TestCase):
    def setUp(self):
        binary = bytearray(application())
        struct.pack_into('<I', binary, 512, 0x55555555)
        self.binary = bytes(binary)
        self.image = uf2(self.binary)

    def reject(self, archive, image=None):
        with self.assertRaises(ValueError):
            guard.inspect_receiver_serial_package(archive, self.image if image is None else image)

    def test_marker_free_receiver_matches_shuffled_guarded_uf2(self):
        result = guard.inspect_receiver_serial_package(package(self.binary), self.image)
        self.assertEqual(result['binary_size'], len(self.binary))
        self.assertEqual(result['erase_start'], hex(guard.START))
        self.assertEqual(result['erase_end_exclusive'], hex(guard.START + 4096))

    def test_receiver_rejects_recovery_marker_in_any_block_order(self):
        binary = application()
        shuffled = uf2(binary)
        ordered = b''.join(reversed([shuffled[i:i + 512] for i in range(0, len(shuffled), 512)]))
        for image in (shuffled, ordered):
            with self.subTest(shuffled=image == shuffled):
                self.reject(package(binary), image)

    def test_original_validator_still_requires_recovery_marker(self):
        with self.assertRaisesRegex(ValueError, 'requires Adafruit recovery marker'):
            guard.inspect_serial_package(package(self.binary), self.image)

    def test_receiver_rejects_corrupt_or_mismatched_packages(self):
        self.reject(b'not a zip')
        self.reject(package(self.binary)[:-30])
        self.reject(package(self.binary, dat=b'\x00' * 14))
        changed = bytearray(self.binary)
        changed[100] ^= 1
        self.reject(package(bytes(changed)))
        self.reject(package(self.binary, mutate=lambda m: m['manifest'].update(bootloader={})))

    def test_receiver_rejects_protected_memory_and_wrong_family(self):
        for address in (guard.START - 256, guard.END):
            image = bytearray(self.image)
            struct.pack_into('<I', image, 12, address)
            with self.subTest(address=address):
                self.reject(package(self.binary), bytes(image))
        image = bytearray(self.image)
        struct.pack_into('<I', image, 28, 0xADA52840)
        self.reject(package(self.binary), bytes(image))


if __name__ == '__main__':
    unittest.main()
