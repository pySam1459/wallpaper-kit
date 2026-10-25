"""Inventory composition points and effect families without modifying Workshop content."""
import argparse
import json
import runpy
from collections import Counter
from pathlib import Path


def inventory(root):
    read = runpy.run_path(str(Path(__file__).with_name('inventory-workshop.py')))['inventory']
    rows = []
    for folder in sorted(root.iterdir(),key=lambda p:p.name):
        if not folder.is_dir() or not folder.name.isdigit():
            continue
        data = read(folder)
        values = data.get('json',{})
        scene = values.get('scene.json',data.get('scene',{}))
        for obj in scene.get('objects',[]):
            name = obj.get('image') or ''
            model = values.get(name,{})
            if (name.lower() not in ['models/util/fullscreenlayer.json','models/util/projectlayer.json','models/util/composelayer.json']
                and not model.get('projectlayer') and not model.get('fullscreen')
                and model.get('material','').lower() != 'materials/util/composelayer.json'):
                continue
            rows.append(dict(folder=folder.name,source_sha256=data.get('source_sha256'),object=obj,model=model))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root',type=Path)
    parser.add_argument('output',type=Path)
    args = parser.parse_args()
    target = args.output.resolve()
    artifacts = Path(__file__).resolve().parents[1] / 'artifacts'
    if target.exists() or target == artifacts or not target.is_relative_to(artifacts):
        parser.error('output must be a new directory under ignored artifacts/')
    rows = inventory(args.root)
    families = Counter()
    for row in rows:
        for effect in row['object'].get('effects',[]):
            visible = effect.get('visible',True)
            if isinstance(visible,dict):
                visible = visible.get('value',True)
            if visible is not False:
                families[effect['file'].split('/')[-2]] += 1
    summary = dict(points=len(rows),wallpapers=len({r['folder'] for r in rows}),active_effects=dict(families))
    target.mkdir(parents=True)
    (target / 'points.json').write_text(json.dumps(rows,indent=2,ensure_ascii=False),encoding='utf-8')
    (target / 'summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
