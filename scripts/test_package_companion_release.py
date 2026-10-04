#!/usr/bin/env python3
"""Offline release packaging refusal tests. No device access."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import package_companion_release as release


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        for folder, *_ in release.ROLES.values():
            source = release.ROOT / '.evidence' / folder
            target = self.root / '.evidence' / folder
            target.mkdir(parents=True)
            for name in ('candidate.uf2', 'candidate.bin', 'verified.json', 'baseline.json', 'before-CURRENT.UF2', 'installed-CURRENT.UF2'):
                if (source / name).exists():
                    shutil.copyfile(source / name, target / name)
        for spec in release.ROLES.values():
            for proof_path in spec[5:]:
                target = self.root / '.evidence' / proof_path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(release.ROOT / '.evidence' / proof_path, target)

    def tearDown(self):
        self.temp.cleanup()

    def edit_proof(self, role, field, value):
        path = self.root / '.evidence' / release.ROLES[role][0] / 'verified.json'
        proof = json.loads(path.read_text())
        proof[field] = value
        path.write_text(json.dumps(proof))

    def test_exact_readbacks_and_repeatable_manifest(self):
        first = release.package(self.root, check_only=True)
        self.assertEqual(first, release.package(self.root, self.root / 'release'))
        self.assertEqual(first[1], 'f2b333a0060fe2771bbabe16eda08ee976e4a186e482674297dab26110ed1f20')
        self.assertEqual(len(list((self.root / 'release').iterdir())), 7)

    def test_bad_schema_and_missing_preservation(self):
        for role, field, value in [('left', 'schema', 2), ('left', 'storage_equal', True), ('receiver', 'padding_and_untouched_gap_exact', False)]:
            path = self.root / '.evidence' / release.ROLES[role][0] / 'verified.json'
            before = path.read_bytes()
            self.edit_proof(role, field, value)
            with self.assertRaises(ValueError): release.package(self.root, check_only=True)
            path.write_bytes(before)

    def test_review_and_retained_recovery_are_required_and_storage_reset_is_explicit(self):
        for role, spec in release.ROLES.items():
            review_path = self.root / '.evidence' / spec[5]
            original = review_path.read_bytes()
            review = json.loads(original)
            review['roles'][role] = '0' * 64
            review_path.write_text(json.dumps(review))
            with self.assertRaises(ValueError): release.package(self.root, check_only=True)
            review_path.write_bytes(original)
            recovery_path = self.root / '.evidence' / spec[6]
            original = recovery_path.read_bytes()
            recovery_path.write_text('{"schema":1}')
            with self.assertRaises(ValueError): release.package(self.root, check_only=True)
            recovery_path.write_bytes(original)
        release.package(self.root, self.root / 'release')
        manifest = json.loads((self.root / 'release/manifest.json').read_text())
        for image in manifest['images']:
            self.assertFalse(image['storage_preserved'])
            self.assertFalse(image['current_image_runtime_recovery'])
            self.assertEqual(image['recovery_evidence'], 'retained_role_specific_route_unchanged')

    def test_dongle_protected_prefix_cannot_change_behind_true_boolean_proofs(self):
        path = self.root / '.evidence' / release.ROLES['receiver'][0] / 'installed-CURRENT.UF2'
        wrong = bytearray(path.read_bytes())
        wrong[32 + 100] ^= 1  # first preserved 0x1000 payload page
        path.write_bytes(wrong)
        with self.assertRaises(ValueError): release.package(self.root, check_only=True)

    def test_corruption_or_swapped_role(self):
        left = self.root / '.evidence' / release.ROLES['left'][0]
        right = self.root / '.evidence' / release.ROLES['right'][0]
        original = (left / 'candidate.uf2').read_bytes()
        wrong = bytearray(original); wrong[100] ^= 1
        (left / 'candidate.uf2').write_bytes(wrong)
        with self.assertRaises(ValueError): release.package(self.root, check_only=True)
        shutil.copyfile(right / 'candidate.uf2', left / 'candidate.uf2')
        with self.assertRaises(ValueError): release.package(self.root, check_only=True)

    def test_candidate_cannot_claim_a_different_readback(self):
        path = self.root / '.evidence' / release.ROLES['right'][0] / 'installed-CURRENT.UF2'
        wrong = bytearray(path.read_bytes())
        # Change payload at the candidate origin, leaving a stored boolean proof true.
        for offset in range(0, len(wrong), 512):
            if int.from_bytes(wrong[offset+12:offset+16], 'little') == 0x1000:
                wrong[offset+32+100] ^= 1
                break
        path.write_bytes(wrong)
        with self.assertRaises(ValueError): release.package(self.root, check_only=True)


if __name__ == '__main__':
    unittest.main()
