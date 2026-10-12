"""Check generated Shake relationships and live timing in retained desktop JPEGs (Pillow required)."""
import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

from PIL import Image


def verify(directory, timeline, client):
    observations = json.loads(timeline.read_text())
    measurements, identities = [], []
    for observation in observations:
        path = Path(observation["path"])
        with Image.open(path) as image:
            displacement = []
            for index in range(12):
                origin = (client[0] + index % 3 * 256, client[1] + index // 3 * 256)
                displacement.append(statistics.median(image.getpixel((origin[0] + x, origin[1] + 85))[0] - x
                                                      for x in range(80, 111)))
            # The neutral-flow tile removes JPEG/color conversion bias.
            displacement = [value - displacement[8] for value in displacement]
            def patch(tile, x):
                # Average a small patch so JPEG chroma subsampling at grid
                # edges does not masquerade as a different opacity operation.
                return [statistics.mean(image.getpixel((client[0]+tile*256+xx, client[1]+768+yy))[c]
                                        for xx in range(x-3,x+4) for yy in range(82,89)) for c in range(3)]
            for x in (24, 32, 56, 64, 92):
                unchanged, half, full = (patch(tile,x) for tile in range(3))
                for a, b, c in zip(unchanged, half, full):
                    if abs(b - (a * 127 + c * 128) / 255) > 10:
                        raise ValueError("Opacity must blend the original and displaced colors")
        measurements.append(displacement)
        identities.append({**observation, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    times = [(v["start_ms"]+v["end_ms"])/2000 for v in observations]
    times = [t-times[0] for t in times]
    if len(times) < 6 or times[-1] < 2:
        raise ValueError("Need six or more live captures spanning at least two seconds")
    # Fit only the unknown start phase. Requested speed and gain are fixed.
    phases = [i * math.tau / 6284 for i in range(6284)]
    phase = min(phases, key=lambda p: sum((row[0]-64*math.sin(t+p))**2 for t, row in zip(times,measurements)))
    errors = []
    for time, row in zip(times, measurements):
        wave = 64 * math.sin(time+phase)
        expected = [wave, wave/4, wave*.04, wave*.04, wave, wave,
                    64*math.sin(time+phase+64/255*math.tau),
                    64*math.sin(time+phase+128/255*math.tau), 0, 0, wave*128/255, wave]
        errors.extend(abs(a-b) for a,b in zip(row,expected))
    if max(errors) > 4:
        raise ValueError(f"Shake relationship/timing error {max(errors):.3f} exceeds four source texels")
    inputs = directory / "probes.json"
    return {"passed":True, "speed_radians_per_second":1, "fitted_start_phase":phase,
            "max_source_texel_error":max(errors), "client_origin":client,
            "inputs_sha256":hashlib.sha256(inputs.read_bytes()).hexdigest(),
            "source_sha256":{str(p.relative_to(directory)):hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in sorted((directory/"relationships").rglob("*")) if p.is_file()},
            "vendor_inputs":json.loads(inputs.read_text())["vendor_inputs"],
            "captures":identities, "measurements":measurements,
            "limits":["Desktop JPEG and capture latency limit precision", "Default periodic profile only",
                      "No proof of displayed frame cadence or whole-scene fidelity"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("timeline", type=Path)
    parser.add_argument("--client-x", type=int, required=True)
    parser.add_argument("--client-y", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = verify(args.directory, args.timeline, (args.client_x,args.client_y))
    args.output.write_text(json.dumps(report,indent=2))
    print(f"Shake reference passed; maximum error {report['max_source_texel_error']:.3f} texels")
