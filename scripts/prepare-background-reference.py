"""Create redistributable background/alpha probes for our renderer and Wallpaper Engine."""
import argparse
import json
import struct
from pathlib import Path


def prepare(output, color="0.2 0.4 0.6"):
    output.mkdir(parents=True, exist_ok=False)
    width, height = 256, 128
    pixels = (bytes([0, 0, 0, 0]) * 128 + bytes([0, 0, 0, 128]) * 128) * height
    tex = b"TEXV0005\0TEXI0001\0" + struct.pack("<7I", 0, 0, width, height, width, height, 0)
    tex += b"TEXB0004\0" + struct.pack("<9I", 1, 0xffffffff, 0, 1, width, height, 0, len(pixels), len(pixels)) + pixels
    for hdr in (False, True):
        directory = output / ("hdr" if hdr else "sdr")
        values = {
            "project.json": {"type": "scene", "file": "scene.json", "title": "Generated background reference",
                             "general": {"properties": {}}},
            "scene.json": {"version": 5, "general": {"orthogonalprojection": {"width": 256, "height": 128},
                "clearcolor": color, "clearenabled": True, "hdr": hdr, "bloom": False,
                "ambientcolor": "1 1 1", "skylightcolor": "1 1 1", "camerafade": False,
                "cameraparallax": False, "nearz": 0.01, "farz": 10000, "zoom": 1},
                "camera": {"center": "0 0 -1", "eye": "0 0 0", "up": "0 1 0"},
                "objects": [{"id": 1, "name": "Transparent and half-black",
                    "image": "models/image.json", "origin": "128 64 0", "size": "256 128", "alpha": 1}]},
            "models/image.json": {"autosize": True, "material": "materials/image.json"},
            "materials/image.json": {"passes": [{"shader": "genericimage4", "textures": ["image"],
                "combos": {}, "blending": "translucent", "cullmode": "nocull", "depthtest": "disabled", "depthwrite": "disabled"}]},
        }
        entries = {}
        for name, value in values.items():
            path = directory / name
            path.parent.mkdir(parents=True, exist_ok=True)
            entries[name] = json.dumps(value, sort_keys=True).encode()
            path.write_bytes(entries[name])
        entries["materials/image.tex"] = tex
        (directory / "materials/image.tex").write_bytes(tex)
        def string(value):
            value = value.encode()
            return struct.pack("<I", len(value)) + value
        index = string("PKGV0016") + struct.pack("<I", len(entries))
        payload = b""
        for name, data in entries.items():
            index += string(name) + struct.pack("<II", len(payload), len(data))
            payload += data
        (directory / "scene.pkg").write_bytes(index + payload)
    (output / "probes.json").write_text(json.dumps({"color": color, "canvas": [256, 128],
        "pixels": "Left transparent; right black alpha 128/255", "hdr_bloom": False}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    prepare(args.output)
