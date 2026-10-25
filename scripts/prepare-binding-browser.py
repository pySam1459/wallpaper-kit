"""Generate local binding probes from original images and checked script identities."""
import argparse
import json
import runpy
from pathlib import Path

PROFILES = {
    '25048b0e1368f6f68ac7264964ce0b8ded11107a06ae797f1fb0da0fafca682b',
    '7398f676f302c25a0db2c2b0ee8bcbbb31be0edcdc8d7c516854e942fe790d2d',
    '21da5a871a27f7c017c9810b364beb5effe027548767d18b6bb4683fc687e11c',
    'd5ccb4c690159bee3cc362bf7d7703c56d581b89aa5e7f595c3304ab0bebb328',
    'ebf5e5f476ec0a0e35c9dd5c77a9691e23468d24b58a465488157c9e026e4912',
    'b73c5f496cbb5bdd2c582c7ace579f0afb0798843e93b8468d8768167d6cda02',
    'dca4b368c3630dec922fd4015afec4962069827b2d45d18af83ff3ab80608f67',
    '2bca0f3a950267440fe733611caee5481f7d60617b318a78ffd60235e29ce1bb',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--workshop-root', type=Path, default=Path('C:/Program Files (x86)/Steam/steamapps/workshop/content/431960'))
    parser.add_argument('--font', type=Path, default=Path('C:/Windows/Fonts/arial.ttf'))
    parser.add_argument('--engine-assets', type=Path, default=Path('C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'))
    args = parser.parse_args()
    root = args.output.resolve()
    artifacts = Path(__file__).resolve().parents[1] / 'artifacts'
    if root.exists() or root == artifacts or not root.is_relative_to(artifacts):
        parser.error('output must be a new directory under ignored artifacts/')
    rows = runpy.run_path(str(Path(__file__).with_name('inventory-bindings.py')))['bindings'](args.workshop_root)
    chosen = {r['script_sha256']: r for r in rows if r['field'] == 'text' and r['script_sha256'] in PROFILES}
    if set(chosen) != PROFILES:
        parser.error('the local collection does not contain all eight checked reference profiles')
    helpers = runpy.run_path(str(Path(__file__).with_name('prepare-shake-reference.py')))
    cases = []
    for index, (digest, row) in enumerate(sorted(chosen.items())):
        binding = row['binding']
        options = dict(binding.get('scriptproperties', {}))
        if 'use24hFormat' in options:
            options.update(showSeconds=dict(user='seconds',value=True),
                           use24hFormat=dict(user='format24',value=index % 2 == 0), delimiter=':')
        cases.append(dict(name=digest[:12], digest=digest, script=binding['script'], scriptproperties=options))
    cases.append(dict(cases[0], name='blurred-color', blur=True))
    for case in cases:
        folder = root / case['name']
        shared = dict(font='fonts/probe.ttf', pointsize=10, origin='128 64 0',
                      color=dict(user='tint', value='1 1 1'))
        effects = [dict(file='effects/blurprecise/effect.json')] if case.get('blur') else []
        entries = {
            'scene.json': dict(version=5, general=dict(orthogonalprojection=dict(width=256,height=128)), objects=[
                dict(id=1, image='models/image.json', origin='128 64 0', size='256 128', visible=dict(user='base',value=True)),
                dict(id=2, **shared, visible=dict(user='clock',value=True), effects=effects,
                     text=dict(value='stale',script=case['script'],scriptproperties=case['scriptproperties'])),
                dict(id=3, **shared, visible=dict(user='static',value=False), effects=effects, text=dict(user='caption',value='stale'))]),
            'models/image.json': dict(material='materials/image.json', autosize=True),
            'materials/image.json': dict(passes=[dict(shader='genericimage4',textures=['image'],
                blending='translucent',cullmode='nocull',depthtest='disabled',depthwrite='disabled')]),
        }
        encoded = {name: json.dumps(data).encode() for name,data in entries.items()}
        encoded.update({'materials/image.tex':helpers['texture'](1,1,bytes([10,20,30,255])),
                        'fonts/probe.ttf':args.font.read_bytes()})
        if case.get('blur'):
            definition = dict(version=1,replacementkey='blur_precise',fbos=[dict(name='_rt_probe',format='rgba_backbuffer')],
                passes=[dict(material='materials/blur-x.json',target='_rt_probe'),dict(material='materials/blur-y.json',
                    bind=[dict(name='_rt_probe',index=0),dict(name='previous',index=1)])])
            encoded['effects/blurprecise/effect.json'] = json.dumps(definition).encode()
            for axis in ['x','y']:
                encoded[f'materials/blur-{axis}.json'] = json.dumps(dict(passes=[dict(shader='effects/blur_precise_gaussian',
                    combos=dict(VERTICAL=int(axis=='y'),ENABLEMASK=int(axis=='y')),
                    blending='normal',depthtest='disabled',depthwrite='disabled',cullmode='nocull')])).encode()
            # Local identity-check inputs only. No installed shader is committed.
            for asset in ['shaders/effects/blur_precise_gaussian.frag','shaders/effects/blur_precise_gaussian.vert','shaders/common_blur.h']:
                source = args.engine_assets / asset
                if not source.exists():
                    source = args.engine_assets / 'effects/blurprecise' / asset
                encoded[asset] = source.read_bytes()
        helpers['write_project'](folder,encoded)
        case['package'] = str(folder / 'scene.pkg')
    (root / 'fixtures.json').write_text(json.dumps(dict(cases=cases),indent=2),encoding='utf-8')
    print(root)


if __name__ == '__main__':
    main()
