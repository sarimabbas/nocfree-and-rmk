import hashlib
import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location('migration_guard', Path(__file__).parents[1] / 'scripts/migration_guard.py')
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def uf2(payload, start=guard.START, family=guard.FAMILY):
    blocks = []
    for index in range(len(payload) // 256):
        block = bytearray(512)
        struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                         start + index * 256, 256, index, len(payload) // 256, family)
        block[32:288] = payload[index * 256:(index + 1) * 256]
        struct.pack_into('<I', block, 508, 0x0AB16F30)
        blocks.append(block)
    return b''.join(blocks)


def candidate(size=0x2100):
    binary = bytearray(size)
    struct.pack_into('<II', binary, 0, guard.RAM_END, guard.START + 0x301)
    struct.pack_into('<I', binary, 0x200, guard.RECOVERY_MARKER)
    payload = binary + b'\xff' * ((size + 4095) // 4096 * 4096 - size)
    return uf2(payload), binary


class MigrationGuardTests(unittest.TestCase):
    def test_valid_candidate_reports_exact_binary_and_touched_pages(self):
        image, binary = candidate()
        result = guard.inspect_migration(image, binary)
        self.assertEqual(result['binary_end_exclusive'], '0x3100')
        self.assertEqual(result['end_exclusive'], '0x4000')
        self.assertEqual(result['touched_pages'], ['0x1000', '0x2000', '0x3000'])
        self.assertEqual(result['binary_sha256'], hashlib.sha256(binary).hexdigest())
        # Physical file ordering is immaterial for addressed candidates.
        self.assertEqual(guard.inspect_migration(image[512:] + image[:512], binary)['blocks'], 48)

    def test_exact_binary_and_only_final_page_padding(self):
        image, binary = candidate()
        corrupt = bytearray(image)
        corrupt[-512 + 32] = 0
        for altered_image, altered_binary in ((corrupt, binary), (image[:-512], binary),
                                              (image, binary[:-4]), (image, binary + b'\xff' * 4096)):
            with self.subTest(), self.assertRaises(ValueError):
                guard.inspect_migration(altered_image, altered_binary)

    def test_old_magic_and_wrong_marker_are_rejected_even_with_matching_bin(self):
        for address, value in ((0x1200, 0), (0x3004, guard.S140_MAGIC)):
            _, binary = candidate()
            struct.pack_into('<I', binary, address - guard.START, value)
            image = uf2(binary + b'\xff' * (0x3000 - len(binary)))
            with self.subTest(address=address), self.assertRaises(ValueError):
                guard.inspect_migration(image, binary)

    def test_binary_must_cover_entire_magic_word(self):
        for size in (0x2004, 0x2007):
            image, binary = candidate(size)
            with self.subTest(size=size), self.assertRaises(ValueError):
                guard.inspect_migration(image, binary)
        image, binary = candidate(0x2008)
        self.assertEqual(guard.inspect_migration(image, binary)['binary_size'], 0x2008)

    def test_pc_cannot_point_to_padding_or_invalid_ram(self):
        for sp, pc in ((guard.RAM_START, 0x1301), (guard.RAM_END + 8, 0x1301),
                       (guard.RAM_END - 1, 0x1301), (guard.RAM_END, 0x3101),
                       (guard.RAM_END, 0x1300), (guard.RAM_END, 0xFFF)):
            _, binary = candidate()
            struct.pack_into('<II', binary, 0, sp, pc)
            image = uf2(binary + b'\xff' * (0x3000 - len(binary)))
            with self.subTest(sp=sp, pc=pc), self.assertRaises(ValueError):
                guard.inspect_migration(image, binary)

    def test_uf2_header_and_protected_addresses(self):
        image, binary = candidate()
        mutations = [(0, 0), (4, 0), (508, 0), (8, 0x2001), (28, 0xD663823C),
                     (28, 0x239A0029), (16, 128), (12, 0), (12, 0x1004),
                     (12, 0x65000), (12, 0x10001000), (20, 48), (24, 47),
                     (512 + 20, 0), (512 + 12, 0x1000), (512 + 12, 0x4000)]
        for offset, value in mutations:
            corrupted = bytearray(image)
            struct.pack_into('<I', corrupted, offset, value)
            with self.subTest(offset=offset, value=value), self.assertRaises(ValueError):
                guard.inspect_migration(corrupted, binary)
        for incomplete in (b'', image[:-1], image + image):
            with self.assertRaises(ValueError):
                guard.inspect_migration(incomplete, binary)

    def test_boundary_and_oversized_candidate(self):
        image, binary = candidate(guard.END - guard.START)
        self.assertEqual(guard.inspect_migration(image, binary)['end_exclusive'], hex(guard.END))
        image, binary = candidate(guard.END - guard.START + 4)
        with self.assertRaises(ValueError):
            guard.inspect_migration(image, binary)


if __name__ == '__main__':
    unittest.main()
