"""Generate original scene-composition probes using the existing image-chain inputs."""
import argparse
import copy
import json
import runpy
import subprocess
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    root = args.output.resolve()
    artifacts = Path(__file__).resolve().parents[1] / 'artifacts'
    if root.exists() or root == artifacts or not root.is_relative_to(artifacts):
        parser.error('output must be a new directory under ignored artifacts/')
    subprocess.run([sys.executable, '-B', str(Path(__file__).with_name('prepare-effect-chain-browser.py')), str(root)], check=True)
    manifest = json.loads((root / 'fixtures.json').read_text())
    manifest['effect'] = 'composition'
    extra = copy.deepcopy(manifest['cases'][1])
    extra.update(name='hdr', hdr=True, chain=extra['chain'][:1])
    manifest['cases'].append(extra)
    helpers = runpy.run_path(str(Path(__file__).with_name('prepare-shake-reference.py')))
    templates = {case['name']:{str(p.relative_to(root / case['name'])).replace('\\','/'):p.read_bytes()
        for p in (root / case['name']).rglob('*') if p.is_file() and p.suffix in ['.json','.tex']}
        for case in manifest['cases'] if not case.get('hdr')}
    for case in manifest['cases']:
        encoded = dict(templates['reversed' if case['name'] == 'hdr' else case['name']])
        scene = json.loads(encoded['scene.json'])
        base = scene['objects'][0]
        effects = base.pop('effects')
        if case.get('hdr'):
            effects = effects[:1]
        case.update(center=[64,48], size=[128,96], angle=0, composition=True,
                    mask_width=128,mask_height=96,image_opacity=base.get('alpha',1))
        base.pop('brightness',None)
        if case['name'] in ['rotated','three-pass','soft-mask']:
            case['size'] = [64,48]
        if case['name'] == 'rotated':
            case['angle'] = 90
        if case['name'] == 'soft-mask':
            case['center'] = [16,48]
        full = case['size'] == [128,96]
        case['full'] = full
        point = dict(id=10,image='models/full.json' if full else 'models/local.json',
            origin=f"{case['center'][0]} {96-case['center'][1]} 0",size=' '.join(map(str,case['size'])),
            angles=f"0 0 {case['angle']}",alpha=case.get('opacity',1),brightness=case.get('brightness',1),effects=effects)
        case['before'] = [48,32,9,7,[7,39,243,255]]
        case['after'] = [62,46,5,9,[235,31,47,255]]
        def overlay(ident, spec, material):
            x,y,w,h,_ = spec
            return dict(id=ident,image=f'models/{material}.json',origin=f'{x+w/2} {96-y-h/2} 0',size=f'{w} {h}')
        scene['objects'] = [base,overlay(2,case['before'],'before'),point,overlay(3,case['after'],'after')]
        scene['general']['hdr'] = case.get('hdr',False)
        scene['general']['bloom'] = False
        encoded['scene.json'] = json.dumps(scene).encode()
        for name,flags in [('full',dict(projectlayer=True)),('local',{})]:
            encoded[f'models/{name}.json'] = json.dumps(dict(material='materials/util/composelayer.json',passthrough=True,**flags)).encode()
        for name in ['before','after']:
            encoded[f'models/{name}.json'] = json.dumps(dict(material=f'materials/{name}.json')).encode()
            encoded[f'materials/{name}.json'] = json.dumps(dict(passes=[dict(shader='genericimage4',combos={},textures=[name],
                blending='translucent',cullmode='nocull',depthtest='disabled',depthwrite='disabled')])).encode()
            encoded[f'materials/{name}.tex'] = helpers['texture'](1,1,bytes(case[name][4]))
        folder = root / case['name']
        helpers['write_project'](folder,encoded)
        case['package'] = str(folder / 'scene.pkg')
    (root / 'fixtures.json').write_text(json.dumps(manifest,indent=2))
    print(root)


if __name__ == '__main__':
    main()
