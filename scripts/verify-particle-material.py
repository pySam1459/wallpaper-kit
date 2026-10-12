"""Check generated particle material probes against independent code-value pixels."""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--case', choices=['channels', 'alpha-change'], default='channels')
    parser.add_argument('--client-x', type=int, default=0)
    parser.add_argument('--client-y', type=int, default=0)
    args = parser.parse_args()
    image = Image.open(args.capture).convert('RGB')
    checks = []
    cases = [('bright', 128, 144), ('dark', 384, 80), ('clear', 640, 16)] if args.case == 'channels' else [('full', 128, 80), ('half', 384, 48)]
    for name, x, expected in cases:
        pixel = image.getpixel((args.client_x + x, args.client_y + 128))
        checks.append(dict(case=name, expected=[expected] * 3, actual=pixel,
                           passed=all(abs(c - expected) <= 3 for c in pixel)))
    result = dict(passed=all(c['passed'] for c in checks), checks=checks,
                  sha256=hashlib.sha256(args.capture.read_bytes()).hexdigest(),
                  limits='Static code-value probe; does not establish motion or full-scene fidelity.')
    args.output.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
