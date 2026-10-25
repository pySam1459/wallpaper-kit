"""Rank actual Workshop omissions by distinct wallpaper IDs, checking source identity."""
import argparse
import hashlib
import json
import re
import runpy
from collections import defaultdict
from pathlib import Path


def classify(message):
    effect = re.search(r"effect (effects/[^ ]+/effect\.json) is omitted", message)
    if effect:
        name = effect[1].split("/")[1]
        return f"effect:{name}", name, "missing-effect"
    if "binding uses its authored default" in message:
        return "bindings", "Script / user-property updates", "missing-feature"
    if "optional content omitted:" in message:
        if "particles/" in message:
            reason = message.split(": optional content omitted: ", 1)[1].split(": ", 1)[-1]
            return f"particle-failure:{reason}", f"Particle preparation: {reason}", "feature-error"
        if "text" in message:
            reason = re.sub(r"(?:text|object) \d+", "text", message.split(": optional content omitted: ", 1)[1])
            return f"text-failure:{reason}", reason, "feature-error"
        return "optional-failure", "Other optional-object preparation failures", "feature-error"
    if message.startswith("Particle ") and " is omitted." in message:
        reason = message.split(": ", 1)[1].removesuffix(" is omitted.")
        if reason.startswith("operator component "):
            reason = "operator component"
        return f"particle:{reason}", f"Particle {reason}", "missing-feature"
    if "unsupported particle" in message:
        reason = message.split(": ", 1)[1].rstrip(".")
        return f"particle:{reason}", reason, "missing-feature"
    if "turbulence uses independent harmonic noise" in message:
        return "particle-turbulence-fidelity", "Particle turbulence approximation", "unverified-fidelity"
    if "exact engine randomness" in message or "exact engine noise" in message:
        return "particle-fidelity", "Particle randomness / integration", "unverified-fidelity"
    if "particle parallax is omitted" in message or "parallax is omitted" in message:
        return "parallax-notice", "Layer parallax notice", "needs-source-check"
    if "composition layer and its effects are omitted" in message:
        return "composition", "Composition layers and their effect chains", "missing-feature"
    if message.startswith("General scene camera,"):
        return "general-notice", "Combined camera / color / postprocessing notice", "needs-source-check"
    if "mesh/puppet deformation" in message:
        return "model-fields-notice", "Extra model fields notice", "needs-source-check"
    if "is invisible" in message or "is not selected" in message:
        return "inactive", "Invisible / unselected objects", "intentional"
    if "Video audio is omitted" in message:
        return "audio", "Audio", "intentional"
    if "non-image content is omitted" in message:
        return "non-image", "Other non-image content", "missing-feature"
    if "exact Wallpaper Engine" in message and "unverified" in message:
        return "hdr-fidelity", "HDR bloom reference fidelity", "unverified-fidelity"
    if "Large logical canvas" in message:
        return "canvas-normalization", "Bounded logical-canvas normalization", "intentional"
    # Keep unfamiliar notices visible rather than losing them in a broad bucket.
    normalized = re.sub(r"(?:Object|object|text|shape) \d+", "object", message)
    return f"notice:{normalized}", normalized, "missing-feature"


def active(value):
    if isinstance(value, dict):
        value = value.get("value", False)
    return value is True


def summarize(report, inventories):
    probes = {wallpaper["id"]: wallpaper for wallpaper in report["wallpapers"]}
    if len(probes) != report["total"] or set(probes) != {item["id"] for item in inventories}:
        raise ValueError("The current numbered folders differ from the probe inventory; rerun scripts/workshop-support.ps1")
    groups = {}
    authored = defaultdict(set)
    applied = defaultdict(set)
    wallpapers = []

    def add(key, title, status, ident, message):
        group = groups.setdefault(key, {"key": key, "title": title, "status": status,
                                        "ids": set(), "occurrences": 0, "examples": set()})
        group["ids"].add(ident)
        group["occurrences"] += 1
        group["examples"].add(message)

    for item in inventories:
        ident = item["id"]
        probe = probes[ident]
        playback = (probe.get("evidence") or {}).get("playback", {})
        source_hash = item.get("source_sha256")
        if probe["supported"] and source_hash != playback.get("package_sha256"):
            raise ValueError(f"{ident}: source bytes changed since the probe; rerun scripts/workshop-support.ps1")
        omissions = playback.get("omissions", [])
        resolved = playback.get("resolved_bindings", [])
        if any("uses user property" in message for message in resolved):
            applied["recognized-user-property-links"].add(ident)
        if any("checked clock/calendar profile" in message for message in resolved):
            applied["recognized-clock-calendar-bindings"].add(ident)
        for chain in playback.get("effect_chains", {}).values():
            if len(chain) > 1:
                applied["ordered-image-effect-chains"].add(ident)
            for name in chain:
                applied[f"image-effect:{name}"].add(ident)
        values = item.get("json", {})
        scene = values.get("scene.json", values.get("gifscene.json", item.get("scene", {})))
        sound_ids = {str(o.get("id")) for o in scene.get("objects", []) if o.get("sound")}
        for message in omissions:
            if ": additional effect effects/" in message:
                add("image-effect-chains", "Additional image effect passes", "missing-feature", ident, message)
            object_id = re.match(r"Object (\d+)", message)
            if "non-image content is omitted" in message and object_id and object_id[1] in sound_ids:
                add("audio", "Audio", "intentional", ident, message)
            else:
                add(*classify(message), ident, message)
        general = scene.get("general", {})
        findings = []
        for field in ("cameraparallax", "camerashake", "bloom"):
            if active(general.get(field)):
                authored[field].add(ident)
                findings.append(field)
        if active(general.get("cameraparallax")) and general.get("cameraparallaxmouseinfluence", 0) != 0:
            authored["mouse-parallax"].add(ident)
        if "clearcolor" in general:
            authored["clearcolor"].add(ident)
            findings.append("clearcolor")
            color = general["clearcolor"]
            actual = playback.get("background_color", [])
            if isinstance(color, str) and len(actual) == 4 and general.get("clearenabled", True) is True:
                expected = [float(v) for v in color.split()]
                if len(expected) == 3 and all(abs(a-b) < 0.000001 for a,b in zip(actual, expected)):
                    applied["clearcolor"].add(ident)
        for name, model in values.items():
            if isinstance(model, dict) and model.get("puppet"):
                authored["puppet"].add(ident)
                findings.append(f"puppet:{name}")
        wallpapers.append({"id": ident, "type": probe.get("type"), "supported": probe["supported"],
                           "load_error": probe.get("error"), "source_sha256": source_hash,
                           "project_sha256": item.get("project_sha256"), "authored_features": findings,
                           "inventory_error": item.get("inventory_error"), "omissions": omissions})
    for group in groups.values():
        group["ids"] = sorted(group["ids"], key=int)
        group["affected_wallpapers"] = len(group["ids"])
        group["examples"] = sorted(group["examples"])
    ranked = sorted(groups.values(), key=lambda group: (-group["affected_wallpapers"], group["key"]))
    return {"schema_version": 1, "total": len(wallpapers),
            "loaded": sum(w["supported"] for w in wallpapers),
            "load_errors": [w["id"] for w in wallpapers if not w["supported"]],
            "groups": ranked, "authored_features": {k: sorted(v, key=int) for k, v in sorted(authored.items())},
            "applied_features": {k: sorted(v, key=int) for k, v in sorted(applied.items())},
            "wallpapers": wallpapers}


def markdown(result):
    lines = ["# Local Workshop fidelity audit", "",
             f"{result['loaded']}/{result['total']} load; {len(result['load_errors'])} load failures.", "",
             "Counts mean distinct wallpaper folders, not repeated warnings or layer instances.", "",
             "Categories overlap; additional image effect passes also appear under their named effects.", "",
             "## Missing effects, features and preparation errors", "",
             "| Gap | Wallpapers | Occurrences | Folder IDs |", "| --- | ---: | ---: | --- |"]
    for group in result["groups"]:
        if group["status"] in ("missing-effect", "missing-feature", "feature-error"):
            title = group["title"].replace("|", "\\|")
            lines.append(f"| {title} | {group['affected_wallpapers']} | {group['occurrences']} | {', '.join(group['ids'])} |")
    lines += ["", "## Source-confirmed feature usage", "", "| Authored feature | Wallpapers | Folder IDs |",
              "| --- | ---: | --- |"]
    for name, ids in result["authored_features"].items():
        lines.append(f"| {name} | {len(ids)} | {', '.join(ids)} |")
    lines += ["", "## Applied feature inputs", "",
              "This records reported input values, not complete engine-fidelity verification.", ""]
    for name, ids in result["applied_features"].items():
        lines.append(f"- {name}: {len(ids)} wallpapers; {', '.join(ids)}")
    lines += ["", "## Notices that are not a missing-feature count", "",
              "| Notice | Wallpapers | Status |", "| --- | ---: | --- |"]
    for group in result["groups"]:
        if group["status"] not in ("missing-effect", "missing-feature", "feature-error"):
            lines.append(f"| {group['title']} | {group['affected_wallpapers']} | {group['status']} |")
    lines += ["", "## Every wallpaper", "", "| Folder | Type | Load | Omission notices | Missing named effects |",
              "| --- | --- | --- | ---: | --- |"]
    for wallpaper in result["wallpapers"]:
        effects = sorted(g["title"] for g in result["groups"] if g["status"] == "missing-effect" and wallpaper["id"] in g["ids"])
        lines.append(f"| {wallpaper['id']} | {wallpaper['type']} | {'pass' if wallpaper['supported'] else 'FAIL'} | {len(wallpaper['omissions'])} | {', '.join(effects) or 'none'} |")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    report_bytes = args.report.read_bytes()
    report = json.loads(report_bytes.decode("utf-8-sig"))
    root = Path(report["workshop_root"])
    inventory = runpy.run_path(str(Path(__file__).with_name("inventory-workshop.py")))["inventory"]
    inventories = []
    for folder in sorted((p for p in root.iterdir() if p.is_dir() and p.name.isdigit()), key=lambda p: int(p.name)):
        try:
            inventories.append(inventory(folder))
        except (OSError, ValueError, AssertionError, KeyError) as error:
            inventories.append({"id": folder.name, "inventory_error": str(error)})
    result = summarize(report, inventories)
    result["probe_report_sha256"] = hashlib.sha256(report_bytes).hexdigest()
    result["probe_executable_sha256"] = report["executable_sha256"]
    result["probe_report"] = str(args.report.resolve())
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "report.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    (args.output / "report.md").write_text(markdown(result), encoding="utf-8")
    print(f"Audited {result['total']} folders; {len(result['load_errors'])} load failures; {args.output / 'report.md'}")


if __name__ == "__main__":
    main()
