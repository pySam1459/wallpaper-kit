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
    def test_applied_chains_count_wallpapers_once_and_keep_every_effect(self):
        report, inventory = fixtures([])
        report["wallpapers"][0]["evidence"]["playback"]["effect_chains"] = {
            "1": ["foliagesway", "waterripple", "foliagesway"], "2": ["shake"]}
        applied = audit["summarize"](report, inventory)["applied_features"]
        self.assertEqual(applied["ordered-image-effect-chains"], ["1"])
        for effect in ["foliagesway", "waterripple", "shake"]:
            self.assertEqual(applied[f"image-effect:{effect}"], ["1"])

    def test_additional_image_passes_are_ranked_without_losing_effect_names(self):
        result = audit["summarize"](*fixtures([
            "scene.json object 1: additional effect effects/waterripple/effect.json is omitted.",
            "scene.json object 1: additional effect effects/foliagesway/effect.json is omitted."]))
        groups = {g["key"]: g for g in result["groups"]}
        self.assertEqual(groups["image-effect-chains"]["affected_wallpapers"], 1)
        self.assertEqual(groups["image-effect-chains"]["occurrences"], 2)
        self.assertEqual(groups["effect:waterripple"]["status"], "missing-effect")
        self.assertEqual(groups["effect:foliagesway"]["status"], "missing-effect")

    def test_approximate_particle_noise_remains_unverified_fidelity(self):
        result = audit["summarize"](*fixtures([
            "Particle particles/a.json: turbulent velocity uses independent noise; exact engine noise is not reproduced."]))
        group = result["groups"][0]
        self.assertEqual(group["key"], "particle-fidelity")
        self.assertEqual(group["status"], "unverified-fidelity")

    def test_turbulence_approximation_is_visible_as_an_unverified_feature(self):
        result = audit["summarize"](*fixtures([
            "Particle particles/a.json: turbulence uses independent harmonic noise; exact engine noise is not reproduced."]))
        group = result["groups"][0]
        self.assertEqual(group["key"], "particle-turbulence-fidelity")
        self.assertEqual(group["status"], "unverified-fidelity")
        self.assertEqual(group["affected_wallpapers"], 1)

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
