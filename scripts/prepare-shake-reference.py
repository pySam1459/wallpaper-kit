"""Generate original Shake probes under ignored artifacts/ for Wallpaper Engine comparison."""
import argparse
import hashlib
import json
import math
import struct
from pathlib import Path


def texture(width, height, pixels):
    return (b"TEXV0005\0TEXI0001\0" + struct.pack("<7I", 0, 0, width, height, width, height, 0)
            + b"TEXB0004\0" + struct.pack("<9I", 1, 0xffffffff, 0, 1, width, height, 0, len(pixels), len(pixels))
            + pixels)


def write_project(directory, entries):
    for asset, data in entries.items():
        path = directory / asset
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    def string(value):
        value = value.encode()
        return struct.pack("<I", len(value)) + value
    index = string("PKGV0016") + struct.pack("<I", len(entries))
    payload = b""
    for asset, data in entries.items():
        index += string(asset) + struct.pack("<II", len(payload), len(data))
        payload += data
    (directory / "scene.pkg").write_bytes(index + payload)


def probe(strength=0.5, flow=(255,127,0,255), phase=255, opacity=255, **constants):
    return {"constants":{"speed":1, "strength":strength, "friction":"1 1", "bounds":"0 1", **constants},
            "flow":flow, "phase":phase, "opacity":opacity}


def prepare(output, engine_assets):
    evidence = Path(__file__).resolve().parents[1] / "artifacts"
    if not output.resolve().is_relative_to(evidence) or output.resolve() == evidence:
        raise ValueError("Reference inputs must stay under ignored artifacts/")
    output.mkdir(parents=True, exist_ok=False)
    # Only Wallpaper Engine consumes these installed inputs. wallpaper-kit
    # neither compiles nor translates the proprietary shaders.
    vendor = {}
    for asset in ("effect.json", "materials/effects/shake.json", "shaders/effects/shake.vert", "shaders/effects/shake.frag"):
        target = "effects/shake/effect.json" if asset == "effect.json" else asset
        vendor[target] = (engine_assets / "effects/shake" / asset).read_bytes()
    vendor["shaders/common.h"] = (engine_assets / "shaders/common.h").read_bytes()
    relationships = [probe(), probe(0.25), probe(), probe(0.1), probe(flow=(255,127,0,0)),
                     probe(flow=(255,127,255,255)), probe(phase=64), probe(phase=128),
                     probe(flow=(127,127,0,255)), probe(opacity=0), probe(opacity=128), probe()]
    relationships[2]["constants"].pop("speed")
    relationships[2]["constants"].pop("strength")
    colors = ((0,127,0,255),(64,127,0,255),(127,127,0,255),(128,127,0,255),
              (192,127,0,255),(255,127,0,255),(255,127,128,255),(255,127,255,255),
              (255,127,0,0),(255,127,0,128),(127,255,0,255),(127,0,0,255))
    groups = {"periodic":[probe()], "relationships":relationships,
              "calibration":[probe(0.25,flow=c,speed=0,bounds="0 0.25") for c in colors],
              "bounds":[probe(0.25,speed=0,bounds=b) for b in
                        ("0 1","0 0.25","0 0.5","0.25 0.75","0.75 1","0.9 0.99")]}
    pixels = bytes(c for y in range(256) for x in range(256)
                   for c in (x,y,255 if x%32 == 0 or y%32 == 0 else 0,255))
    for name, cases in groups.items():
        columns = min(3,len(cases))
        width, height = columns*256, math.ceil(len(cases)/columns)*256
        scene = {"version":5, "general":{"orthogonalprojection":{"width":width,"height":height},
                 "clearcolor":"0 0 0", "clearenabled":True, "hdr":False, "bloom":False,
                 "ambientcolor":"1 1 1", "skylightcolor":"1 1 1", "camerafade":False,
                 "cameraparallax":False, "nearz":0.01, "farz":10000, "zoom":1},
                 "camera":{"center":"0 0 -1", "eye":"0 0 0", "up":"0 1 0"}, "objects":[]}
        entries = {**vendor, "materials/image.tex":texture(256,256,pixels)}
        for i, case in enumerate(cases):
            scene["objects"].append({"id":i+1, "name":f"{name}-{i}", "image":"models/image.json",
                "origin":f"{i%columns*256+128} {height-i//columns*256-128} 0", "size":"256 256",
                "effects":[{"file":"effects/shake/effect.json", "passes":[{
                    "combos":{"AUDIOPROCESSING":0,"DIRECTION":0,"NOISE":0},
                    "constantshadervalues":case["constants"],
                    "textures":[None,f"flow-{i}",f"phase-{i}",f"opacity-{i}"]}]}]})
            for kind, color in (("flow",case["flow"]),("phase",[case["phase"]]*3+[255]),
                                ("opacity",[case["opacity"]]*3+[255])):
                entries[f"materials/{kind}-{i}.tex"] = texture(1,1,bytes(color))
        values = {"scene.json":scene, "project.json":{"type":"scene","file":"scene.json","title":f"Generated Shake {name}"},
                  "models/image.json":{"autosize":True,"material":"materials/image.json"},
                  "materials/image.json":{"passes":[{"shader":"genericimage4","textures":["image"],
                      "blending":"normal","cullmode":"nocull","depthtest":"disabled","depthwrite":"disabled"}]}}
        entries.update({asset:json.dumps(value).encode() for asset,value in values.items()})
        write_project(output/name,entries)
    (output/"probes.json").write_text(json.dumps({"groups":groups,
        "vendor_inputs":{p:hashlib.sha256(data).hexdigest() for p,data in vendor.items()}},indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output",type=Path)
    parser.add_argument("engine_assets",type=Path)
    args = parser.parse_args()
    prepare(args.output,args.engine_assets)
