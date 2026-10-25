"""Windows job accounting and PDH telemetry for the benchmark controller only."""
import ctypes as c
from ctypes import wintypes as w
import re
from contextlib import contextmanager


k = c.WinDLL('kernel32', use_last_error=True)
pdh = c.WinDLL('pdh', use_last_error=True)
U64 = c.c_ulonglong
SIZE = c.c_size_t


class Io(c.Structure):
    _fields_ = [(name, U64) for name in ['reads', 'writes', 'other', 'read_bytes', 'write_bytes', 'other_bytes']]


class BasicLimits(c.Structure):
    _fields_ = [('process_time', c.c_longlong), ('job_time', c.c_longlong), ('flags', w.DWORD),
                ('min_working_set', SIZE), ('max_working_set', SIZE), ('process_limit', w.DWORD),
                ('affinity', SIZE), ('priority', w.DWORD), ('scheduling', w.DWORD)]


class Limits(c.Structure):
    _fields_ = [('basic', BasicLimits), ('io', Io), ('process_memory', SIZE), ('job_memory', SIZE),
                ('peak_process_memory', SIZE), ('peak_job_memory', SIZE)]


class Accounting(c.Structure):
    _fields_ = [(name, c.c_longlong) for name in ['user', 'kernel', 'period_user', 'period_kernel']] + [
        (name, w.DWORD) for name in ['page_faults', 'processes', 'active', 'terminated']]


class Memory(c.Structure):
    _fields_ = [('size', w.DWORD), ('faults', w.DWORD)] + [
        (name, SIZE) for name in ['peak_working_set', 'working_set', 'peak_paged', 'paged',
                                'peak_nonpaged', 'nonpaged', 'pagefile', 'peak_pagefile', 'private']]


class CounterUnion(c.Union):
    _fields_ = [('number', c.c_double), ('integer', c.c_longlong), ('text', w.LPWSTR)]


class CounterValue(c.Structure):
    _fields_ = [('status', w.DWORD), ('value', CounterUnion)]


class CounterItem(c.Structure):
    _fields_ = [('name', w.LPWSTR), ('counter', CounterValue)]


def bind(dll, name, args, result=w.BOOL):
    fn = getattr(dll, name)
    fn.argtypes, fn.restype = args, result
    return fn


create_job = bind(k, 'CreateJobObjectW', [c.c_void_p, w.LPCWSTR], w.HANDLE)
set_job = bind(k, 'SetInformationJobObject', [w.HANDLE, c.c_int, c.c_void_p, w.DWORD])
query_job = bind(k, 'QueryInformationJobObject', [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.c_void_p])
assign_job = bind(k, 'AssignProcessToJobObject', [w.HANDLE, w.HANDLE])
open_process = bind(k, 'OpenProcess', [w.DWORD, w.BOOL, w.DWORD], w.HANDLE)
close = bind(k, 'CloseHandle', [w.HANDLE])
get_memory = bind(k, 'K32GetProcessMemoryInfo', [w.HANDLE, c.POINTER(Memory), w.DWORD])
get_handles = bind(k, 'GetProcessHandleCount', [w.HANDLE, c.POINTER(w.DWORD)])
system_times = bind(k, 'GetSystemTimes', [c.POINTER(U64), c.POINTER(U64), c.POINTER(U64)])
execution_state = bind(k, 'SetThreadExecutionState', [w.DWORD], w.DWORD)
query_open = bind(pdh, 'PdhOpenQueryW', [w.LPCWSTR, SIZE, c.POINTER(w.HANDLE)], w.DWORD)
counter_add = bind(pdh, 'PdhAddEnglishCounterW', [w.HANDLE, w.LPCWSTR, SIZE, c.POINTER(w.HANDLE)], w.DWORD)
collect = bind(pdh, 'PdhCollectQueryData', [w.HANDLE], w.DWORD)
counter_array = bind(pdh, 'PdhGetFormattedCounterArrayW', [w.HANDLE, w.DWORD, c.POINTER(w.DWORD), c.POINTER(w.DWORD), c.c_void_p], w.DWORD)
query_close = bind(pdh, 'PdhCloseQuery', [w.HANDLE], w.DWORD)


def checked(ok):
    if not ok:
        raise c.WinError(c.get_last_error())


@contextmanager
def keep_awake():
    """Prevent idle system sleep only; retain display and power-plan settings."""
    checked(execution_state(0x80000001))
    try:
        yield
    finally:
        checked(execution_state(0x80000000))


class Job:
    """Account for the worker and all decoders, including CPU of exited children."""
    def __init__(self, pid):
        self.handle = create_job(None, None)
        checked(self.handle)
        try:
            limits = Limits()
            limits.basic.flags = 0x2000  # Kill only this owned job on close.
            checked(set_job(self.handle, 9, c.byref(limits), c.sizeof(limits)))
            process = open_process(0x0100 | 0x0001, False, pid)
            checked(process)
            try:
                checked(assign_job(self.handle, process))
            finally:
                close(process)
        except BaseException:
            self.close()
            raise
        self.seen_pids = {pid}

    def close(self):
        if self.handle:
            close(self.handle)
            self.handle = None

    def sample(self):
        accounting, limits, io = Accounting(), Limits(), Io()
        checked(query_job(self.handle, 1, c.byref(accounting), c.sizeof(accounting), None))
        checked(query_job(self.handle, 9, c.byref(limits), c.sizeof(limits), None))
        # Job basic-and-I/O accounting is the basic block followed by IO_COUNTERS.
        class WithIo(c.Structure):
            _fields_ = [('basic', Accounting), ('io', Io)]
        combined = WithIo()
        checked(query_job(self.handle, 8, c.byref(combined), c.sizeof(combined), None))
        io = combined.io
        ids = c.create_string_buffer(8 + 256*c.sizeof(SIZE))
        checked(query_job(self.handle, 3, ids, len(ids), None))
        assigned, count = (w.DWORD*2).from_buffer(ids)
        if assigned > 256:
            raise RuntimeError('benchmark job exceeded 256 processes')
        pids = list((SIZE*count).from_buffer(ids, 8))
        self.seen_pids.update(pids)
        private = working = handles = faults = measured = 0
        for pid in pids:
            handle = open_process(0x0400 | 0x0010, False, pid)
            if not handle:  # A decoder may exit between enumeration and sampling.
                continue
            try:
                memory, handle_count = Memory(), w.DWORD()
                memory.size = c.sizeof(memory)
                if get_memory(handle, c.byref(memory), memory.size):
                    private += memory.private
                    working += memory.working_set
                    faults += memory.faults
                    measured += 1
                if get_handles(handle, c.byref(handle_count)):
                    handles += handle_count.value
            finally:
                close(handle)
        idle, kernel, user = U64(), U64(), U64()
        checked(system_times(c.byref(idle), c.byref(kernel), c.byref(user)))
        return dict(cpu_seconds=(accounting.user+accounting.kernel)/1e7,
                    working_set_bytes=working, private_bytes=private, handles=handles,
                    page_faults_live_processes=faults, active_processes=accounting.active,
                    memory_processes_sampled=measured, total_processes=accounting.processes,
                    peak_job_private_bytes=limits.peak_job_memory,
                    io_read_bytes=io.read_bytes, io_write_bytes=io.write_bytes,
                    system_cpu_idle=idle.value, system_cpu_total=kernel.value+user.value)


class GpuCounters:
    def __init__(self):
        self.query = w.HANDLE()
        self.counters = {}
        self.error = None
        try:
            status = query_open(None, 0, c.byref(self.query))
            if status:
                raise RuntimeError(f'PdhOpenQueryW: {status:#x}')
            for key, path in [
                ('engine', r'\GPU Engine(*)\Utilization Percentage'),
                ('dedicated', r'\GPU Process Memory(*)\Dedicated Usage'),
                ('shared', r'\GPU Process Memory(*)\Shared Usage'),
            ]:
                counter = w.HANDLE()
                status = counter_add(self.query, path, 0, c.byref(counter))
                if status:
                    raise RuntimeError(f'{path}: {status:#x}')
                self.counters[key] = counter
            collect(self.query)
        except (OSError, RuntimeError) as error:
            self.error = str(error)
            self.close()

    def close(self):
        if self.query:
            query_close(self.query)
            self.query = w.HANDLE()

    @staticmethod
    def values(counter):
        size, count = w.DWORD(), w.DWORD()
        status = counter_array(counter, 0x200, c.byref(size), c.byref(count), None)
        if status != 0x800007D2:  # PDH_MORE_DATA
            raise RuntimeError(f'PdhGetFormattedCounterArrayW size: {status:#x}')
        buffer = c.create_string_buffer(size.value)
        status = counter_array(counter, 0x200, c.byref(size), c.byref(count), buffer)
        if status:
            raise RuntimeError(f'PdhGetFormattedCounterArrayW: {status:#x}')
        items = c.cast(buffer, c.POINTER(CounterItem))
        return [(items[i].name, items[i].counter.value.number) for i in range(count.value)
                if items[i].counter.status in (0, 1)]

    def sample(self, pids):
        if self.error:
            return dict(available=False, error=self.error)
        try:
            status = collect(self.query)
            if status:
                raise RuntimeError(f'PdhCollectQueryData: {status:#x}')
            groups = {key:self.values(counter) for key,counter in self.counters.items()}
            def owned(name):
                match = re.match(r'pid_(\d+)_', name)
                return match and int(match[1]) in pids
            engines = {}
            for name,value in groups['engine']:
                if owned(name):
                    engine = re.sub(r'^pid_\d+_', '', name)
                    engines[engine] = engines.get(engine, 0.) + max(0., value)
            return dict(available=True, busiest_engine_percent=min(100.,max(engines.values(), default=0.)),
                        engines=engines, dedicated_bytes=sum(value for name,value in groups['dedicated'] if owned(name)),
                        shared_bytes=sum(value for name,value in groups['shared'] if owned(name)))
        except RuntimeError as error:
            return dict(available=False, error=str(error))
