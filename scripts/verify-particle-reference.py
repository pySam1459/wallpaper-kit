"""Check isolated particle rotation and emitter bounds in an engine or native capture."""
import argparse
import hashlib
import json
from pathlib import Path
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--client-x', type=int, default=0)
    parser.add_argument('--client-y', type=int, default=0)
    args = parser.parse_args()
    image = Image.open(args.capture).convert('RGB')
    if image.width < args.client_x + 1024 or image.height < args.client_y + 512:
        raise ValueError('Expected a 1024 x 512 client image')

    def red(x, y):
        return image.getpixel((x + args.client_x, y + args.client_y))[0]

    for lit, dark in [([(104, 128), (128, 104)], (152, 128)),
                      ([(408, 128), (384, 104)], (360, 128))]:
        if any(red(x, y) < 200 for x, y in lit) or red(*dark) > 40:
            raise ValueError('Asymmetric sprites do not show zero and clockwise quarter-turn rotation')

    def bounds(rect):
        x0, y0, x1, y1 = rect
        points = [(x, y) for y in range(y0, y1) for x in range(x0, x1) if red(x, y) > 200]
        if len(points) < 100:
            raise ValueError(f'Too few emitter pixels in {rect}')
        return [min(x for x, y in points), min(y for x, y in points),
                max(x for x, y in points), max(y for x, y in points)]

    flat = bounds((520, 64, 760, 192))
    quarter = bounds((776, 64, 1016, 192))
    sign = bounds((16, 272, 144, 400))
    # Allow three output pixels for sprite extent and JPEG edge contamination.
    if flat[2] - flat[0] < 180 or not (125 <= flat[1] <= flat[3] <= 131):
        raise ValueError(f'Box zero-height direction failed: {flat}')
    if quarter[2] - quarter[0] < 180 or quarter[3] - quarter[1] < 32 or not (97 <= quarter[1] <= quarter[3] <= 159):
        raise ValueError(f'Box quarter-height direction failed: {quarter}')
    if not (25 <= sign[0] <= sign[2] <= 131 and 281 <= sign[1] <= sign[3] <= 387):
        raise ValueError(f'Sphere sign quadrant failed: {sign}')
    result = dict(passed=True, capture=str(args.capture.resolve()),
                  sha256=hashlib.sha256(args.capture.read_bytes()).hexdigest(),
                  client_origin=[args.client_x, args.client_y],
                  emitter_bounds=dict(flat=flat, quarter=quarter, sign=sign),
                  limits='Static probes check geometry and rotation, not random-sequence agreement or displayed cadence.')
    args.output.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
