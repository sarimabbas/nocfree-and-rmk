"""Clean-clone release refusal tests, without private images or device access."""
from pathlib import Path
import hashlib
import tempfile
import unittest
import zipfile

import package_companion_release as release
import unpack_companion_release as extractor


class CleanClonePackageTests(unittest.TestCase):
    def test_missing_evidence_cannot_create_or_replace_release(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            output = root / 'output'
            with self.assertRaises(FileNotFoundError):
                release.package(root, output)
            self.assertFalse(output.exists())
            output.mkdir()
            manifest = output / 'manifest.json'
            manifest.write_bytes(b'previous-release')
            with self.assertRaises(FileNotFoundError):
                release.package(root, output)
            self.assertEqual(manifest.read_bytes(), b'previous-release')
            self.assertEqual(list(output.iterdir()), [manifest])

    def test_unapproved_candidate_is_refused_before_output_changes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            candidate = root / '.evidence' / release.ROLES['left'][0]
            candidate.mkdir(parents=True)
            (candidate / 'candidate.uf2').write_bytes(b'unapproved-image')
            (candidate / 'candidate.bin').write_bytes(b'unapproved-binary')
            output = root / 'output'
            output.mkdir()
            manifest = output / 'manifest.json'
            manifest.write_bytes(b'previous-release')
            with self.assertRaisesRegex(ValueError, 'outside the release allowlist'):
                release.package(root, output)
            self.assertEqual(manifest.read_bytes(), b'previous-release')
            self.assertEqual(list(output.iterdir()), [manifest])


class ReviewedZipTests(unittest.TestCase):
    def source(self, root, extra=None):
        source = root / 'input.zip'
        with zipfile.ZipFile(source, 'w') as archive:
            for name in sorted(extractor.FILES):
                archive.writestr(name, b'reviewed-fixture')
            if extra:
                archive.writestr(extra, b'unexpected')
        return source, hashlib.sha256(source.read_bytes()).hexdigest()

    def test_exact_members_extract_without_replacing_existing_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source, digest = self.source(root)
            output = root / 'output'
            extractor.unpack(source, digest, output)
            self.assertEqual({p.name for p in output.iterdir()}, extractor.FILES)
            for path in output.iterdir():
                self.assertEqual(path.read_bytes(), b'reviewed-fixture')
            with self.assertRaisesRegex(ValueError, 'already exists'):
                extractor.unpack(source, digest, output)
            self.assertEqual((output / 'manifest.json').read_bytes(), b'reviewed-fixture')

    def test_all_layout_members_extract(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / 'input.zip'
            with zipfile.ZipFile(source, 'w') as archive:
                for name in sorted(extractor.ALL_LAYOUT_FILES):
                    archive.writestr(name, b'reviewed-fixture')
            extractor.unpack(source, hashlib.sha256(source.read_bytes()).hexdigest(), root / 'output')
            self.assertEqual({p.name for p in (root / 'output').iterdir()}, extractor.ALL_LAYOUT_FILES)

    def test_checksum_and_unexpected_members_fail_before_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source, digest = self.source(root)
            output = root / 'output'
            with self.assertRaisesRegex(ValueError, 'SHA256'):
                extractor.unpack(source, '0' * 64, output)
            self.assertFalse(output.exists())
            for extra in ['../outside', 'unexpected.txt', 'manifest.json']:
                with self.subTest(extra=extra):
                    source, digest = self.source(root, extra)
                    with self.assertRaisesRegex(ValueError, 'exactly one reviewed package'):
                        extractor.unpack(source, digest, output)
                    self.assertFalse(output.exists())
                    self.assertFalse((root / 'outside').exists())


if __name__ == '__main__':
    unittest.main()
