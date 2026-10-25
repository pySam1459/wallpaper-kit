"""Live job accounting/cleanup check. Run separately from performance measurements."""
import ctypes as c
import os
import subprocess
import sys
import time
import unittest


@unittest.skipUnless(os.name=='nt','Windows job accounting only')
class WindowsAccounting(unittest.TestCase):
    def test_exited_child_cpu_is_retained_and_job_close_stops_owned_process(self):
        from benchmark_windows import Job, k, bind, w, open_process, close
        code="import subprocess,sys; input(); subprocess.run([sys.executable,'-c','import time; t=time.perf_counter(); x=bytearray(16*1024**2); exec(\"while time.perf_counter()-t<0.3: pass\")'],check=True); print('child-finished',flush=True); input()"
        process=subprocess.Popen([sys.executable,'-B','-c',code],stdin=subprocess.PIPE,stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NO_WINDOW)
        job=None
        handle=None
        try:
            job=Job(process.pid)
            handle=open_process(0x1000,False,process.pid)
            before=job.sample()
            process.stdin.write('\n');process.stdin.flush()
            self.assertEqual(process.stdout.readline().strip(),'child-finished')
            after=job.sample()
            self.assertGreaterEqual(after['total_processes'],2)
            self.assertGreater(after['cpu_seconds']-before['cpu_seconds'],.1)
            self.assertGreater(after['peak_job_private_bytes'],16*1024**2)
            job.close()
            process.wait(timeout=10)
            exit_code=w.DWORD()
            get_exit=bind(k,'GetExitCodeProcess',[w.HANDLE,c.POINTER(w.DWORD)])
            self.assertTrue(get_exit(handle,c.byref(exit_code)))
            self.assertNotEqual(exit_code.value,259)
        finally:
            if job:job.close()
            if handle:close(handle)
            if process.poll() is None:process.kill()
            process.wait(timeout=10)
            for stream in [process.stdin,process.stdout,process.stderr]:stream.close()


if __name__=='__main__':unittest.main()
