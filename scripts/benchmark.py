"""Run release renderer workloads with Windows job/PDH accounting and retained raw data."""
import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import threading
import time


ROOT = Path(__file__).resolve().parents[1]


def distribution(values):
    if not values:
        return None
    ordered = sorted(values)
    return dict(mean=statistics.fmean(values), p50=ordered[math.ceil(len(values)*.5)-1],
                p95=ordered[math.ceil(len(values)*.95)-1], p99=ordered[math.ceil(len(values)*.99)-1], max=ordered[-1])


def summarize_phase(phase, rows, logical_cpus):
    selected = [r for r in rows if phase['start_unix_ms'] <= r['unix_ms'] <= phase['end_unix_ms']]
    if len(selected) < 2:
        return dict(available=False, samples=len(selected), reason='Fewer than two samples wholly inside the phase')
    elapsed = selected[-1]['monotonic_s']-selected[0]['monotonic_s']
    cpu = selected[-1]['process']['cpu_seconds']-selected[0]['process']['cpu_seconds']
    intervals = []
    system = []
    gpu = []
    for a,b in zip(selected, selected[1:]):
        dt = b['monotonic_s']-a['monotonic_s']
        intervals.append((b['process']['cpu_seconds']-a['process']['cpu_seconds'])/dt)
        total = b['process']['system_cpu_total']-a['process']['system_cpu_total']
        idle = b['process']['system_cpu_idle']-a['process']['system_cpu_idle']
        if total > 0:
            system.append((1-idle/total)*100)
        if b['gpu']['available']:
            gpu.append(b['gpu']['busiest_engine_percent'])
    first, last = selected[0]['process'], selected[-1]['process']
    return dict(available=True, samples=len(selected), covered_seconds=elapsed,
        coverage_fraction=elapsed/((phase['end_unix_ms']-phase['start_unix_ms'])/1000),
        cpu_cores_mean=cpu/elapsed, cpu_machine_percent_mean=cpu/elapsed/logical_cpus*100,
        cpu_cores=distribution(intervals), system_cpu_percent=distribution(system),
        working_set_bytes=distribution([r['process']['working_set_bytes'] for r in selected]),
        private_bytes=distribution([r['process']['private_bytes'] for r in selected]),
        private_growth_bytes=last['private_bytes']-first['private_bytes'],
        private_growth_bytes_per_minute=(last['private_bytes']-first['private_bytes'])/elapsed*60,
        handles=distribution([r['process']['handles'] for r in selected]),
        active_processes_max=max(r['process']['active_processes'] for r in selected),
        gpu_busiest_engine_percent=distribution(gpu), gpu_valid_intervals=len(gpu),
        gpu_dedicated_bytes=distribution([r['gpu']['dedicated_bytes'] for r in selected if r['gpu']['available']]),
        gpu_shared_bytes=distribution([r['gpu']['shared_bytes'] for r in selected if r['gpu']['available']]),
        io_read_bytes=last['io_read_bytes']-first['io_read_bytes'],io_write_bytes=last['io_write_bytes']-first['io_write_bytes'])


def command(args):
    return subprocess.check_output(args, cwd=ROOT, text=True, encoding='utf-8', errors='replace').strip()


def source_hashes():
    files = [ROOT/'Cargo.toml', ROOT/'Cargo.lock', ROOT/'benchmarks/workloads.json']
    for name in ['src', 'examples', 'scripts']:
        files += [p for p in (ROOT/name).rglob('*') if p.is_file() and p.suffix in ['.rs','.hlsl','.py','.ps1']]
    return {str(p.relative_to(ROOT)).replace('\\','/'):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)}


def start_nvidia(output):
    executable = shutil.which('nvidia-smi')
    if not executable:
        return None, [], None
    fields = ['timestamp','index','utilization.gpu','utilization.memory','memory.used','temperature.gpu',
              'power.draw','clocks.current.graphics','pstate']
    rows = []
    process = subprocess.Popen([executable,'--query-gpu='+','.join(fields),'--format=csv,noheader,nounits','--loop-ms=1000'],
        stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NO_WINDOW)
    def read():
        for line in process.stdout:
            rows.append(dict(unix_ms=time.time_ns()/1e6, values=dict(zip(fields,next(csv.reader([line]))))))
    thread=threading.Thread(target=read,daemon=True);thread.start()
    return process, rows, thread


def run_case(executable, folder, config, sample_seconds):
    from benchmark_windows import Job, GpuCounters
    folder.mkdir()
    config_path=folder/'config.json'
    config_path.write_text(json.dumps(config,indent=2),encoding='utf-8')
    gpu=GpuCounters()
    samples, lines = [], []
    process=None
    job=None
    nvidia, nvidia_rows, nvidia_thread = start_nvidia(folder)
    monitor_cpu=time.process_time()
    try:
        process=subprocess.Popen([str(executable),str(config_path),str(folder/'worker')],stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NO_WINDOW)
        job=Job(process.pid)
        def read():
            for line in process.stdout:
                lines.append(json.loads(line))
        reader=threading.Thread(target=read,daemon=True);reader.start()
        process.stdin.write('start\n');process.stdin.flush();process.stdin.close()
        deadline=time.monotonic()+180+2*config['measure_seconds']+3*config['warmup_seconds']+4*config['idle_seconds']
        tick=time.monotonic()
        while process.poll() is None:
            if time.monotonic()>deadline:
                raise TimeoutError('benchmark process exceeded its bounded duration')
            started=time.perf_counter()
            resource=job.sample()
            samples.append(dict(unix_ms=time.time_ns()/1e6,monotonic_s=time.monotonic(),process=resource,
                gpu=gpu.sample(job.seen_pids),monitor_collection_ms=(time.perf_counter()-started)*1000))
            tick+=sample_seconds
            time.sleep(max(0,tick-time.monotonic()))
        reader.join(timeout=5)
        stderr=process.stderr.read()
        (folder/'stderr.txt').write_text(stderr,encoding='utf-8')
        if process.returncode:
            raise RuntimeError(f'benchmark exited {process.returncode}: {stderr}')
        report=json.loads((folder/'worker/report.json').read_text())
        if max(b['monotonic_s']-a['monotonic_s'] for a,b in zip(samples,samples[1:]))>2:
            raise RuntimeError('Resource sampling gap exceeded two seconds; exclude interrupted timing')
        report['telemetry']={phase['name']:summarize_phase(phase,samples,os.cpu_count()) for phase in report['phases']}
        report['process_tree']=dict(pid=process.pid,seen_pids=sorted(job.seen_pids),final_accounting=job.sample())
        report['monitor']=dict(cpu_seconds=time.process_time()-monitor_cpu,sample_seconds=sample_seconds,
            collection_ms=distribution([r['monitor_collection_ms'] for r in samples]))
        report['gpu_telemetry_errors']=sorted({r['gpu']['error'] for r in samples if not r['gpu']['available']})
        if not report['telemetry']['paced']['available'] or not report['telemetry']['uncapped']['available']:
            raise RuntimeError('resource sampling did not cover measured phases')
        for phase in ['paced','uncapped']:
            if report['telemetry'][phase]['gpu_valid_intervals']==0:
                report.setdefault('limitations',[]).append(f'{phase}: GPU utilization unavailable; see telemetry errors')
        (folder/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
        return report
    finally:
        gpu.close()
        if job:job.close()
        if process:
            if process.poll() is None:process.kill()
            process.wait(timeout=10)
        if nvidia:
            nvidia.terminate();nvidia.wait(timeout=10);nvidia_thread.join(timeout=5)
            (folder/'adapter-nvidia.json').write_text(json.dumps(nvidia_rows,indent=2),encoding='utf-8')
        (folder/'resources.json').write_text(json.dumps(samples,indent=2),encoding='utf-8')
        (folder/'events.json').write_text(json.dumps(lines,indent=2),encoding='utf-8')


def markdown(report):
    lines=['# Wallpaper-kit performance baseline','',f"Revision: `{report['revision']}`. Build: {report['build_profile']}; hardware rendering unless a run explicitly says software.",'',
        'CPU includes the renderer and decoder children. CPU % uses total logical CPU capacity. Memory is sampled process-tree private commit, not GPU memory. Uncapped FPS counts completed native frames, not displayed frames.','',
        '| Workload | Resolution | Run | 60 fps completed | Uncapped fps | Paced CPU % | Paced private MiB | Paced GPU % | p95 ready gap ms |',
        '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
    for run in report['runs']:
        if run['status']!='passed':
            lines.append(f"| {run['name']} | {run['resolution']} | {run['repeat']} | FAILED | | | | | |")
            continue
        data=run['metrics'];phases={p['name']:p for p in data['phases']}; t=data['telemetry']['paced']
        fmt=lambda value:'unavailable' if value is None else f'{value:.2f}'
        gpu=t['gpu_busiest_engine_percent']
        lines.append(f"| {run['name']} | {run['resolution']} | {run['repeat']} | {phases['paced']['completed_fps']:.2f} | {phases['uncapped']['completed_fps']:.2f} | {t['cpu_machine_percent_mean']:.2f} | {t['private_bytes']['mean']/2**20:.1f} | {fmt(gpu['mean'] if gpu else None)} | {phases['paced']['ready_gap_ms']['p95']:.2f} |")
    lines+=['','Each run retains raw per-frame CSV, process/job/PDH samples, source/adapter identity, initial pixels, phase boundaries and cleanup evidence.',
        'Paused, disabled and released controls, startup, deadline misses, source age, GPU memory and memory growth are in report.json.',
        'GPU % is the busiest owned GPU engine. NVIDIA telemetry describes the whole adapter and is kept separately. Neither measure is GPU command duration.',
        'Short samples do not establish absence of leaks, smooth display cadence, full scene fidelity, or consuming-app input latency.']
    return '\n'.join(lines)+'\n'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--executable',type=Path,default=ROOT/'target/release/examples/benchmark.exe')
    parser.add_argument('--workshop-root',type=Path,default=Path('C:/Program Files (x86)/Steam/steamapps/workshop/content/431960'))
    parser.add_argument('--engine-assets',type=Path,default=Path('C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets'))
    parser.add_argument('--workloads',default='all')
    parser.add_argument('--resolutions',default='1920x1080,3840x2160')
    parser.add_argument('--repeat',type=int,default=2)
    parser.add_argument('--seconds',type=int,default=8)
    parser.add_argument('--warmup',type=int,default=2)
    parser.add_argument('--idle',type=int,default=2)
    parser.add_argument('--fps',type=int,default=60)
    parser.add_argument('--software',action='store_true')
    parser.add_argument('--trim-paused',action='store_true',help='Freeze the presented sample and asynchronously release idle caches')
    parser.add_argument('--no-vfx',action='store_true',help='Feature-cost ablation: disable supported VFX/particles using consumer settings')
    parser.add_argument('--profile-gpu',action='store_true',help='Opt-in timestamp queries and worker timings; separate from production performance runs')
    parser.add_argument('--retry-failed',action='store_true',help='Append retries for failed cases in an existing output; preserve original reports')
    args=parser.parse_args()
    if not (1<=args.repeat<=10 and 1<=args.seconds<=120 and 1<=args.warmup<=60 and 1<=args.idle<=60 and 1<=args.fps<=240):
        parser.error('benchmark settings exceed supported bounds')
    output=args.output.resolve()
    artifacts=ROOT/'artifacts'
    if (output.exists() and not args.retry_failed) or output==artifacts or not output.is_relative_to(artifacts):
        parser.error('output must be a new directory under ignored artifacts/')
    if args.retry_failed:
        report_path=output/'report.json'
        raw=report_path.read_bytes()
        report=json.loads(raw)
        if report['status']!='failed':parser.error('retry requires a failed benchmark report')
        executable=output/'benchmark.exe'
        if hashlib.sha256(executable.read_bytes()).hexdigest()!=report['executable_sha256']:
            parser.error('pinned executable no longer matches the original report')
        attempt=1
        while (output/f'report-before-retry-{attempt}.json').exists():attempt+=1
        (output/f'report-before-retry-{attempt}.json').write_bytes(raw)
        for row in report['runs']:
            if row['status']=='passed':continue
            previous={k:v for k,v in row.items() if k not in ['metrics','retry_history']}
            config=json.loads((output/row['folder']/'config.json').read_text())
            folder=output/f"{row['name']}-{row['resolution']}-{row['repeat']}-retry-{attempt}"
            row.setdefault('retry_history',[]).append(previous)
            try:
                row['metrics']=run_case(executable,folder,config,.25)
                row['status']='passed';row.pop('error',None)
            except (OSError,RuntimeError,TimeoutError) as error:row['error']=str(error)
            row['folder']=str(folder.relative_to(output))
            print(folder.name,row['status'],flush=True)
        report['status']='passed' if all(r['status']=='passed' for r in report['runs']) else 'failed'
        report['retry_controller_sources']=source_hashes()
        report_path.write_text(json.dumps(report,indent=2),encoding='utf-8')
        (output/'report.md').write_text(markdown(report),encoding='utf-8')
        if report['status']!='passed':raise SystemExit(1)
        return
    workloads=json.loads((ROOT/'benchmarks/workloads.json').read_text())['workloads']
    selected=set(args.workloads.split(','))
    if args.workloads!='all':
        if selected-{w['name'] for w in workloads}:parser.error('unknown workload name')
        workloads=[w for w in workloads if w['name'] in selected]
    resolutions=[]
    for text in args.resolutions.split(','):
        width,height=map(int,text.split('x'))
        if not(1<=width<=16384 and 1<=height<=16384 and width*height*4<=64*1024**2):parser.error('invalid output size')
        resolutions.append((width,height))
    output.mkdir(parents=True)
    executable=output/'benchmark.exe';shutil.copyfile(args.executable,executable)
    machine=json.loads(command(['pwsh','-NoProfile','-Command',"[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); [ordered]@{cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors);gpu=@(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion);os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,TotalVisibleMemorySize);power_plan=(& powercfg /getactivescheme)} | ConvertTo-Json -Depth 5"]))
    report=dict(schema_version=1,status='running',revision=command(['git','rev-parse','HEAD']),
        build_profile='release' if args.executable.parent.parent.name=='release' else 'custom executable',
        working_tree=command(['git','status','--porcelain']).splitlines(),sources=source_hashes(),
        executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),rustc=command(['rustc','--version']),
        machine=machine,logical_cpus=os.cpu_count(),configuration=dict(seconds=args.seconds,warmup=args.warmup,idle=args.idle,
            trim_paused=args.trim_paused,visual_effects=not args.no_vfx,profile_gpu=args.profile_gpu,target_fps=args.fps,repeats=args.repeat,software=args.software,resolutions=resolutions),runs=[])
    for repeat in range(1,args.repeat+1):
        for width,height in resolutions:
            for workload in workloads:
                folder=output/f"{workload['name']}-{width}x{height}-{repeat}"
                source=dict(kind=workload['kind'])
                if source['kind']=='workshop':
                    project=json.loads((args.workshop_root/workload['folder']/'project.json').read_text(encoding='utf-8-sig'))
                    path=args.workshop_root/workload['folder']/project['file']
                    if project['type'].lower()=='scene':path=path.with_suffix('.pkg')
                    source=dict(kind='scene',path=str(path),engine_assets=str(args.engine_assets))
                row=dict(name=workload['name'],description=workload['description'],resolution=f'{width}x{height}',repeat=repeat,
                    folder=str(folder.relative_to(output)),status='failed')
                config=dict(source=source,width=width,height=height,warmup_seconds=args.warmup,measure_seconds=args.seconds,
                    idle_seconds=args.idle,target_fps=args.fps,software=args.software,profile_gpu=args.profile_gpu)
                if args.trim_paused:config['trim_paused']=True
                if args.no_vfx:config['visual_effects']=False
                try:
                    row['metrics']=run_case(executable,folder,config,.25)
                    row['status']='passed'
                    phases={p['name']:p for p in row['metrics']['phases']}
                    print(f"{folder.name}: paced {phases['paced']['completed_fps']:.2f}, uncapped {phases['uncapped']['completed_fps']:.2f} fps",flush=True)
                except (OSError,RuntimeError,TimeoutError) as error:
                    row['error']=str(error)
                    print(f'{folder.name}: FAILED {error}',flush=True)
                report['runs'].append(row)
                (output/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
                (output/'report.md').write_text(markdown(report),encoding='utf-8')
    report['status']='passed' if all(r['status']=='passed' for r in report['runs']) else 'failed'
    (output/'report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(f"{report['status']}: {output/'report.md'}")
    if report['status']!='passed':raise SystemExit(1)


if __name__=='__main__':
    from benchmark_windows import keep_awake
    with keep_awake():main()
