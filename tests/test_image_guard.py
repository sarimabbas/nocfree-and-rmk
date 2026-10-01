import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location('image_guard', Path(__file__).parents[1] / 'scripts/image_guard.py')
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def image(address=guard.START, family=guard.FAMILY, flags=0x2000, sp=0x20020000, pc=guard.START + 9):
    data = bytearray(512)
    struct.pack_into('<8I', data, 0, 0x0A324655, 0x9E5D5157, flags, address, 256, 0, 1, family)
    struct.pack_into('<II', data, 32, sp, pc)
    struct.pack_into('<I', data, 508, 0x0AB16F30)
    return data


class GuardTests(unittest.TestCase):
    def test_valid_application(self):
        self.assertEqual(guard.inspect(image())['blocks'], 1)

    def test_protected_addresses(self):
        for address in (0, 0x1000, guard.START - 256, guard.END, 0x74000, 0x10001000):
            with self.subTest(address=address), self.assertRaises(ValueError):
                guard.inspect(image(address=address))

    def test_other_chip_and_non_flash_flags(self):
        for change in ({'family': 0xADA52840}, {'flags': 0x2001}, {'flags': 0}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                guard.inspect(image(**change))

    def test_bad_vectors(self):
        for change in ({'sp': 0x20040000}, {'sp': 0x20000003}, {'pc': guard.START}, {'pc': guard.END + 1}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                guard.inspect(image(**change))

    def test_truncated_duplicate_and_corrupt(self):
        bad = image()
        bad[0] = 0
        for data in (b'', image()[:-1], image() + image(), bad):
            with self.assertRaises(ValueError):
                guard.inspect(data)


if __name__ == '__main__':
    unittest.main()
