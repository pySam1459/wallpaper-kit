"""Generated audit contracts; no engine or Workshop assets required."""
import runpy
import unittest
from pathlib import Path

audit = runpy.run_path(str(Path(__file__).with_name("workshop-fidelity.py")))


def fixtures(messages):
    report = {"total": 1, "wallpapers": [{"id": "1", "type": "scene", "supported": True,
              "evidence": {"playback": {"package_sha256": "same", "omissions": messages}}}]}
    inventory = [{"id": "1", "source_sha256": "same", "json": {"scene.json": {
        "general": {"cameraparallax": False}, "objects": [{"id": 3, "sound": "local.mp3"}]},
        "models/simple.json": {"material": "local.json", "autosize": True}}}]
    return report, inventory


class FidelityAudit(unittest.TestCase):
    def test_partial_particle_operators_remain_missing_features(self):
        result = audit["summarize"](*fixtures([
            "Particle particles/a.json: operator component turbulence is omitted.",
            "Particle particles/b.json: operator component oscillateposition is omitted."]))
        group = result["groups"][0]
        self.assertEqual((group["key"], group["affected_wallpapers"], group["occurrences"]),
                         ("particle:operator component", 1, 2))
        self.assertEqual(group["status"], "missing-feature")

    def test_repeated_layers_count_one_wallpaper_and_keep_occurrences(self):
        result = audit["summarize"](*fixtures([
            "Object 1: effect effects/waterwaves/effect.json is omitted.",
            "Object 2: effect effects/waterwaves/effect.json is omitted."]))
        group = result["groups"][0]
        self.assertEqual((group["affected_wallpapers"], group["occurrences"], group["ids"]), (1, 2, ["1"]))

    def test_generic_notices_do_not_invent_enabled_parallax_or_puppets(self):
        result = audit["summarize"](*fixtures([
            "Object 1: parallax is omitted.",
            "scene.json object 1: extra model fields, including mesh/puppet deformation, are omitted; drawing the base material."]))
        self.assertNotIn("cameraparallax", result["authored_features"])
        self.assertNotIn("puppet", result["authored_features"])
        self.assertTrue(all(g["status"] == "needs-source-check" for g in result["groups"]))

    def test_audio_and_invisible_objects_do_not_become_visual_failures(self):
        result = audit["summarize"](*fixtures([
            "Object 3 (local): non-image content is omitted.", "Object 4 is invisible.",
            "Object 5 (visual): non-image content is omitted.",
            "Object 6: optional content omitted: particles/local.json: expected a finite scalar"]))
        groups = {g["key"]: g for g in result["groups"]}
        self.assertEqual(groups["audio"]["status"], "intentional")
        self.assertEqual(groups["inactive"]["status"], "intentional")
        self.assertEqual(groups["non-image"]["status"], "missing-feature")
        self.assertEqual(groups["particle-failure:expected a finite scalar"]["status"], "feature-error")

    def test_stale_source_or_folder_inventory_is_rejected(self):
        report, inventory = fixtures([])
        inventory[0]["source_sha256"] = "changed"
        with self.assertRaisesRegex(ValueError, "source bytes changed"):
            audit["summarize"](report, inventory)
        with self.assertRaisesRegex(ValueError, "numbered folders differ"):
            audit["summarize"](report, [])

    def test_reported_background_must_match_the_authored_input(self):
        report, inventory = fixtures([])
        general = inventory[0]["json"]["scene.json"]["general"]
        general["clearcolor"] = "0.2 0.4 0.6"
        playback = report["wallpapers"][0]["evidence"]["playback"]
        playback["background_color"] = [0.2, 0.4, 0.6, 1]
        self.assertEqual(audit["summarize"](report, inventory)["applied_features"]["clearcolor"], ["1"])
        playback["background_color"][0] = 0.7
        self.assertNotIn("clearcolor", audit["summarize"](report, inventory)["applied_features"])


if __name__ == "__main__":
    unittest.main()
