"""Read-only PKG index/layout and scene feature inventory, stored as local evidence."""
import argparse
import hashlib
import json
import struct
from pathlib import Path


def inventory(folder):
    project_bytes = (folder / "project.json").read_bytes()
    project = json.loads(project_bytes.decode("utf-8-sig"))
    result = {"id": folder.name, "project": project,
              "project_sha256": hashlib.sha256(project_bytes).hexdigest()}
    package = folder / "scene.pkg"
    if not package.exists():
        package = folder / Path(project["file"]).with_suffix(".pkg")
    if not package.exists():
        result["files"] = [str(p.relative_to(folder)) for p in folder.rglob("*") if p.is_file()]
        filename = project.get("file", "")
        if filename and Path(filename).name == filename and (folder / filename).is_file():
            with (folder / filename).open("rb") as stream:
                result["source_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
        if project["type"].lower() == "scene" and (folder / project["file"]).exists():
            result["scene"] = json.loads((folder / project["file"]).read_text(encoding="utf-8-sig"))
        return result
    with package.open("rb") as stream:
        result["source_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    with package.open("rb") as stream:
        def u32():
            return struct.unpack("<I", stream.read(4))[0]
        def string():
            length = u32()
            assert length <= 255
            return stream.read(length).decode("utf-8")
        signature = string()
        count = u32()
        assert count <= 65536
        entries = [(string(), u32(), u32()) for _ in range(count)]
        start = stream.tell()
        length = package.stat().st_size
        assert all(start + offset + size <= length for _, offset, size in entries)
        ranges = sorted((offset, offset + size) for _, offset, size in entries if size)
        assert all(a[1] <= b[0] for a, b in zip(ranges, ranges[1:]))
        values, textures = {}, {}
        for name, offset, size in entries:
            if name.endswith(".json"):
                stream.seek(start + offset)
                try:
                    values[name] = json.loads(stream.read(size))
                except (ValueError, UnicodeDecodeError) as error:
                    values[name] = {"inventory_error": str(error)}
            elif name.endswith(".tex"):
                stream.seek(start + offset)
                header = stream.read(min(size, 64))
                textures[name] = {"bytes": size, "header_hex": header.hex()}
                if len(header) >= 26 and struct.unpack_from("<I", header, 22)[0] & 32:
                    stream.seek(start + offset + 55)
                    textures[name]["video_layout_hex"] = stream.read(min(size - 55, 96)).hex()
                if folder.name == "843532366":
                    stream.seek(start + offset + max(0, size - 4096))
                    tail = stream.read(min(size, 4096))
                    marker = tail.find(b"TEXS")
                    textures[name]["sprite_header_hex"] = tail[marker:marker + 96].hex() if marker >= 0 else "not in last 4096 bytes"
        result.update(signature=signature, index_layout="relative offset/length, validated bounds and non-overlap", json=values, textures=textures)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    results = []
    for folder in sorted(args.root.iterdir(), key=lambda p: p.name):
        if folder.is_dir() and folder.name.isdigit():
            results.append(inventory(folder))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding="utf-8")
    for result in results:
        scene = result.get("json", {}).get("scene.json", result.get("scene", {}))
        print(result["id"], result.get("signature", result.get("files")), "scene version", scene.get("version"))


if __name__ == "__main__":
    main()
