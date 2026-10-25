"""Inventory scene property links and script identities without executing scripts."""
import argparse
import hashlib
import json
import runpy
from collections import Counter
from pathlib import Path


def bindings(root):
    inventory = runpy.run_path(str(Path(__file__).with_name('inventory-workshop.py')))['inventory']
    rows = []
    for folder in sorted(root.iterdir(), key=lambda p: p.name):
        if not folder.is_dir() or not folder.name.isdigit():
            continue
        data = inventory(folder)
        scene = data.get('json', {}).get('scene.json', data.get('scene', {}))
        for obj in scene.get('objects', []):
            for field, value in obj.items():
                if not isinstance(value, dict) or 'value' not in value:
                    continue
                script = value.get('script')
                rows.append(dict(folder=folder.name, object_id=obj.get('id'), field=field,
                    source_sha256=data.get('source_sha256'), binding=value,
                    script_sha256=hashlib.sha256(script.encode()).hexdigest() if isinstance(script, str) else None))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    artifacts = Path(__file__).resolve().parents[1] / 'artifacts'
    output = args.output.resolve()
    if output.exists() or not output.is_relative_to(artifacts) or output == artifacts:
        parser.error('output must be a new directory under ignored artifacts/')
    rows = bindings(args.root)
    summary = dict(total=len(rows), wallpapers=len({r['folder'] for r in rows}),
        fields=dict(Counter(r['field'] for r in rows)),
        direct_links=sum('user' in r['binding'] and 'script' not in r['binding'] for r in rows),
        script_profiles=len({r['script_sha256'] for r in rows if r['script_sha256']}))
    output.mkdir(parents=True)
    (output / 'bindings.json').write_text(json.dumps(rows, indent=2, ensure_ascii=False), encoding='utf-8')
    (output / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
