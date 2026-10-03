import struct
import unittest
from scripts.bootloader_readback import END, HEADER, SPANS, parse_readback


class BootloaderReadbackTests(unittest.TestCase):
    def setUp(self):
        self.lines = [HEADER + b"LEFT\n"]
        for start, end in SPANS.values():
            self.lines.extend(f"{a:08X}:{a ^ 0x12345678:08X}\n".encode()
                              for a in range(start, end, 4))
        self.lines.append(END)

    def test_complete_fixed_readback(self):
        result = parse_readback(self.lines, "left")
        for name, (start, end) in SPANS.items():
            self.assertEqual(len(result[name]), end - start)
            self.assertEqual(struct.unpack_from("<I", result[name])[0], start ^ 0x12345678)

    def test_wrong_role(self):
        with self.assertRaises(ValueError):
            parse_readback(self.lines, "right")

    def test_missing_duplicate_and_out_of_order_words(self):
        for broken in [self.lines[:2] + self.lines[3:],
                       self.lines[:2] + [self.lines[1]] + self.lines[2:],
                       self.lines[:1] + [self.lines[2], self.lines[1]] + self.lines[3:]]:
            with self.assertRaises(ValueError):
                parse_readback(broken, "left")

    def test_truncated_or_extra_transcript(self):
        for broken in [[], self.lines[:-1], self.lines + [b"extra\n"]]:
            with self.assertRaises(ValueError):
                parse_readback(broken, "left")

    def test_invalid_word_or_address(self):
        for invalid in [b"00000000:ZZZZZZZZ\n", b"10000000:FFFFFFFF\n",
                        b"00000000:12345678", b"00000000:12345678\r\n"]:
            broken = self.lines.copy()
            broken[1] = invalid
            with self.assertRaises(ValueError):
                parse_readback(broken, "left")

    def test_error_reply_is_not_backup(self):
        with self.assertRaises(ValueError):
            parse_readback([b"ERR REQUEST\n"], "left")


if __name__ == "__main__":
    unittest.main()
