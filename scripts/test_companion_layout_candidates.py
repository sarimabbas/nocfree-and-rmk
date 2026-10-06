"""Layout candidate builds must not reuse another layout's half image."""
import copy
import json
import tomllib
from pathlib import Path
import subprocess
import sys
import unittest

import build_firmware_candidates as candidates


class LayoutCandidates(unittest.TestCase):
    def test_half_features_are_explicit_and_receiver_is_shared(self):
        for layout in candidates.LAYOUTS:
            for role in ('left', 'right'):
                selected = [feature for feature in candidates.features(role, layout)
                            if feature.startswith('layout-')]
                self.assertEqual(selected, [] if layout == 'ansi' else [f'layout-{layout}'])
            self.assertEqual(candidates.features('receiver', layout),
                             candidates.features('receiver', 'ansi'))
            self.assertNotIn('mac-keymap', candidates.features('receiver', layout))
        with self.assertRaises(ValueError):
            candidates.features('left', 'unknown')

    def test_candidate_rejects_redirected_dependency_sources(self):
        manifest = tomllib.loads((candidates.ROOT / 'firmware/Cargo.toml').read_text())
        candidates.validate_dependency_sources(manifest)
        for section, name in ((('dependencies',), 'rmk'), (('dependencies',), 'rmk-types'),
                              (('patch', 'crates-io'), 'embassy-nrf')):
            redirected = copy.deepcopy(manifest)
            dependency = redirected
            for key in section:
                dependency = dependency[key]
            dependency[name]['path'] = '../unreviewed-source'
            with self.subTest(name=name), self.assertRaises(ValueError):
                candidates.validate_dependency_sources(redirected)

    def test_all_layouts_have_separate_output_directories(self):
        script = Path(candidates.__file__)
        output = subprocess.check_output(
            [sys.executable, str(script), '--all-layouts', '--dry-run'], text=True)
        decoder = json.JSONDecoder()
        plans = []
        while output.strip():
            plan, end = decoder.raw_decode(output.lstrip())
            plans.append(plan)
            output = output.lstrip()[end:]
        self.assertEqual([plan['layout'] for plan in plans], list(candidates.LAYOUTS))
        self.assertEqual(len({plan['output'] for plan in plans}), 4)
        for plan in plans:
            self.assertEqual(Path(plan['output']).name, plan['layout'])


if __name__ == '__main__':
    unittest.main()
