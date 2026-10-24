"""Check stationary turbulent-velocity trail bounds independently of engine time."""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageChops


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--case', choices=['minimum', 'maximum'], required=True)
    parser.add_argument('--client-x', type=int, default=0)
    parser.add_argument('--client-y', type=int, default=0)
    args = parser.parse_args()
    capture = Image.open(args.capture).convert('RGB')
    client = capture.crop((args.client_x, args.client_y,
                           args.client_x + 1024, args.client_y + 512))
    red, green, blue = client.split()
    mask = ImageChops.darker(ImageChops.darker(red, green), blue).point(lambda c: 255 if c > 220 else 0)
    # Size 8 spans 8 pixels at this output scale. Trail stretch is speed * 0.1.
    speeds = [100, 100] if args.case == 'minimum' else [250, 300]
    checks = []
    for center, speed in zip([256, 768], speeds):
        local = mask.crop((center - 16, 0, center + 16, 512)).getbbox()
        actual = [local[0] + center - 16, local[1], local[2] + center - 16, local[3]] if local else None
        half_height = round(8 * speed * .1 / 2)
        expected = [center - 4, 256 - half_height, center + 4, 256 + half_height]
        checks.append(dict(expected=expected, actual=actual,
                           passed=actual is not None and all(abs(a-b) <= 2 for a, b in zip(actual, expected))))
    report = dict(passed=all(c['passed'] for c in checks), checks=checks,
                  capture_sha256=hashlib.sha256(args.capture.read_bytes()).hexdigest(),
                  limits='Stationary maximum trail extents verify speed defaults, not noise trajectories or live cadence.')
    args.output.write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
