"""Factory-preserving images must exclude MBR, saved data, bootloader and UICR."""
import pathlib
import struct
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import build_firmware_candidates as candidates
from tests.test_startup_image_guard import candidate


class FactoryBoundsTests(unittest.TestCase):
    def fixture(self, role):
        if role != 'receiver':
            return candidate(size=0x2100)
        binary = bytearray(0x2100)
        struct.pack_into('<II', binary, 0, 0x20020000, 0x27009)
        return candidates.encode(binary, 0x27000), bytes(binary)

    def test_valid_half_and_receiver_images_use_factory_application_maps(self):
        for role in candidates.ROLES:
            image, binary = self.fixture(role)
            result = candidates.guard(role, image, binary)
            self.assertEqual(result['start'], '0x27000' if role == 'receiver' else '0x1000')
            self.assertLessEqual(int(result['end_exclusive'], 16), 0x65000)

    def test_added_protected_block_is_rejected_for_every_role(self):
        # Attacker retains a valid application but adds one extra write elsewhere.
        for role in candidates.ROLES:
            image, binary = self.fixture(role)
            for address in (0, 0xF00, 0x65000, 0x6D000, 0x74000, 0x7FF00, 0x10001000):
                blocks = [bytearray(image[offset:offset + 512]) for offset in range(0, len(image), 512)]
                extra = bytearray(blocks[-1])
                struct.pack_into('<I', extra, 12, address)
                blocks.append(extra)
                for index, block in enumerate(blocks):
                    struct.pack_into('<II', block, 20, index, len(blocks))
                with self.subTest(role=role, address=hex(address)), self.assertRaises(ValueError):
                    candidates.guard(role, b''.join(blocks), binary)

    def test_new_bootloader_origins_and_families_are_rejected(self):
        for role in candidates.ROLES:
            _, binary = self.fixture(role)
            for address in (0, 0x7000):
                with self.subTest(role=role, address=address), self.assertRaises(ValueError):
                    candidates.guard(role, candidates.encode(binary, address), binary)
            image = bytearray(self.fixture(role)[0])
            for offset in range(0, len(image), 512):
                struct.pack_into('<I', image, offset + 28, 0xD663823C)
            with self.subTest(role=role), self.assertRaises(ValueError):
                candidates.guard(role, image, binary)


if __name__ == '__main__':
    unittest.main()
