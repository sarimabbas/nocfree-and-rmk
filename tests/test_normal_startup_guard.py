"""Marker-only transition checks; no installation or hardware claims."""
import hashlib
import struct
import unittest

from scripts import migration_guard as g


def image(binary):
    payload = binary + b'\xff' * ((-len(binary)) % 4096)
    blocks = []
    count = len(payload) // 256
    for index in range(count):
        block = bytearray(512)
        struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                         g.START + index * 256, 256, index, count, g.FAMILY)
        block[32:288] = payload[index * 256:(index + 1) * 256]
        struct.pack_into('<I', block, 508, 0x0AB16F30)
        blocks.append(block)
    return b''.join(blocks)


class NormalStartupGuardTest(unittest.TestCase):
    def setUp(self):
        self.marked = bytearray(b'\xff' * 0x3000)
        struct.pack_into('<II', self.marked, 0, 0x20020000, 0x1205)
        struct.pack_into('<I', self.marked, 0x200, g.RECOVERY_MARKER)
        self.normal = self.marked.copy()
        self.normal[0x200:0x204] = b'\xff' * 4

    def check(self, normal=None, marked=None):
        normal = self.normal if normal is None else normal
        marked = self.marked if marked is None else marked
        return g.inspect_normal_startup(image(normal), normal, image(marked), marked)

    def test_exact_change_and_basis_hashes(self):
        report = self.check()
        self.assertEqual(report['reset_vector'], '0x1205')
        for field, data in (
            ('sha256', image(self.normal)), ('binary_sha256', self.normal),
            ('marked_image_sha256', image(self.marked)),
            ('marked_binary_sha256', self.marked),
        ):
            self.assertEqual(report[field], hashlib.sha256(data).hexdigest())

    def test_removed_marker_cannot_be_reset_entry(self):
        for pc in (0x1001, 0x1201, 0x1203):
            marked = self.marked.copy()
            normal = self.normal.copy()
            struct.pack_into('<I', marked, 4, pc)
            struct.pack_into('<I', normal, 4, pc)
            with self.assertRaisesRegex(ValueError, 'beyond the removed marker'):
                self.check(normal, marked)

    def test_other_changes_and_partial_marker_rejected(self):
        for offset in (0x200, 0x203, 0x204, len(self.normal) - 1):
            normal = self.normal.copy()
            normal[offset] ^= 1
            with self.assertRaises(ValueError):
                self.check(normal)
        for normal in (self.normal[:-4], self.normal + b'\xff' * 4):
            with self.assertRaises(ValueError):
                self.check(normal)

    def test_original_marked_policy_still_required(self):
        marked = self.marked.copy()
        marked[0x200:0x204] = b'\xff' * 4
        with self.assertRaisesRegex(ValueError, 'requires recovery marker'):
            self.check(marked=marked)

    def test_exact_normal_container_required(self):
        for offset in (32, 32 + 256 - 1):
            candidate = bytearray(image(self.normal))
            candidate[offset] ^= 1
            with self.assertRaises(ValueError):
                g.inspect_normal_startup(candidate, self.normal,
                                         image(self.marked), self.marked)
        candidate = bytearray(image(self.normal))
        struct.pack_into('<I', candidate, 28, 0xD663823C)
        with self.assertRaises(ValueError):
            g.inspect_normal_startup(candidate, self.normal,
                                     image(self.marked), self.marked)


if __name__ == '__main__':
    unittest.main()
