"""Inventory particle definitions behind current Workshop omissions without editing assets."""
import argparse
import hashlib
import json
import re
import runpy
from collections import defaultdict
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text(encoding='utf-8-sig'))
    inventory = runpy.run_path(str(Path(__file__).with_name('inventory-workshop.py')))['inventory']
    root, engine = Path(report['workshop_root']), Path(report['engine_assets'])
    records, groups = [], defaultdict(set)
    for wallpaper in report['wallpapers']:
        playback = (wallpaper.get('evidence') or {}).get('playback', {})
        names = defaultdict(list)
        for notice in playback.get('omissions', []):
            match = re.search(r'(particles/[^:]+\.json): (.+)', notice)
            if match:
                names[match[1]].append(match[2])
        if not names:
            continue
        source = inventory(root / wallpaper['id'])
        if source.get('source_sha256') != playback.get('package_sha256'):
            raise ValueError(f"{wallpaper['id']}: source changed; rerun the support probe")
        for name, notices in sorted(names.items()):
            value = source.get('json', {}).get(name)
            path = engine / name
            if value is None:
                value = json.loads(path.read_text(encoding='utf-8-sig'))
            digest = hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()
            records.append(dict(id=wallpaper['id'], asset=name, notices=notices,
                                definition_sha256=digest, definition=value))
            for emitter in value.get('emitter', []):
                groups[(emitter.get('name'), tuple(sorted(emitter)))].add(wallpaper['id'])
    result = dict(records=records, emitter_shapes=[
        dict(name=name, fields=fields, ids=sorted(ids), affected_wallpapers=len(ids))
        for (name, fields), ids in sorted(groups.items(), key=lambda item: (-len(item[1]), str(item[0])))])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result['emitter_shapes'], indent=2))


if __name__ == '__main__':
    main()
