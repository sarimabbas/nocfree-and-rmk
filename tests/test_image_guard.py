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


def multi_image(addresses):
    """Construct individually numbered blocks without assuming file order."""
    blocks = []
    for index, address in enumerate(addresses):
        block = image(address=address)
        struct.pack_into('<II', block, 20, index, len(addresses))
        blocks.append(block)
    return blocks


class GuardTests(unittest.TestCase):
    def test_valid_application(self):
        self.assertEqual(guard.inspect(image())['blocks'], 1)

    def test_recovery_marker_is_checked_by_address_even_in_shuffled_file(self):
        blocks = multi_image([guard.START, guard.START + 256, guard.START + 512])
        struct.pack_into('<I', blocks[2], 32, 0x87EEB07C)
        data = b''.join([blocks[2], blocks[0], blocks[1]])
        self.assertEqual(guard.inspect(data, require_recovery_marker=True)['blocks'], 3)

    def test_missing_wrong_or_misplaced_recovery_marker_is_rejected(self):
        with self.assertRaises(ValueError):
            guard.inspect(image(), require_recovery_marker=True)
        blocks = multi_image([guard.START, guard.START + 256, guard.START + 512])
        for index in (1, 2):
            struct.pack_into('<I', blocks[index], 36, 0x87EEB07C)
        with self.assertRaises(ValueError):
            guard.inspect(b''.join(blocks), require_recovery_marker=True)

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

    def test_shuffled_blocks_are_valid(self):
        blocks = multi_image([guard.START, guard.START + 256, guard.START + 512])
        result = guard.inspect(b''.join([blocks[2], blocks[0], blocks[1]]))
        self.assertEqual(result['blocks'], 3)
        self.assertEqual(result['end_exclusive'], hex(guard.START + 768))

    def test_gaps_and_repeated_target_addresses_are_rejected(self):
        for addresses in ([guard.START, guard.START + 512], [guard.START, guard.START]):
            with self.subTest(addresses=addresses), self.assertRaises(ValueError):
                guard.inspect(b''.join(multi_image(addresses)))

    def test_final_application_block_can_end_at_boundary(self):
        addresses = range(guard.START, guard.END, 256)
        result = guard.inspect(b''.join(multi_image(addresses)))
        self.assertEqual(result['end_exclusive'], hex(guard.END))

    def test_stack_may_point_one_past_ram_but_not_above_it(self):
        self.assertEqual(guard.inspect(image(sp=0x20020000))['stack_pointer'], '0x20020000')
        for sp in (0x20020008, 0x20000000, 0x2001ffff):
            with self.subTest(sp=sp), self.assertRaises(ValueError):
                guard.inspect(image(sp=sp))

    def test_reset_vector_must_be_inside_loaded_payload(self):
        self.assertEqual(guard.inspect(image(pc=guard.START + 255))['blocks'], 1)
        for pc in (guard.START - 1, guard.START + 257):
            with self.subTest(pc=pc), self.assertRaises(ValueError):
                guard.inspect(image(pc=pc))

    def test_unaligned_addresses_and_nonstandard_payload_sizes_are_rejected(self):
        with self.assertRaises(ValueError):
            guard.inspect(image(address=guard.START + 4))
        for size in (0, 128, 257, 476):
            data = image()
            struct.pack_into('<I', data, 16, size)
            with self.subTest(size=size), self.assertRaises(ValueError):
                guard.inspect(data)

    def test_block_numbering_is_complete_and_unique(self):
        for index, count in ((1, 1), (0, 2), (0, 0)):
            data = image()
            struct.pack_into('<II', data, 20, index, count)
            with self.subTest(index=index, count=count), self.assertRaises(ValueError):
                guard.inspect(data)
        blocks = multi_image([guard.START, guard.START + 256])
        struct.pack_into('<I', blocks[1], 20, 0)
        with self.assertRaises(ValueError):
            guard.inspect(b''.join(blocks))

    def test_each_magic_word_is_checked(self):
        for offset in (0, 4, 508):
            data = image()
            struct.pack_into('<I', data, offset, 0)
            with self.subTest(offset=offset), self.assertRaises(ValueError):
                guard.inspect(data)


if __name__ == '__main__':
    unittest.main()
