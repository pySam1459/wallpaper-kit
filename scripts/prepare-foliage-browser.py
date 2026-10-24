"""Generate original UV sway inputs for the browser's installed-shader oracle."""
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
    noise = bytes(v for y in range(8) for x in range(8)
                  for v in [x * 31, (x * 31 + y * 47) % 256, y * 31, 255])
    cases = [
        dict(name='defaults', constants={}),
        dict(name='authored', constants=dict(phase=.16, power=.7, ratio=.22,
             scale=.02, scrolldirection=0, speeduv=.32, strength=.4)),
        dict(name='rotated', constants=dict(phase=1.2, power=1.3, ratio=.47,
             scale=.23, scrolldirection=.8, speeduv=4.3, strength=.9)),
        dict(name='vertical', constants=dict(phase=.8, power=.7, ratio=2.01,
             scale=.9, scrolldirection=math.pi / 2, speeduv=30, strength=.8)),
        dict(name='reverse', constants=dict(speeduv=-4.59, strength=.9, phase=.64)),
        dict(name='zero-strength', constants=dict(strength=0)),
        dict(name='zero-speed', constants=dict(speeduv=0)),
        dict(name='zero-phase', constants=dict(phase=0, strength=.9)),
        dict(name='zero-scale', constants=dict(scale=0, strength=.9)),
        dict(name='mask-disabled', constants=dict(strength=.9), combos=dict(MASK=0)),
        dict(name='unmasked', constants=dict(strength=.9), mask=False),
        dict(name='soft-mask', constants=dict(strength=.9), soft=True),
        dict(name='transparent', constants=dict(strength=.9, phase=.8), transparent=True),
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
        effect = dict(file='effects/foliagesway/effect.json', passes=[dict(
            constantshadervalues=case['constants'], combos=case.get('combos', {}),
            textures=[None, 'mask' if case.get('mask', True) else None, None])])
        entries = {
            'scene.json': dict(version=5, general=dict(orthogonalprojection=dict(width=width, height=height),
                clearcolor='0 0 0', clearenabled=True, hdr=False), objects=[dict(id=1,
                image='models/image.json', origin='64 48 0', size='128 96', effects=[effect])]),
            'models/image.json': dict(material='materials/image.json', autosize=True),
            'materials/image.json': dict(passes=[dict(shader='genericimage2', blending='translucent',
                cullmode='nocull', depthtest='disabled', depthwrite='disabled', textures=['image'])]),
            'effects/foliagesway/effect.json': dict(version=2, replacementkey='foliagesway',
                passes=[dict(material='materials/effect.json')]),
            'materials/effect.json': dict(passes=[dict(shader='effects/foliagesway')]),
        }
        encoded = {name: json.dumps(data).encode() for name, data in entries.items()}
        encoded.update({'materials/image.tex': texture(width, height, image),
                        'materials/mask.tex': texture(width, height, mask),
                        'materials/util/noise.tex': texture(8, 8, noise)})
        write_project(folder, encoded)
    (root / 'fixtures.json').write_text(json.dumps(dict(width=width, height=height,
        rgba=base64.b64encode(pixels).decode(), noise_rgba=base64.b64encode(noise).decode(),
        cases=cases), indent=2))
    print(root)


if __name__ == '__main__':
    main()
