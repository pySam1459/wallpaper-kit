"""Independent metric checks; Windows telemetry itself is exercised by live smoke runs."""
import unittest
import runpy
from pathlib import Path
from benchmark import distribution, summarize_phase


class BenchmarkMetrics(unittest.TestCase):
    def test_comparison_preserves_zero_baselines_ranges_and_policy_changes(self):
        import copy
        compare=runpy.run_path(str(Path(__file__).with_name('compare-benchmarks.py')))['compare']
        before=dict(status='passed',machine={},configuration={},results=[dict(name='fixture',resolution='4x4',
            source_sha256='same',omission_counts=[0],metrics=dict(paused_cpu_percent=dict(median=0,minimum=0,maximum=0),
            paused_private_mib=dict(median=100,minimum=90,maximum=110)))])
        after=copy.deepcopy(before)
        after['configuration']['trim_paused']=True
        after['results'][0]['metrics']['paused_private_mib']=dict(median=50,minimum=45,maximum=55)
        report=compare(before,after,'Release idle resources')
        self.assertEqual(report['results'][0]['metrics']['paused_private_mib']['percent_change'],-50)
        self.assertIsNone(report['results'][0]['metrics']['paused_cpu_percent']['percent_change'])
        self.assertEqual(report['configuration_changes']['trim_paused'],dict(before=False,after=True))
        after['results'][0]['source_sha256']='different'
        with self.assertRaises(ValueError):compare(before,after,'changed content')
        before['status']='failed'
        with self.assertRaises(ValueError):compare(before,after,'incomplete')

    def test_baseline_requires_complete_data_and_retains_repeat_spread(self):
        summary=runpy.run_path(str(Path(__file__).with_name('summarize-benchmark.py')))
        self.assertEqual(summary['spread']([1,3]),dict(median=2,minimum=1,maximum=3))
        for report in [dict(status='running',runs=[]),dict(status='passed',runs=[]),
                       dict(status='failed',runs=[dict(status='failed')])]:
            with self.assertRaises(ValueError):summary['summarize'](report)

    def test_empty_metrics_are_unavailable_and_a_stall_is_preserved(self):
        self.assertIsNone(distribution([]))
        self.assertEqual(distribution([1,2,3,120])['p95'],120)
        self.assertFalse(summarize_phase(dict(start_unix_ms=0,end_unix_ms=1000),[],32)['available'])

    def test_cpu_capacity_and_phase_boundaries(self):
        rows=[]
        for i in range(5):
            rows.append(dict(unix_ms=i*1000,monotonic_s=i,process=dict(cpu_seconds=i*.5,
                working_set_bytes=100,private_bytes=200+i*10,handles=20,active_processes=2,
                system_cpu_total=i*100,system_cpu_idle=i*80,io_read_bytes=i*20,io_write_bytes=i*10),
                gpu=dict(available=True,busiest_engine_percent=25,dedicated_bytes=400,shared_bytes=100)))
        result=summarize_phase(dict(start_unix_ms=500,end_unix_ms=3500),rows,32)
        self.assertEqual(result['samples'],3)
        self.assertEqual(result['cpu_cores_mean'],.5)
        self.assertEqual(result['cpu_machine_percent_mean'],1.5625)
        self.assertEqual(result['private_growth_bytes'],20)
        self.assertEqual(result['gpu_busiest_engine_percent']['mean'],25)
        self.assertAlmostEqual(result['coverage_fraction'],2/3)
        for row in rows:row['gpu']=dict(available=False,error='unavailable')
        result=summarize_phase(dict(start_unix_ms=500,end_unix_ms=3500),rows,32)
        self.assertIsNone(result['gpu_busiest_engine_percent'])


if __name__=='__main__':
    unittest.main()
