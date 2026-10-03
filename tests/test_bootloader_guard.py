"""Adversarial checks for self-update payload boundaries, not hardware tests."""
import hashlib
import struct
import unittest
from scripts import bootloader_guard as g


class BootloaderGuardTest(unittest.TestCase):
    def setUp(self):
        self.binary = bytearray(b'\xff' * (g.END - g.START))
        struct.pack_into('<II', self.binary, 0, 0x20020000, g.START + 0x101)
        at = g.CONFIG - g.START
        struct.pack_into('<4I', self.binary, at, 0x1E9E10F1, 0x20227A79, 5, 100)
        for index, item in enumerate(g.CF2.items()):
            struct.pack_into('<II', self.binary, at + 16 + index * 8, *item)
        self.upper = bytes(self.binary) + b'\xff' * 0x2000
        self.uicr = bytearray(b'\xff' * 0x308)
        struct.pack_into('<II', self.uicr, 0x14, g.START, g.END)
        addresses = list(range(g.START, g.END, 256)) + [g.UICR]
        blocks = []
        for index, address in enumerate(addresses):
            block = bytearray(512)
            struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                             address, 256, index, len(addresses), g.FAMILY)
            block[32:288] = self.uicr[:256] if address == g.UICR else self.binary[address - g.START:address - g.START + 256]
            struct.pack_into('<I', block, 508, 0x0AB16F30)
            blocks.append(block)
        self.image = bytearray(b''.join(blocks))

    def inspect(self, image=None, binary=None, uicr=None):
        return g.inspect(self.image if image is None else image,
                         self.binary if binary is None else binary,
                         self.upper, self.uicr if uicr is None else uicr)

    def test_valid_and_complete(self):
        self.assertEqual(self.inspect()['staging_range'], ['0x63000', '0x6d000'])
        for image in (self.image[:-512], self.image + self.image[:512]):
            with self.assertRaises(ValueError): self.inspect(image=image)

    def test_package_matches_independently_constructed_fixture(self):
        image = g.package(self.binary, self.upper, self.uicr)
        self.assertEqual(image, bytes(self.image))
        report = self.inspect(image=image)
        for field, data in (
            ('sha256', image), ('binary_sha256', self.binary),
            ('original_upper_sha256', self.upper),
            ('original_uicr_sha256', self.uicr),
        ):
            self.assertEqual(report[field], hashlib.sha256(data).hexdigest())

    def test_package_rejects_invalid_binary(self):
        binary = self.binary.copy()
        struct.pack_into('<I', binary, 4, g.CONFIG | 1)
        with self.assertRaises(ValueError):
            g.package(binary, self.upper, self.uicr)

    def test_wrong_target_flags_family_and_order(self):
        for offset, value in ((0, 0), (4, 0), (8, 0x2001), (12, 0x7E000),
                              (16, 252), (20, 1), (24, 160),
                              (28, 0x621E937A), (508, 0)):
            image = self.image.copy(); struct.pack_into('<I', image, offset, value)
            with self.assertRaises(ValueError): self.inspect(image=image)
        image = self.image[512:1024] + self.image[:512] + self.image[1024:]
        with self.assertRaises(ValueError): self.inspect(image=image)

    def test_exact_input_sizes(self):
        for binary, upper, uicr in (
            (self.binary[:-1], self.upper, self.uicr),
            (self.binary + b'\xff', self.upper, self.uicr),
            (self.binary, self.upper[:-1], self.uicr),
            (self.binary, self.upper + b'\xff', self.uicr),
            (self.binary, self.upper, self.uicr[:-1]),
            (self.binary, self.upper, self.uicr + b'\xff'),
        ):
            with self.assertRaises(ValueError):
                g.validate_binary(binary, upper, uicr)

    def test_vector_boundaries(self):
        for offset, value in (
            (0, 0x20000000), (0, 0x20020008), (0, 0x2001fffc),
            (4, g.START - 1), (4, g.CONFIG | 1), (4, g.END | 1),
        ):
            binary = self.binary.copy()
            struct.pack_into('<I', binary, offset, value)
            with self.assertRaises(ValueError):
                g.validate_binary(binary, self.upper, self.uicr)

    def test_entire_config_tail_preserved(self):
        # Preserve unused CF2 slots and bytes after the declared entries too.
        for offset in (g.CONFIG - g.START, g.CONFIG - g.START + 16 + 5 * 8,
                       len(self.binary) - 1):
            binary = self.binary.copy()
            binary[offset] ^= 1
            with self.assertRaises(ValueError):
                g.validate_binary(binary, self.upper, self.uicr)

    def test_cf2_identity_rejected_even_if_original_matches(self):
        # A counterfeit original cannot bypass the measured CF2 field checks.
        for delta, value in ((8, 4), (12, 99), (16 + 2 * 8 + 4, 0x239A002A)):
            binary = self.binary.copy()
            struct.pack_into('<I', binary, g.CONFIG - g.START + delta, value)
            upper = bytes(binary) + self.upper[len(binary):]
            with self.assertRaises(ValueError):
                g.validate_binary(binary, upper, self.uicr)

    def test_metadata_payload_and_reserved_bytes(self):
        for offset in (32, 288, len(self.image) - 512 + 32 + 0x14):
            image = self.image.copy(); image[offset] ^= 1
            with self.assertRaises(ValueError): self.inspect(image=image)
        uicr = self.uicr.copy(); struct.pack_into('<I', uicr, 0x14, 0x73000)
        with self.assertRaises(ValueError): self.inspect(uicr=uicr)
        uicr = self.uicr.copy(); struct.pack_into('<I', uicr, 0x18, 0x7F000)
        with self.assertRaises(ValueError): self.inspect(uicr=uicr)
        image = self.image.copy()
        struct.pack_into('<I', image, len(image) - 512 + 12, g.UICR + 256)
        with self.assertRaises(ValueError): self.inspect(image=image)

    def test_vectors_and_cf2_changes(self):
        for offset, value in ((0, 0x20040000), (4, g.START + 0x100),
                              (g.CONFIG - g.START + 36, 0x239A002A)):
            binary = self.binary.copy(); struct.pack_into('<I', binary, offset, value)
            with self.assertRaises(ValueError): self.inspect(binary=binary)


if __name__ == '__main__': unittest.main()
