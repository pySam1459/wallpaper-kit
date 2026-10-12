"""Generate local particle probes for observable engine/library comparisons."""
import argparse
import json
import shutil
import struct
from pathlib import Path


def texture(width, height, pixels, frames=(), format=0):
    data = b'TEXV0005\0TEXI0001\0' + struct.pack('<7I', format, 4 if frames else 0, width, height, width, height, 0)
    data += b'TEXB0004\0' + struct.pack('<9I', 1, 0xffffffff, 0, 1, width, height, 0, len(pixels), len(pixels)) + pixels
    if frames:
        data += b'TEXS0003\0' + struct.pack('<3I', len(frames), 4, 4)
        for index in frames:
            data += struct.pack('<I7f', 0, .25, index * 4, 0, 4, 0, 0, 4)
    return data


def write_json(root, name, value):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2), encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('engine', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--case', help='Generate only the named probe')
    args = parser.parse_args()
    root = args.output
    if root.exists():
        parser.error('output must be a new directory to preserve reference evidence')
    (root / 'materials').mkdir(parents=True, exist_ok=True)
    (root / 'materials/black.tex').write_bytes(texture(512, 256, bytes([16, 16, 16, 255]) * (512 * 256)))
    (root / 'materials/white.tex').write_bytes(texture(1, 1, bytes([255] * 4)))
    colors = [bytes(c) for c in [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (255, 255, 0, 255)]]
    pixels = b''.join(b''.join(c * 4 for c in colors) for _ in range(4))
    (root / 'materials/atlas.tex').write_bytes(texture(16, 4, pixels, range(4)))
    # Asymmetric shape makes angular motion visible without an animated texture.
    pixels = b''.join(bytes([255, 255, 255, 255 if x < 2 or y < 2 else 0]) for y in range(8) for x in range(8))
    (root / 'materials/angle.tex').write_bytes(texture(8, 8, pixels))
    for name, value in [('rg-bright', [255, 128]), ('rg-dark', [64, 255]), ('rg-clear', [255, 0])]:
        (root / f'materials/{name}.tex').write_bytes(texture(1, 1, bytes(value), format=8))
    state = dict(cullmode='nocull', depthtest='disabled', depthwrite='disabled')
    write_json(root, 'models/base.json', dict(material='materials/base.json', autosize=True))
    write_json(root, 'materials/base.json', dict(passes=[dict(state, shader='genericimage4', blending='translucent', combos={}, textures=['black'])]))
    objects = [dict(id=1, image='models/base.json', origin='256 128 0', size='512 256')]
    cases = [
        dict(name='size', x=64, y=192),
        dict(name='alpha', x=192, y=192, alpha=.25),
        dict(name='overbright', x=320, y=192, alpha=.25, brightness=4),
        dict(name='angle', x=448, y=192, texture='angle', angular=1),
        dict(name='sequence-12', x=64, y=64, texture='atlas', life=12),
        dict(name='sequence-120', x=192, y=64, texture='atlas', life=120),
        dict(name='sequence-3x', x=320, y=64, texture='atlas', life=12, multiplier=3),
        dict(name='size-envelope', x=448, y=64, life=12, envelope=True),
        dict(name='trail', x=64, y=128, velocity='2 0 0', drag=0, renderer='spritetrail', color='255 0 0', trail=dict(length=2, minlength=0, maxlength=200)),
        dict(name='trail-default', x=192, y=128, velocity='32 0 0', drag=0, renderer='spritetrail', color='0 255 0', size=8),
        dict(name='trail-clamped', x=320, y=128, velocity='2 0 0', drag=0, renderer='spritetrail', color='0 0 255', trail=dict(length=1, minlength=1, maxlength=1)),
        dict(name='trail-slow-default', x=448, y=128, velocity='2 0 0', drag=0, renderer='spritetrail', color='255 255 0'),
        dict(name='flare', x=256, y=128, size=512, texture='particle/halo', color='255 197 85', alpha=.38),
        dict(name='trail-fast-default', x=256, y=128, size=8, velocity='1000 0 0', renderer='spritetrail', stationary=True),
        dict(name='compat-rotation-zero', x=64, y=192, texture='angle', rotation='0 0 0'),
        dict(name='compat-rotation-quarter', x=192, y=192, texture='angle', rotation='0 0 1.5707963267948966'),
        dict(name='compat-box-flat', x=320, y=192, size=4, count=80,
             emitter=dict(name='boxrandom', distancemin='0 0 0', distancemax='56 56 0', directions='1 0 0')),
        dict(name='compat-box-quarter', x=448, y=192, size=4, count=80,
             emitter=dict(name='boxrandom', distancemin='0 0 0', distancemax='56 56 0', directions='1 0.25 0')),
        dict(name='compat-sphere-sign', x=64, y=64, size=4, count=80,
             emitter=dict(name='sphererandom', distancemin=0, distancemax=50, sign='-1 1 0')),
        dict(name='channels-bright', x=64, y=192, texture='rg-bright'),
        dict(name='channels-dark', x=192, y=192, texture='rg-dark'),
        dict(name='channels-clear', x=320, y=192, texture='rg-clear'),
        dict(name='alpha-change-full', x=64, y=192, operator=dict(name='alphachange', startvalue=.25, endvalue=.25)),
        dict(name='alpha-change-half', x=192, y=192, alpha=.5, operator=dict(name='alphachange', startvalue=.25, endvalue=.25)),
        dict(name='alpha-change-fade', x=320, y=192, alpha=.5,
             before_operator=dict(name='alphafade', fadeintime=1, fadeouttime=0),
             operator=dict(name='alphachange', startvalue=.25, endvalue=.25)),
        dict(name='velocity-turb-default', x=85, y=32, size=2, life=.1, count=200, rate=1000, start=.1, velocity='0 0 0', initializer=dict(name='turbulentvelocityrandom', scale=0, offset=0)),
        dict(name='velocity-turb-fixed', x=255, y=32, size=2, life=2, count=200, rate=100, start=2, velocity='0 0 0', initializer=dict(name='turbulentvelocityrandom', scale=0, offset=0, speedmin=32, speedmax=32)),
        dict(name='velocity-turb-offset', x=425, y=32, size=2, life=2, count=200, rate=100, start=2, velocity='0 0 0', initializer=dict(name='turbulentvelocityrandom', scale=0, offset=-.5, speedmin=32, speedmax=32)),
        dict(name='velocity-defaults-implicit', x=1024, y=512, size=32, count=1000, start=1, velocity='0 0 0', initializer=dict(name='turbulentvelocityrandom', scale=0, offset=0)),
        dict(name='velocity-defaults-explicit', x=3072, y=512, size=32, count=1000, start=1, velocity='0 0 0', initializer=dict(name='turbulentvelocityrandom', scale=0, offset=0, speedmin=100, speedmax=300)),
        dict(name='velocity-speed-implicit', x=128, y=128, size=8, count=1000, velocity='0 0 0', stationary=True, renderer='spritetrail', trail=dict(length=.1, maxlength=100), initializer=dict(name='turbulentvelocityrandom', scale=0, offset=0)),
        dict(name='velocity-speed-explicit', x=384, y=128, size=8, count=1000, velocity='0 0 0', stationary=True, renderer='spritetrail', trail=dict(length=.1, maxlength=100), initializer=dict(name='turbulentvelocityrandom', scale=0, offset=0, speedmin=100, speedmax=300)),
        dict(name='turb-mask-zero', x=256, y=224, size=8, velocity='0 0 0',
             turbulence_op=dict(mask='0 0 0', speedmin=40, speedmax=40, phasemin=1, phasemax=1, scale=.002)),
        dict(name='turb-no-movement', x=256, y=192, size=8,
             turbulence_op=dict(mask='1 0 0', speedmin=40, speedmax=40, phasemin=1, phasemax=1, scale=.002)),
        dict(name='turb-phase-zero', x=256, y=160, size=8, velocity='0 0 0',
             turbulence_op=dict(mask='1 0 0', speedmin=40, speedmax=40, phasemin=0, phasemax=0, scale=.002)),
        dict(name='turb-phase-one', x=256, y=128, size=8, velocity='0 0 0',
             turbulence_op=dict(mask='1 0 0', speedmin=40, speedmax=40, phasemin=1, phasemax=1, scale=.002)),
        dict(name='turb-phase-two', x=256, y=96, size=8, velocity='0 0 0',
             turbulence_op=dict(mask='1 0 0', speedmin=40, speedmax=40, phasemin=2, phasemax=2, scale=.002)),
        dict(name='turb-speed-zero', x=256, y=64, size=8, velocity='0 0 0',
             turbulence_op=dict(mask='1 0 0', speedmin=0, speedmax=0, phasemin=1, phasemax=1, scale=.002)),
        dict(name='position-zero', x=64, y=192, size=8, operator=dict(name='oscillateposition', mask='1 0.5 0', frequencymin=0, frequencymax=0, scalemin=32, scalemax=32, phasemin=0, phasemax=0)),
        dict(name='position-quarter', x=192, y=192, size=8, operator=dict(name='oscillateposition', mask='1 0.5 0', frequencymin=0, frequencymax=0, scalemin=32, scalemax=32, phasemin=1.5707963267948966, phasemax=1.5707963267948966)),
        dict(name='position-three-quarter', x=320, y=192, size=8, operator=dict(name='oscillateposition', mask='1 0.5 0', frequencymin=0, frequencymax=0, scalemin=32, scalemax=32, phasemin=4.71238898038469, phasemax=4.71238898038469)),
        dict(name='position-cycle', x=448, y=64, size=8, life=8, operator=dict(name='oscillateposition', mask='1 0.5 0', frequencymin=1, frequencymax=1, scalemin=32, scalemax=32, phasemin=0, phasemax=0)),
        dict(name='position-live-zero', x=128, y=128, size=2, life=8, count=100, rate=10, start=8, velocity='0 0 0', operator=dict(name='oscillateposition', mask='1 0.5 0', frequencymin=1, frequencymax=1, scalemin=32, scalemax=32, phasemin=0, phasemax=0)),
        dict(name='position-live-quarter', x=384, y=128, size=2, life=8, count=100, rate=10, start=8, velocity='0 0 0', operator=dict(name='oscillateposition', mask='1 0.5 0', frequencymin=1, frequencymax=1, scalemin=32, scalemax=32, phasemin=1.5707963267948966, phasemax=1.5707963267948966)),
    ]
    for index, mask in enumerate(['1 0 0', '0.5 0 0', '0 1 0', '0 0.5 0', '1 1 0', '1 0.5 0']):
        cases.append(dict(name=f'position-mask-{index}', x=85 + index % 3 * 170,
                          y=192 - index // 3 * 128, size=2, life=8, count=100,
                          rate=10, start=8, velocity='0 0 0',
                          operator=dict(name='oscillateposition', mask=mask,
                                        frequencymin=1, frequencymax=1, scalemin=16,
                                        scalemax=16, phasemin=0, phasemax=0)))
    for index, start in enumerate([15, 30, 45, 60, 75, 90]):
        cases.append(dict(name=f'position-warm-{start}', x=85 + index % 3 * 170,
                          y=192 - index // 3 * 128, size=4, life=120, start=start,
                          velocity='0 0 0', operator=dict(name='oscillateposition',
                          mask='1 0.5 0', frequencymin=1, frequencymax=1,
                          scalemin=16, scalemax=16, phasemin=0, phasemax=0)))
    if args.case:
        cases = [case for case in cases if case['name'] == args.case
                 or args.case == 'compatibility' and case['name'].startswith('compat-')
                 or args.case == 'channels' and case['name'].startswith('channels-')
                 or args.case == 'turbulence' and case['name'].startswith('turb-')
                 or args.case == 'position' and case['name'].startswith('position-') and not case['name'].startswith('position-live-')
                 or args.case == 'position-live' and case['name'].startswith('position-live-')
                 or args.case == 'position-mask' and case['name'].startswith('position-mask-')
                 or args.case == 'alpha-change' and case['name'].startswith('alpha-change-')
                 or args.case == 'position-warm' and case['name'].startswith('position-warm-')
                 or args.case == 'velocity-turbulence' and case['name'].startswith('velocity-turb-')
                 or args.case == 'velocity-defaults' and case['name'].startswith('velocity-defaults-')
                 or args.case == 'velocity-speed' and case['name'].startswith('velocity-speed-')]
        if not cases:
            parser.error('unknown probe case')
    for index, case in enumerate(cases, 2):
        name = case['name']
        if case.get('texture') == 'particle/halo':
            target = root / 'materials/particle/halo.tex'
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(args.engine / 'materials/particle/halo.tex', target)
        write_json(root, f'materials/{name}.json', dict(passes=[dict(state, shader='genericparticle', blending='additive', textures=[case.get('texture', 'white')], constantshadervalues=dict(ui_editor_properties_overbright=case.get('brightness', 1)))]))
        life = case.get('life', 120)
        size = case.get('size', 64)
        init = [dict(name='lifetimerandom', min=life, max=life), dict(name='sizerandom', min=size, max=size), dict(name='alpharandom', min=case.get('alpha', 1), max=case.get('alpha', 1))]
        ops = []
        if 'color' in case:
            init.append(dict(name='colorrandom', min=case['color'], max=case['color']))
        if 'velocity' in case:
            init.append(dict(name='velocityrandom', min=case['velocity'], max=case['velocity']))
            if not case.get('stationary'):
                ops.append(dict(name='movement', gravity='0 0 0'))
                if 'drag' in case:
                    ops[0]['drag'] = case['drag']
        if case.get('turbulence'):
            init.append(dict(name='turbulentvelocityrandom', speedmin=0, speedmax=0, scale=.5, offset=3))
        if 'initializer' in case:
            init.append(case['initializer'].copy())
        if 'turbulence_op' in case:
            ops.append(dict(name='turbulence', **case['turbulence_op']))
        if 'operator' in case:
            if 'before_operator' in case:
                ops.append(case['before_operator'].copy())
            ops.append(case['operator'].copy())
        if 'angular' in case:
            init.append(dict(name='angularvelocityrandom', min='0 0 1', max='0 0 1'))
            ops.append(dict(name='angularmovement', force='0 0 0'))
        if 'rotation' in case:
            init.append(dict(name='rotationrandom', min=case['rotation'], max=case['rotation']))
        if case.get('envelope'):
            ops = [dict(name='sizechange', starttime=.5), dict(name='sizechange', endtime=.5, startvalue=0, endvalue=1)]
        renderer = dict(name=case.get('renderer', 'sprite'))
        renderer.update(case.get('trail', {}))
        emitter = dict(name='sphererandom', instantaneous=1, rate=0, origin='0 0 0', distancemin=0, distancemax=0, speedmin=0, speedmax=0, directions='1 1 0')
        emitter.update(case.get('emitter', {}))
        if emitter['name'] == 'boxrandom':
            emitter.pop('speedmin')
            emitter.pop('speedmax')
        emitter['instantaneous'] = case.get('count', 1)
        if 'rate' in case:
            emitter['instantaneous'] = 0
            emitter['rate'] = case['rate']
        for component_id, component in enumerate([renderer, *init, emitter, *ops], 1):
            component['id'] = component_id
        write_json(root, f'particles/{name}.json', dict(flags=0, maxcount=max(4,case.get('count',1)), starttime=case.get('start', 0), animationmode=0, sequencemultiplier=case.get('multiplier', 1), material=f'materials/{name}.json', controlpoint=[dict(id=i, flags=0, offset='0 0 0') for i in range(8)], emitter=[emitter], initializer=init, operator=ops, renderer=[renderer]))
        objects.append(dict(id=index, particle=f'particles/{name}.json', origin=f"{case['x']} {case['y']} 0"))
    canvas = [4096, 4096] if args.case == 'velocity-defaults' else [512, 256]
    objects[0]['origin'] = f'{canvas[0] / 2} {canvas[1] / 2} 0'
    objects[0]['size'] = f'{canvas[0]} {canvas[1]}'
    write_json(root, 'scene.json', dict(version=5, camera=dict(center='0 0 -1', eye='0 0 0', up='0 1 0'), general=dict(orthogonalprojection=dict(width=canvas[0], height=canvas[1]), clearcolor='0 0 0', clearenabled=True, hdr=True, bloom=False, cameraparallax=False, fov=50, nearz=.01, farz=10000, zoom=1), objects=objects))
    write_json(root, 'project.json', dict(type='scene', file='scene.json', title='Owned particle motion probes', general=dict(properties={})))
    files = [(p.relative_to(root).as_posix(), p.read_bytes()) for p in sorted(root.rglob('*')) if p.is_file() and p.name != 'project.json']
    data = struct.pack('<I', 8) + b'PKGV0016' + struct.pack('<I', len(files))
    offset = 0
    for name, payload in files:
        key = name.encode()
        data += struct.pack('<I', len(key)) + key + struct.pack('<II', offset, len(payload))
        offset += len(payload)
    (root / 'library.pkg').write_bytes(data + b''.join(payload for _, payload in files))
    shutil.copytree(args.engine / 'shaders', root / 'shaders', dirs_exist_ok=True)
    write_json(root, 'probes.json', dict(cases=cases, limits='Generated data only; reference timing is measured from captures, not engine simulation time.'))
    print(root.resolve())


if __name__ == '__main__':
    main()
