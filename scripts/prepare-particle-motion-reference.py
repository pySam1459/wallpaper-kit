"""Generate local particle probes for observable engine/library comparisons."""
import argparse
import json
import shutil
import struct
from pathlib import Path


def texture(width, height, pixels, frames=()):
    data = b'TEXV0005\0TEXI0001\0' + struct.pack('<7I', 0, 4 if frames else 0, width, height, width, height, 0)
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
    ]
    if args.case:
        cases = [case for case in cases if case['name'] == args.case]
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
        if 'angular' in case:
            init.append(dict(name='angularvelocityrandom', min='0 0 1', max='0 0 1'))
            ops.append(dict(name='angularmovement', force='0 0 0'))
        if case.get('envelope'):
            ops = [dict(name='sizechange', starttime=.5), dict(name='sizechange', endtime=.5, startvalue=0, endvalue=1)]
        renderer = dict(name=case.get('renderer', 'sprite'))
        renderer.update(case.get('trail', {}))
        emitter = dict(name='sphererandom', instantaneous=1, rate=0, origin='0 0 0', distancemin=0, distancemax=0, speedmin=0, speedmax=0, directions='1 1 0')
        for component_id, component in enumerate([renderer, *init, emitter, *ops], 1):
            component['id'] = component_id
        write_json(root, f'particles/{name}.json', dict(flags=0, maxcount=4, starttime=0, animationmode=0, sequencemultiplier=case.get('multiplier', 1), material=f'materials/{name}.json', controlpoint=[dict(id=i, flags=0, offset='0 0 0') for i in range(8)], emitter=[emitter], initializer=init, operator=ops, renderer=[renderer]))
        objects.append(dict(id=index, particle=f'particles/{name}.json', origin=f"{case['x']} {case['y']} 0"))
    write_json(root, 'scene.json', dict(version=5, camera=dict(center='0 0 -1', eye='0 0 0', up='0 1 0'), general=dict(orthogonalprojection=dict(width=512, height=256), clearcolor='0 0 0', clearenabled=True, hdr=True, bloom=False, cameraparallax=False, fov=50, nearz=.01, farz=10000, zoom=1), objects=objects))
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
