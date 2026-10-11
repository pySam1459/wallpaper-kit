"""Read-only JSON/TEX inventory; writes receipts, never modifies source assets."""
import argparse
import hashlib
import json
import struct
from pathlib import Path


class Reader:
    def __init__(self, data):
        self.data, self.offset = data, 0

    def take(self, length):
        end = self.offset + length
        if end > len(self.data):
            raise ValueError("truncated input")
        value = self.data[self.offset:end]
        self.offset = end
        return value

    def u32(self):
        return struct.unpack("<I", self.take(4))[0]

    def signature(self):
        return self.take(9).rstrip(b"\0").decode("ascii")


def texture(data):
    reader = Reader(data)
    signatures = [reader.signature(), reader.signature()]
    header = [reader.u32() for _ in range(7)]
    container = reader.signature()
    version = int(container[-4:])
    count = reader.u32()
    codec = reader.u32() if version >= 3 else 0xFFFFFFFF
    video = reader.u32() if version == 4 else 0
    pages = []
    for _ in range(count):
        mips = []
        for _ in range(reader.u32()):
            extent = [reader.u32(), reader.u32()]
            compression = reader.u32() if version >= 2 else 0
            declared = reader.u32() if version >= 2 else 0
            stored = reader.u32()
            payload = reader.take(stored)
            mips.append(dict(extent=extent, compression=compression,
                             declared=declared, stored=stored,
                             prefix=payload[:12].hex()))
        pages.append(mips)
    sprite = None
    if header[1] & 4:
        signature = reader.signature()
        count = reader.u32()
        extent = [reader.u32(), reader.u32()]
        frames = []
        for _ in range(count):
            frames.append(struct.unpack("<I7f", reader.take(32)))
        sprite = dict(signature=signature, extent=extent, frames=frames)
    return dict(signatures=signatures+[container], header=header, codec=codec,
                video=video, pages=pages, sprite=sprite,
                trailing=len(data)-reader.offset)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("engine", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--reference", type=Path, help="Optional ignored local engine reference project")
    parser.add_argument("--objects", type=int, nargs="*", help="Object IDs retained in the reference scene")
    args = parser.parse_args()
    data = args.package.read_bytes()
    reader = Reader(data)
    signature = reader.take(reader.u32()).decode("ascii")
    entries = []
    for _ in range(reader.u32()):
        name = reader.take(reader.u32()).decode("utf-8")
        entries.append((name, reader.u32(), reader.u32()))
    start = reader.offset
    values, textures = {}, {}
    for name, offset, length in entries:
        if start+offset+length > len(data):
            raise ValueError(f"out-of-range asset {name}")
        payload = data[start+offset:start+offset+length]
        if args.reference:
            root = args.reference.resolve()
            destination = (root/name).resolve()
            if not destination.is_relative_to(root) or destination == root:
                raise ValueError(f"invalid asset path {name}")
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(payload)
        if name.endswith(".json"):
            values[name] = json.loads(payload)
        elif name.endswith(".tex"):
            textures[name] = texture(payload)
    for name in ["particles/presets/leaves1.json", "particles/presets/leaves2.json",
                 "materials/presets/leaves1.json", "materials/presets/leaves2.json"]:
        if name not in values:
            values[name] = json.loads((args.engine/name).read_bytes())
    for name in ["materials/particle/nature/leaves1.tex", "materials/particle/nature/leaves7.tex"]:
        if name not in textures:
            textures[name] = texture((args.engine/name).read_bytes())
    receipt = dict(package=str(args.package.resolve()), signature=signature,
                   sha256=hashlib.sha256(data).hexdigest(), json=values, textures=textures)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    if args.reference:
        scene = values["scene.json"]
        if args.objects is not None:
            scene["objects"] = [o for o in scene["objects"] if o["id"] in args.objects]
        scene["general"]["clearcolor"] = "0 0 0"
        scene["general"]["cameraparallax"] = False
        (args.reference/"scene.json").write_text(json.dumps(scene, indent=2), encoding="utf-8")
        (args.reference/"project.json").write_text(json.dumps(dict(type="scene", file="scene.json", title="Owned compatibility reference")), encoding="utf-8")
    print(args.output.resolve())


if __name__ == "__main__":
    main()
