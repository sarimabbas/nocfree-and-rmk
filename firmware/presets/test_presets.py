"""Offline safety checks for the two whole-keyboard Vial presets."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class PresetTests(unittest.TestCase):
    def setUp(self):
        self.presets = {
            name: json.loads((ROOT / f"firmware/presets/nocfree-{name}.vil").read_text())
            for name in ("mac", "windows-linux")
        }

    def test_uid_dimensions_and_no_unsupported_feature_writes(self):
        source = (ROOT / "firmware/src/vial.rs").read_text()
        raw = source.split("KEYBOARD_ID: [u8; 8] = [", 1)[1].split("];", 1)[0]
        uid = int.from_bytes(bytes(int(x.strip(), 0) for x in raw.split(",")), "little")
        for preset in self.presets.values():
            self.assertEqual(preset["uid"], uid)
            self.assertEqual(preset["version"], 1)
            self.assertEqual(preset["encoder_layout"], [[], []])
            self.assertEqual(preset["layout_options"], -1)
            self.assertEqual(preset["settings"], {})
            for field in ("macro", "tap_dance", "combo", "key_override", "alt_repeat_key"):
                self.assertNotIn(field, preset)
            self.assertEqual(len(preset["layout"]), 2)
            for layer in preset["layout"]:
                self.assertEqual(len(layer), 1)
                self.assertEqual(len(layer[0]), 84)
                self.assertTrue(all(isinstance(code, int) and 0 <= code <= 65535 for code in layer[0]))

    def test_physical_fn_profile_selection_and_bond_clear_are_preserved(self):
        for preset in self.presets.values():
            base, function = (layer[0] for layer in preset["layout"])
            self.assertEqual([base[32], base[79]], [0x5221, 0x5221])
            self.assertEqual(function[8:13], list(range(0x7E00, 0x7E05)))
            self.assertEqual(function[48], 0x7E07)

    def test_os_changes_only_top_row_and_preserves_real_backlight_actions(self):
        mac = [layer[0] for layer in self.presets["mac"]["layout"]]
        pc = [layer[0] for layer in self.presets["windows-linux"]["layout"]]
        top = [1, 2, 3, 4, 5, 6, 37, 38, 39, 40, 41, 42]
        for layer in range(2):
            self.assertEqual([i for i in range(84) if mac[layer][i] != pc[layer][i]], top)
        self.assertEqual(mac[0][1:7], [0x69, 0x6A, 0xC1, 0xB4, 0x7803, 0x7804])
        self.assertEqual([mac[1][i] for i in top], list(range(0x3A, 0x46)))
        self.assertEqual([pc[0][i] for i in top], list(range(0x3A, 0x46)))
        self.assertEqual(pc[1][3:5], [0x0001, 0x0001])
        self.assertEqual(pc[1][5:7], [0x7803, 0x7804])

    def test_each_physical_layout_exposes_every_key_once_in_vial(self):
        for name, count in (("iso", 85), ("jis", 85), ("kr", 89)):
            definition = json.loads((ROOT / f"firmware/vial-{name}.json").read_text())
            self.assertEqual(definition["matrix"], {"rows": 1, "cols": count})
            coords = [item for row in definition["layouts"]["keymap"]
                      for item in row if isinstance(item, str)]
            self.assertEqual(len(coords), count)
            self.assertEqual(set(coords), {f"0,{i}" for i in range(count)})
            ansi = json.loads((ROOT / "firmware/vial.json").read_text())
            self.assertEqual(definition["customKeycodes"], ansi["customKeycodes"])

    def test_custom_names_cover_preset_user_slots_without_changing_geometry(self):
        definition = json.loads((ROOT / "firmware/vial.json").read_text())
        self.assertEqual(len(definition["customKeycodes"]), 8)
        self.assertEqual([k["shortName"] for k in definition["customKeycodes"]],
                         ["BT 1", "BT 2", "BT 3", "BT 4", "BT 5", "BT next", "BT prev", "BT clear"])
        coords = [item for row in definition["layouts"]["keymap"] for item in row if isinstance(item, str)]
        self.assertEqual(len(coords), 84)
        self.assertEqual(set(coords), {f"0,{i}" for i in range(84)})
        self.assertEqual(definition["lighting"], "none")


if __name__ == "__main__":
    unittest.main()
