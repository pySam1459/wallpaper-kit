"""Freeze a completed benchmark matrix into a compact baseline JSON and Markdown table."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics


def spread(values):
    return dict(median=statistics.median(values), minimum=min(values), maximum=max(values)) if values else None


def summarize(report):
    if report['status']!='passed' or not report['runs'] or any(r['status']!='passed' for r in report['runs']):
        raise ValueError('Only a completed, passing matrix can become a baseline')
    groups={}
    for run in report['runs']:
        groups.setdefault((run['name'],run['resolution']),[]).append(run)
    results=[]
    for (name,resolution),runs in groups.items():
        samples={}
        packages=set()
        omissions=set()
        for run in runs:
            metrics=run['metrics']
            playback=metrics['playback']
            packages.add(playback['package_sha256'] if playback else 'blank')
            omissions.add(len(playback['omissions']) if playback else 0)
            for phase in ['disabled','paused','paced','uncapped','paused_after','released']:
                timing=next(p for p in metrics['phases'] if p['name']==phase)
                t=metrics['telemetry'][phase]
                if not t['available']:continue
                values={phase+'_cpu_percent':t['cpu_machine_percent_mean'],phase+'_cpu_cores':t['cpu_cores_mean'],
                    phase+'_private_mib':t['private_bytes']['mean']/2**20,phase+'_rss_mib':t['working_set_bytes']['mean']/2**20,
                    phase+'_private_growth_mib':t['private_growth_bytes']/2**20}
                for field,key in [('gpu_percent','gpu_busiest_engine_percent'),('gpu_dedicated_mib','gpu_dedicated_bytes'),('gpu_shared_mib','gpu_shared_bytes')]:
                    if t[key]:values[phase+'_'+field]=t[key]['mean']/(2**20 if field.endswith('mib') else 1)
                if 'completed_fps' in timing:
                    values.update({phase+'_fps':timing['completed_fps'],phase+'_p95_gap_ms':timing['ready_gap_ms']['p95'],
                        phase+'_p99_gap_ms':timing['ready_gap_ms']['p99'],phase+'_max_gap_ms':timing['ready_gap_ms']['max'],
                        phase+'_deadline_misses':timing['deadline_misses'],phase+'_skipped_slots':timing['skipped_slots'],
                        phase+'_source_age_p95_ms':timing['source_age_ms']['p95'],
                        phase+'_changed_source_selections_per_second':timing['changed_source_index_selections']/timing['elapsed_seconds'],
                        phase+'_texture_uploads':timing['pool_after']['texture_uploads']-timing['pool_before']['texture_uploads'],
                        phase+'_texture_allocations':timing['pool_after']['texture_allocations']-timing['pool_before']['texture_allocations'],
                        phase+'_texture_payload_mib':sum(timing['pool_after'][k] for k in ['resident_bytes','source_texture_bytes','effect_texture_bytes'])/2**20})
                for key,value in values.items():samples.setdefault(key,[]).append(value)
            for key in ['first_content_ms','asset_open_ms','device_start_ms','shutdown_ms']:
                samples.setdefault(key,[]).append(metrics[key])
            samples.setdefault('peak_job_private_mib',[]).append(metrics['process_tree']['final_accounting']['peak_job_private_bytes']/2**20)
        if len(packages)!=1:raise ValueError(f'Source changed across repetitions: {name}')
        results.append(dict(name=name,resolution=resolution,repetitions=len(runs),source_sha256=next(iter(packages)),
            omission_counts=sorted(omissions),adapter=runs[0]['metrics']['frame']['adapter'],
            retry_history=[dict(repeat=run['repeat'],history=run['retry_history']) for run in runs if run.get('retry_history')],
            metrics={key:spread(values) for key,values in samples.items()}))
    return dict(schema_version=1,status='passed',revision=report['revision'],build_profile=report['build_profile'],
        executable_sha256=report['executable_sha256'],source_hashes=report['sources'],
        retry_controller_sources=report.get('retry_controller_sources'),machine=report['machine'],
        logical_cpus=report['logical_cpus'],configuration=report['configuration'],results=results,
        limits=['Medians and ranges across repetitions; frame p95/p99 fields summarize each repetition, not a pooled percentile.',
            'Completed native one-request throughput, not displayed FPS, distinct video frames, or pure GPU duration.',
            'CPU includes job children; GPU percent is the busiest owned engine. Memory includes benchmark and driver overhead.',
            'Short-duration measurements, existing scene omissions and ordinary desktop contention; no host latency or leak-free claim.'])


def markdown(baseline):
    lines=['# Initial performance baseline','',
        'Release hardware runs. Values are medians across repetitions; JSON also retains the minimum and maximum.',
        'CPU % uses total logical CPU capacity. Private MiB is process-tree committed memory during paced playback. GPU % is the busiest owned engine.',
        '', '| Workload | Output | Paced fps | Uncapped fps | CPU % | Private MiB | GPU % | p95 gap ms |',
        '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |']
    for r in baseline['results']:
        m=r['metrics']
        def value(key):return f"{m[key]['median']:.2f}" if key in m else 'unavailable'
        lines.append(f"| {r['name']} | {r['resolution']} | {value('paced_fps')} | {value('uncapped_fps')} | {value('paced_cpu_percent')} | {value('paced_private_mib')} | {value('paced_gpu_percent')} | {value('paced_p95_gap_ms')} |")
    return '\n'.join(lines)+'\n'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report',type=Path)
    parser.add_argument('output',type=Path)
    args=parser.parse_args()
    raw=args.report.read_bytes()
    baseline=summarize(json.loads(raw))
    baseline['raw_report_sha256']=hashlib.sha256(raw).hexdigest()
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x',encoding='utf-8') as stream:stream.write(json.dumps(baseline,indent=2)+'\n')
    with args.output.with_suffix('.md').open('x',encoding='utf-8') as stream:stream.write(markdown(baseline))
    print(args.output)


if __name__=='__main__':main()
