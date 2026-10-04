#!/usr/bin/env python3
"""Offline release packaging refusal tests. No device access."""
import copy
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
            for name in ('candidate.uf2', 'candidate.bin', 'verified.json', 'candidate-review.json', 'installed-CURRENT.UF2', 'runtime-recovery-observation.json', 'acceptance-checkpoint.json'):
                if (source / name).exists():
                    shutil.copyfile(source / name, target / name)

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
        self.assertEqual(first[1], '2012f3cbcb2f904a9b9e920bc99af0d982ec4b0d587b1bd6e77a6999971129a8')
        self.assertEqual(len(list((self.root / 'release').iterdir())), 7)

    def test_bad_schema_and_missing_preservation(self):
        for role, field, value in [('left', 'schema', 2), ('left', 'storage_equal', False), ('receiver', 'S140_prefix_exact', False)]:
            path = self.root / '.evidence' / release.ROLES[role][0] / 'verified.json'
            before = path.read_bytes()
            self.edit_proof(role, field, value)
            with self.assertRaises(ValueError): release.package(self.root, check_only=True)
            path.write_bytes(before)

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
