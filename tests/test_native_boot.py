"""Keep rmk-boot prototype images outside the existing release image policy."""
import importlib.util
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class NativeBootTests(unittest.TestCase):
    def test_candidate_builder_refuses_native_firmware_before_touching_output(self):
        with tempfile.TemporaryDirectory() as folder:
            output = pathlib.Path(folder) / 'previous'
            output.mkdir()
            manifest = output / 'manifest.json'
            manifest.write_bytes(b'previous approved images')
            result = subprocess.run(
                [sys.executable, str(ROOT / 'scripts/build_firmware_candidates.py'),
                 '--output', str(output)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Factory UF2 packaging is disabled', result.stderr)
            self.assertEqual(manifest.read_bytes(), b'previous approved images')
            self.assertEqual(list(output.iterdir()), [manifest])

    def test_factory_release_guard_rejects_native_boot_and_app_vectors(self):
        guard = load('image_guard')
        migration = load('migration_guard')
        for address in (0, 0x7000):
            block = bytearray(512)
            struct.pack_into('<8I', block, 0, 0x0A324655, 0x9E5D5157, 0x2000,
                             address, 256, 0, 1, guard.FAMILY)
            struct.pack_into('<II', block, 32, 0x20020000, address + 9)
            struct.pack_into('<I', block, 508, 0x0AB16F30)
            with self.subTest(address=address):
                with self.assertRaises(ValueError):
                    guard.inspect(block)
                with self.assertRaises(ValueError):
                    migration.inspect_startup_image(block, bytes(block[32:288]))


if __name__ == '__main__':
    unittest.main()
