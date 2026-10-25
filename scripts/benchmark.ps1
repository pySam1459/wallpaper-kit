[CmdletBinding()]
param(
    [string]$Output = ('artifacts/benchmark-' + (Get-Date -Format 'yyyyMMdd-HHmmss')),
    [string]$Workloads = 'all',
    [string]$Resolutions = '1920x1080,3840x2160',
    [int]$Repeat = 2,
    [int]$Seconds = 8,
    [int]$Warmup = 2,
    [int]$Idle = 2,
    [int]$Fps = 60,
    [switch]$Software,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (-not $SkipBuild) {
        cargo build --release --locked --example benchmark
        if ($LASTEXITCODE) { throw 'Release benchmark build failed' }
    }
    $benchmarkArguments = @('-B', 'scripts/benchmark.py', '--output', $Output, '--workloads', $Workloads,
        '--resolutions', $Resolutions, '--repeat', $Repeat, '--seconds', $Seconds, '--warmup', $Warmup, '--idle', $Idle, '--fps', $Fps)
    if ($Software) { $benchmarkArguments += '--software' }
    python @benchmarkArguments
    if ($LASTEXITCODE) { throw 'Benchmark failed; retained reports identify incomplete runs' }
} finally { Pop-Location }
