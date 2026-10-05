"""Offline regressions for notice integrity, drift, and shallow-checkout rendering."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import unittest

spec = importlib.util.spec_from_file_location('companion_notices', Path(__file__).with_name('companion_notices.py'))
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


class NoticesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for folder in ['scripts', 'desktop', 'docs/notices/texts']:
            (self.root / folder).mkdir(parents=True)
        for name in ['Cargo.toml', 'Cargo.lock']:
            (self.root / 'desktop' / name).write_text(name)
        commit, revision = 'a' * 40, 'b' * 40
        (self.root / 'scripts/package_companion_release.py').write_text(f'ROLES = {{"left": ("private evidence unavailable", "", "", "{commit}", "{revision}")}}\nraise RuntimeError("must not import release tooling")\n')
        raw = b'Copyright Example\nPermission notice with ``` in its text.\n'
        sha = notices.digest(raw)
        self.text_path = self.root / 'docs/notices/texts' / (sha + '.txt')
        self.text_path.write_bytes(raw)
        package = {'name': 'example', 'version': '1.0', 'source': 'registry+https://example.invalid', 'license': 'MIT', 'texts': [{'sha256': sha, 'file': f'texts/{sha}.txt', 'origin': 'Published package: LICENSE'}]}
        desktop = {'target': 'aarch64-apple-darwin', 'cargo_lock_sha256': notices.digest(b'Cargo.lock'), 'cargo_manifest_sha256': notices.digest(b'Cargo.toml'), 'packages': [package]}
        firmware = {'target': 'thumbv7em-none-eabihf', 'features': 'all-features (conservative superset)', 'roles': {'left': {'source_commit': commit, 'rmk_revision': revision}}, 'graphs': [{'source_commit': commit, 'rmk_revision': revision, 'cargo_lock_sha256': 'c' * 64, 'packages': [package]}]}
        self.inventory = {'schema': 1, 'desktop': desktop, 'firmware': firmware}
        self.save()

    def save(self):
        (self.root / 'docs/notices/inventory.json').write_text(json.dumps(self.inventory))

    def test_shallow_checkout_deterministic_full_text_no_private_paths(self):
        inventory, texts = notices.load_inventory(self.root)
        a, gaps = notices.render(inventory, texts)
        shuffled = copy.deepcopy(inventory)
        b, _ = notices.render(shuffled, dict(reversed(list(texts.items()))))
        self.assertEqual(a, b)
        self.assertFalse(gaps)
        self.assertNotIn(str(self.root), a)
        self.assertNotIn('private evidence', a)
        self.assertEqual(a.count('Copyright Example'), 1)
        self.assertIn('````text', a)
        catalog, _ = notices.render(inventory, texts, inventory_only=True)
        self.assertIn('../docs/notices/texts/', catalog)
        self.assertNotIn('](#notice-', catalog)
        self.assertIn('](#notice-', a)

    def test_manifest_and_lock_drift_rejected(self):
        for filename in ['Cargo.toml', 'Cargo.lock']:
            with self.subTest(filename=filename):
                path = self.root / 'desktop' / filename
                path.write_text('changed')
                with self.assertRaisesRegex(ValueError, filename):
                    notices.load_inventory(self.root)
                path.write_text(filename)

    def test_release_pin_drift_rejected(self):
        self.inventory['firmware']['roles']['left']['rmk_revision'] = 'd' * 40
        self.save()
        with self.assertRaisesRegex(ValueError, 'pins changed'):
            notices.load_inventory(self.root)

    def test_text_corruption_and_path_escape_rejected(self):
        self.text_path.write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            notices.load_inventory(self.root)
        self.inventory['desktop']['packages'][0]['texts'][0]['file'] = '../Cargo.toml'
        self.save()
        with self.assertRaisesRegex(ValueError, 'filename'):
            notices.load_inventory(self.root)

    def test_missing_and_duplicate_provenance_rejected(self):
        self.inventory['desktop']['packages'][0]['texts'] = []
        self.save()
        with self.assertRaisesRegex(ValueError, 'explicit unresolved'):
            notices.load_inventory(self.root)
        self.inventory['desktop']['packages'][0]['unresolved'] = 'Published archive omits license'
        self.save()
        inventory, texts = notices.load_inventory(self.root)
        _, gaps = notices.render(inventory, texts)
        self.assertEqual(gaps, ['example 1.0: Published archive omits license'])
        self.inventory['desktop']['packages'] *= 2
        self.save()
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            notices.load_inventory(self.root)


    def test_strict_failure_preserves_existing_output_and_catalog_checks(self):
        self.inventory['desktop']['packages'][0]['texts'] = []
        self.inventory['desktop']['packages'][0]['unresolved'] = 'No source text'
        self.save()
        output = self.root / 'notices.md'
        output.write_text('Keep existing output')
        with patch.object(notices, 'ROOT', self.root):
            self.assertEqual(notices.main([str(output), '--strict']), 1)
            self.assertEqual(output.read_text(), 'Keep existing output')
            self.assertEqual(notices.main([str(output), '--inventory']), 0)
            self.assertEqual(notices.main([str(output), '--inventory', '--check']), 0)
            self.assertNotIn('Full source notice texts', output.read_text())

    def test_committed_inventory_matches_current_pins_and_catalog(self):
        inventory, texts = notices.load_inventory(notices.ROOT)
        catalog, _ = notices.render(inventory, texts, inventory_only=True)
        self.assertEqual((notices.ROOT / 'desktop/THIRD_PARTY_NOTICES.md').read_text(), catalog)


if __name__ == '__main__':
    unittest.main()
