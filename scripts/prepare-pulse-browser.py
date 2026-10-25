"""Generate original periodic Pulse inputs for the installed-shader browser oracle."""
import argparse
import base64
import json
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
    pixels = bytes(v for y in range(height) for x in range(width) for v in [x*2, y*2, x+y, 255])
    noise = bytes(v for y in range(8) for x in range(8) for v in [(x*47+y*29)%256, 0, 0, 255])
    cases = [
        dict(name='defaults', constants={}),
        dict(name='rotated', constants=dict(speed=2.01, amount=.79, power=1.75,
             tinthigh='1 .533333333 .149019608')),
        dict(name='hard-light', constants=dict(speed=6.34, amount=2, tinthigh='1 1 0'), combos=dict(BLENDMODE=13)),
        dict(name='bounds-phase', constants=dict(bounds='.2 .8', phase=1.52, amount=.36, speed=2.08)),
        dict(name='noise', constants=dict(noiseamount=1.63, noisespeed=.1)),
        dict(name='noise-only', constants=dict(amount=0, noiseamount=.8, noisespeed=.5, power=.29)),
        dict(name='soft-mask', constants=dict(tintlow='.3 .8 .9', tinthigh='.9 .2 .6'), soft=True),
        dict(name='alpha', constants=dict(amount=.6), combos=dict(PULSECOLOR=0, PULSEALPHA=1), transparent=True),
        dict(name='color-alpha', constants=dict(amount=.8), combos=dict(PULSEALPHA=1), transparent=True),
        dict(name='overbright-alpha', constants=dict(amount=2,noiseamount=1), combos=dict(PULSEALPHA=1), transparent=True),
        dict(name='unmasked', constants={}, mask=False),
        dict(name='mask-disabled', constants={}, combos=dict(MASK=0)),
        dict(name='low-power', constants=dict(power=.01)),
        dict(name='zero-speed', constants=dict(speed=0, phase=1)),
        dict(name='zero-amount', constants=dict(amount=0)),
        dict(name='zero-noise-missing', constants=dict(noiseamount=0), missing_noise=True),
        dict(name='hdr', constants=dict(amount=1.5), hdr=True),
        dict(name='composition', constants=dict(speed=2.01, power=1.75), composition=True),
    ]
    for case in cases:
        folder = root / case['name']
        image = bytes(v for y in range(height) for x in range(width)
                      for v in [x*2, y*2, x+y, x*2]) if case.get('transparent') else pixels
        mask = bytes(v for y in range(height) for x in range(width)
                     for v in [x*2 if case.get('soft') else (255 if x>=width//2 else 0), 0, 0, 255])
        case.update(package=str(folder/'scene.pkg'), masked=case.get('mask',True) and case.get('combos',{}).get('MASK')!=0,
                    rgba=base64.b64encode(image).decode(), mask_rgba=base64.b64encode(mask).decode())
        effect = dict(file='effects/pulse/effect.json', passes=[dict(constantshadervalues=case['constants'],
            combos=case.get('combos',{}), textures=[None, 'missing' if case.get('missing_noise') else 'noise',
                'mask' if case.get('mask',True) else None])])
        base = dict(id=1,image='models/image.json',origin='64 48 0',size='128 96',effects=[effect])
        objects = [base]
        if case.get('composition'):
            base.pop('effects')
            objects.append(dict(id=10,image='models/full.json',effects=[effect]))
        entries = {
            'scene.json': dict(version=5,general=dict(orthogonalprojection=dict(width=width,height=height),
                clearcolor='0 0 0',clearenabled=True,hdr=case.get('hdr',False),bloom=False),objects=objects),
            'models/image.json': dict(material='materials/image.json',autosize=True),
            'models/full.json': dict(projectlayer=True),
            'materials/image.json': dict(passes=[dict(shader='genericimage2',blending='translucent',
                cullmode='nocull',depthtest='disabled',depthwrite='disabled',textures=['image'])]),
            'effects/pulse/effect.json': dict(version=1,replacementkey='pulse',passes=[dict(material='materials/effect.json')]),
            'materials/effect.json': dict(passes=[dict(shader='effects/pulse')]),
        }
        encoded = {name:json.dumps(value).encode() for name,value in entries.items()}
        encoded.update({'materials/image.tex':texture(width,height,image), 'materials/mask.tex':texture(width,height,mask),
            'materials/noise.tex':texture(8,8,noise)})
        write_project(folder,encoded)
    (root/'fixtures.json').write_text(json.dumps(dict(effect='pulse',width=width,height=height,
        rgba=base64.b64encode(pixels).decode(),noise_rgba=base64.b64encode(noise).decode(),cases=cases),indent=2))
    print(root)


if __name__ == '__main__':
    main()
