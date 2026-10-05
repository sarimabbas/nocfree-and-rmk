import hashlib
import struct
import unittest

from tests.test_migration_guard import guard, uf2


def candidate(size=0x2100, **changes):
    binary = bytearray(size)
    struct.pack_into('<II', binary, 0, changes.get('sp', guard.RAM_END),
                     changes.get('pc', guard.START + 0x205))
    struct.pack_into('<I', binary, 0x200, changes.get('reserved', 0xFFFFFFFF))
    struct.pack_into('<I', binary, 0x3004 - guard.START, changes.get('sd', 0))
    payload = binary + b'\xff' * ((size + 4095) // 4096 * 4096 - size)
    return uf2(payload, family=changes.get('family', guard.FAMILY)), bytes(binary)


class StartupImageGuardTests(unittest.TestCase):
    def test_declared_right_role_and_invalid_roles(self):
        image, binary = candidate()
        self.assertEqual(guard.inspect_startup_image(image, binary, 'right')['role'], 'right')
        for role in ('receiver', '', None):
            with self.subTest(role=role), self.assertRaises(ValueError):
                guard.inspect_startup_image(image, binary, role)

    def test_valid_image_reports_exact_hash_and_bounds(self):
        image, binary = candidate()
        result = guard.inspect_startup_image(image, binary)
        self.assertEqual(result['role'], 'left')
        self.assertEqual(result['binary_sha256'], hashlib.sha256(binary).hexdigest())
        self.assertEqual(result['end_exclusive'], '0x4000')

    def test_bootloader_family_rejected(self):
        with self.assertRaises(ValueError):
            guard.inspect_startup_image(*candidate(family=0xD663823C))

    def test_reset_must_be_thumb_beyond_reserved_word_inside_exact_bin(self):
        for pc in (0x1001, 0x1201, 0x1204, 0x3101, 0x4001):
            with self.subTest(pc=pc), self.assertRaises(ValueError):
                guard.inspect_startup_image(*candidate(pc=pc))

    def test_invalid_stack_rejected(self):
        for sp in (guard.RAM_START, guard.RAM_END + 8, guard.RAM_END - 1):
            with self.subTest(sp=sp), self.assertRaises(ValueError):
                guard.inspect_startup_image(*candidate(sp=sp))

    def test_recovery_marker_and_other_non_erased_reserved_word_rejected(self):
        for reserved in (guard.RECOVERY_MARKER, 0, 0xFFFFFFFE):
            with self.subTest(reserved=reserved), self.assertRaises(ValueError):
                guard.inspect_startup_image(*candidate(reserved=reserved))

    def test_bin_mismatch_rejected(self):
        image, binary = candidate()
        changed = bytearray(binary)
        changed[0x100] ^= 1
        with self.assertRaises(ValueError):
            guard.inspect_startup_image(image, changed)

    def test_s140_magic_rejected(self):
        with self.assertRaises(ValueError):
            guard.inspect_startup_image(*candidate(sd=guard.S140_MAGIC))

    def test_out_of_application_range_rejected(self):
        with self.assertRaises(ValueError):
            guard.inspect_startup_image(*candidate(size=guard.END - guard.START + 4))

    def test_padding_must_be_erased(self):
        image, binary = candidate()
        changed = bytearray(image)
        changed[-512 + 32 + 255] = 0
        with self.assertRaises(ValueError):
            guard.inspect_startup_image(changed, binary)


if __name__ == '__main__':
    unittest.main()
