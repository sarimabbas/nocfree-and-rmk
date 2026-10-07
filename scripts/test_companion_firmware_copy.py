from pathlib import Path
import json
import tempfile
import unittest

from copy_companion_firmware import copy


class FirmwareCopyTests(unittest.TestCase):
    def test_only_manifest_files_are_bundled(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'source'
            destination = Path(folder) / 'bundle'
            source.mkdir()
            (source / 'manifest.json').write_text(json.dumps({
                'images': [{'uf2': 'left.uf2', 'binary': 'left.bin'}],
            }))
            for name in ('left.uf2', 'left.bin', 'stale.uf2'):
                (source / name).write_bytes(b'image')
            copy(source, destination)
            self.assertEqual({item.name for item in destination.iterdir()},
                             {'manifest.json', 'left.uf2', 'left.bin'})

    def test_manifest_cannot_escape_bundle(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'source'
            source.mkdir()
            (source / 'manifest.json').write_text(json.dumps({
                'images': [{'uf2': '../outside', 'binary': 'left.bin'}],
            }))
            with self.assertRaisesRegex(ValueError, 'invalid filename'):
                copy(source, Path(folder) / 'bundle')


if __name__ == '__main__':
    unittest.main()
