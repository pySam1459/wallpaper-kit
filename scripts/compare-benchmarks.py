"""Compare immutable compact benchmark snapshots and retain notes and provenance."""
import argparse
import hashlib
import json
from pathlib import Path


def compare(before, after, note):
    if any(d.get('status') != 'passed' or not d.get('results') for d in [before, after]):
        raise ValueError('Both snapshots must contain completed, passing results')
    previous = {(r['name'], r['resolution']): r for r in before['results']}
    rows = []
    for current in after['results']:
        key = current['name'], current['resolution']
        if key not in previous:
            raise ValueError(f'No matching baseline workload: {key}')
        old = previous[key]
        if old['source_sha256'] != current['source_sha256']:
            raise ValueError(f'Source changed: {key}')
        metrics = {}
        for name in sorted(old['metrics'].keys() | current['metrics'].keys()):
            a, b = old['metrics'].get(name), current['metrics'].get(name)
            delta = b['median'] - a['median'] if a and b else None
            percent = delta / a['median'] * 100 if a and b and a['median'] else None
            metrics[name] = dict(before=a, after=b, delta=delta, percent_change=percent)
        rows.append(dict(name=key[0], resolution=key[1], source_sha256=current['source_sha256'],
                         omission_counts_before=old['omission_counts'], omission_counts_after=current['omission_counts'], metrics=metrics))
    defaults = dict(profile_gpu=False, trim_paused=False, visual_effects=True)
    configs = [{**defaults, **d['configuration']} for d in [before, after]]
    changes = {key: dict(before=configs[0].get(key), after=configs[1].get(key))
               for key in sorted(configs[0].keys() | configs[1].keys()) if configs[0].get(key) != configs[1].get(key)}
    return dict(schema_version=1, note=note, configuration_changes=changes,
                machine_before=before['machine'], machine_after=after['machine'], results=rows,
                limits=['Changes are after minus before; negative CPU/memory/GPU deltas are reductions.',
                        'Zero baselines have no percentage change. Missing values remain unavailable.',
                        'Configuration changes are explicit. Feature-disabled comparisons measure different rendered content.',
                        'Medians/ranges are per-repetition observations, not statistical confidence intervals.'])


def markdown(report):
    lines = ['# Performance comparison', '', report['note'], '',
             'Configuration changes: ' + json.dumps(report['configuration_changes'], sort_keys=True), '',
             '| Workload | Output | CPU % | Private MiB | GPU % | Uncapped fps |',
             '| --- | --- | ---: | ---: | ---: | ---: |']
    for row in report['results']:
        def cell(key):
            m = row['metrics'].get(key, {})
            a, b = m.get('before'), m.get('after')
            return f"{a['median']:.2f} → {b['median']:.2f}" if a and b else 'unavailable'
        lines.append(f"| {row['name']} | {row['resolution']} | {cell('paced_cpu_percent')} | {cell('paced_private_mib')} | {cell('paced_gpu_percent')} | {cell('uncapped_fps')} |")
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--note', required=True)
    args = parser.parse_args()
    report = compare(json.loads(args.before.read_text()), json.loads(args.after.read_text()), args.note)
    for name in ['before', 'after']:
        p = getattr(args, name)
        d = json.loads(p.read_text())
        report[name] = dict(path=p.as_posix(), sha256=hashlib.sha256(p.read_bytes()).hexdigest(),
                            revision=d['revision'], executable_sha256=d['executable_sha256'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists() or args.output.with_suffix('.md').exists():
        raise ValueError('Comparison output already exists; keep previous results immutable')
    with args.output.open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(report, indent=2) + '\n')
    with args.output.with_suffix('.md').open('x', encoding='utf-8') as stream:
        stream.write(markdown(report))
    print(args.output)


if __name__ == '__main__':
    main()
