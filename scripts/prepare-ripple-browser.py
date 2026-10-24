"""Generate original planar Water Ripple inputs for the browser shader oracle."""
import argparse
import base64
import json
import math
import runpy
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    root = args.output.resolve()
    artifacts = Path(__file__).resolve().parents[1] / 'artifacts'
    if root.exists() or root == artifacts or not root.is_relative_to(artifacts):
        parser.error('output must be a new directory under ignored artifacts/')
    helpers = runpy.run_path(str(Path(__file__).with_name('prepare-shake-reference.py')))
    texture, write_project = helpers['texture'], helpers['write_project']
    width, height = 128, 96
    pixels = bytes(v for y in range(height) for x in range(width)
                   for v in [x * 2, y * 2, x + y, 255])
    normal = bytes(v for y in range(8) for x in range(8) for v in [
        round(128 + 80 * math.cos(x * math.tau / 8)),
        round(128 + 80 * math.sin(y * math.tau / 8)),
        round(210 + 20 * math.cos((x + y) * math.tau / 8)), 255])
    cases = [
        dict(name='defaults', constants={}),
        dict(name='authored', constants=dict(animationspeed=.25, ratio=.25,
             ripplestrength=.15, scale=.6, scrolldirection=-2.2989695, scrollspeed=0)),
        dict(name='rotated', constants=dict(animationspeed=.4, scrollspeed=.3,
             scrolldirection=.8, scale=1.7, ratio=2.1, ripplestrength=.3)),
        dict(name='vertical-scroll', constants=dict(animationspeed=0, scrollspeed=.3,
             scrolldirection=math.pi / 2, ripplestrength=.4)),
        dict(name='negative-speeds', constants=dict(animationspeed=-.3, scrollspeed=-.2, ripplestrength=.3)),
        dict(name='zero-speed', constants=dict(animationspeed=0, scrollspeed=0)),
        dict(name='zero-strength', constants=dict(ripplestrength=0)),
        dict(name='zero-scale', constants=dict(scale=0)),
        dict(name='zero-ratio', constants=dict(ratio=0)),
        dict(name='mask-disabled', constants=dict(ripplestrength=.3), combos=dict(MASK=0)),
        dict(name='unmasked', constants=dict(ripplestrength=.3), mask=False),
        dict(name='soft-mask', constants=dict(ripplestrength=.3), soft=True),
        dict(name='transparent', constants=dict(ripplestrength=.3), transparent=True),
        dict(name='builtin-default', constants={}, builtin=True),
        dict(name='normal-override', constants=dict(ripplestrength=.3), override=True),
    ]
    for case in cases:
        folder = root / case['name']
        mask = bytes(v for y in range(height) for x in range(width)
                     for v in [x * 2 if case.get('soft') else (255 if x >= width // 2 else 0), 0, 0, 255])
        case['mask_rgba'] = base64.b64encode(mask).decode()
        case['masked'] = case.get('mask', True) and case.get('combos', {}).get('MASK') != 0
        case['package'] = str(folder / 'scene.pkg')
        image = bytes(v for y in range(height) for x in range(width)
                      for v in [x * 2, y * 2, x + y, x * 2]) if case.get('transparent') else pixels
        if case.get('transparent'):
            case['rgba'] = base64.b64encode(image).decode()
        if case.get('override'):
            case['normal_rgba'] = base64.b64encode(bytes([240, 80, 255, 255]) * 64).decode()
        effect = dict(file='effects/waterripple/effect.json', passes=[dict(
            constantshadervalues=case['constants'], combos=case.get('combos', {}),
            textures=[None, 'mask' if case.get('mask', True) else None,
                      'custom-normal' if case.get('override') else None])])
        entries = {
            'scene.json': dict(version=5, general=dict(orthogonalprojection=dict(width=width, height=height),
                clearcolor='0 0 0', clearenabled=True, hdr=False), objects=[dict(id=1,
                image='models/image.json', origin='64 48 0', size='128 96', effects=[effect])]),
            'models/image.json': dict(material='materials/image.json', autosize=True),
            'materials/image.json': dict(passes=[dict(shader='genericimage2', blending='translucent',
                cullmode='nocull', depthtest='disabled', depthwrite='disabled', textures=['image'])]),
            'effects/waterripple/effect.json': dict(version=1, replacementkey='waterripple',
                passes=[dict(material='materials/effect.json')]),
            'materials/effect.json': dict(passes=[dict(shader='effects/waterripple',
                **({} if case.get('builtin') else dict(textures=[None, None, 'material-normal'])))]),
        }
        encoded = {name: json.dumps(data).encode() for name, data in entries.items()}
        encoded.update({'materials/image.tex': texture(width, height, image),
                        'materials/mask.tex': texture(width, height, mask),
                        'materials/material-normal.tex': texture(8, 8, normal),
                        'materials/effects/waterripplenormal.tex': texture(8, 8, normal),
                        'materials/custom-normal.tex': texture(8, 8, bytes([240, 80, 255, 255]) * 64)})
        write_project(folder, encoded)
    (root / 'fixtures.json').write_text(json.dumps(dict(effect='waterripple', width=width, height=height,
        rgba=base64.b64encode(pixels).decode(), normal_rgba=base64.b64encode(normal).decode(), cases=cases), indent=2))
    print(root)


if __name__ == '__main__':
    main()
