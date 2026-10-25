"""Generate original ordered image-pass fixtures for the installed-shader oracle."""
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
    width, height = 128, 96
    pixels = bytes(v for y in range(height) for x in range(width)
                   for v in [x * 2, y * 2, (x + y) % 256, 255])
    normal = bytes(v for y in range(8) for x in range(8) for v in [
        round(128 + 80 * math.cos(x * math.tau / 8)),
        round(128 + 80 * math.sin(y * math.tau / 8)), 230, 255])
    noise = bytes(v for y in range(8) for x in range(8)
                  for v in [17, (x * 37 + y * 19) % 256, 255, 255])
    foliage = dict(effect='foliagesway', constants=dict(strength=1, speeduv=4,
                   phase=.7, power=.8, ratio=.8, scale=.3, scrolldirection=.4))
    ripple = dict(effect='waterripple', constants=dict(ripplestrength=.35,
                  animationspeed=.3, scrollspeed=.1, ratio=.7, scale=.9))
    cases = [
        dict(name='rotated', chain=[foliage, ripple]),
        dict(name='reversed', chain=[ripple, foliage]),
        dict(name='three-pass', chain=[foliage, ripple, foliage]),
        dict(name='four-pass', chain=[ripple, foliage, ripple, foliage]),
        dict(name='soft-mask', chain=[foliage, ripple], soft=True),
        dict(name='transparent', chain=[ripple, foliage, ripple], transparent=True,
             opacity=.6, brightness=1.4),
        dict(name='unsupported-middle', chain=[foliage, ripple], unsupported=True),
        dict(name='missing-normal', chain=[foliage, foliage], missing=True),
    ]
    for case in cases:
        folder = root / case['name']
        mask = bytes(v for y in range(height) for x in range(width) for v in [
            x * 2 if case.get('soft') else (255 if x >= width // 2 else 0), 0, 0, 255])
        image = bytes(v for y in range(height) for x in range(width)
                      for v in [x * 2, y * 2, (x + y) % 256, x * 2]) if case.get('transparent') else pixels
        case.update(package=str(folder / 'scene.pkg'), rgba=base64.b64encode(image).decode(),
                    mask_rgba=base64.b64encode(mask).decode(), masked=True)
        effects = [dict(file=f"effects/{item['effect']}/effect.json", passes=[dict(
            constantshadervalues=item['constants'], textures=[None, 'mask',
                'normal' if item['effect'] == 'waterripple' else 'noise'])]) for item in case['chain']]
        if case.get('unsupported'):
            effects.insert(1, dict(file='effects/unknown/effect.json'))
        if case.get('missing'):
            effects.insert(1, dict(file='effects/waterripple/effect.json', passes=[dict(textures=[None, None, 'missing-normal'])]))
        entries = {
            'scene.json': dict(version=5, general=dict(orthogonalprojection=dict(width=width, height=height),
                clearcolor='0 0 0', clearenabled=True, hdr=False), objects=[dict(id=1,
                image='models/image.json', origin='64 48 0', size='128 96', effects=effects,
                alpha=case.get('opacity', 1), brightness=case.get('brightness', 1))]),
            'models/image.json': dict(material='materials/image.json', autosize=True),
            'materials/image.json': dict(passes=[dict(shader='genericimage2', blending='translucent',
                cullmode='nocull', depthtest='disabled', depthwrite='disabled', textures=['image'])]),
        }
        for kind in ['foliagesway', 'waterripple']:
            entries[f'effects/{kind}/effect.json'] = dict(version=1, replacementkey=kind,
                passes=[dict(material=f'materials/{kind}.json')])
            entries[f'materials/{kind}.json'] = dict(passes=[dict(shader=f'effects/{kind}')])
        encoded = {name: json.dumps(data).encode() for name, data in entries.items()}
        encoded.update({f'materials/{name}.tex': helpers['texture'](w, h, data) for name, w, h, data in [
            ('image', width, height, image), ('mask', width, height, mask),
            ('normal', 8, 8, normal), ('noise', 8, 8, noise)]})
        helpers['write_project'](folder, encoded)
    (root / 'fixtures.json').write_text(json.dumps(dict(effect='chain', width=width, height=height,
        normal_rgba=base64.b64encode(normal).decode(), noise_rgba=base64.b64encode(noise).decode(), cases=cases), indent=2))
    print(root)


if __name__ == '__main__':
    main()
