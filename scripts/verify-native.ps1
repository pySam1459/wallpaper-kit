[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [switch]$Software
)

$ErrorActionPreference = 'Stop'
$outputPath = [IO.Path]::GetFullPath($Output)
$executablePath = [IO.Path]::GetFullPath($Executable)
$start = [Diagnostics.ProcessStartInfo]::new($executablePath)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.WorkingDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$start.ArgumentList.Add('--output')
$start.ArgumentList.Add($outputPath)
if ($Software) { $start.ArgumentList.Add('--software') }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
$transcript = [Collections.Generic.List[object]]::new()
$errors = $null
$started = $false

function Read-Reply {
    $task = $process.StandardOutput.ReadLineAsync()
    if (-not $task.Wait(15000)) { throw 'Viewer response timed out' }
    $line = $task.Result
    if ($null -eq $line) { throw 'Viewer exited before responding' }
    $reply = $line | ConvertFrom-Json
    $transcript.Add($reply)
    if ($reply.event -eq 'failed' -or $reply.ok -eq $false) { throw "Viewer error: $line" }
    return $reply
}

function Send-Command([hashtable]$Command) {
    $process.StandardInput.WriteLine(($Command | ConvertTo-Json -Compress))
    $process.StandardInput.Flush()
    $reply = Read-Reply
    if ($reply.id -ne $Command.id) { throw 'Viewer replied to an unexpected request' }
    return $reply
}

try {
    if (-not $process.Start()) { throw 'Viewer launch failed' }
    $started = $true
    $errors = $process.StandardError.ReadToEndAsync()
    $ready = Read-Reply
    if ($ready.event -ne 'ready' -or $ready.state.pid -ne $process.Id) { throw 'Viewer readiness identity mismatch' }
    $initialCapture = Send-Command @{id=1; command='capture'; name='initial.png'}
    if (($initialCapture.uniform_rgba -join ',') -ne '16,24,32,255') { throw 'Initial native GPU pixels differ' }
    $before = Send-Command @{id=2; command='inspect'}
    $sampleStart = Get-Date
    $cpuBefore = $process.TotalProcessorTime.TotalMilliseconds
    # This wait defines an idle measurement interval, not a guessed readiness delay.
    Start-Sleep -Milliseconds 1000
    $process.Refresh()
    $cpuAfter = $process.TotalProcessorTime.TotalMilliseconds
    $workingSet = $process.WorkingSet64
    $handles = $process.HandleCount
    $elapsedMs = ((Get-Date) - $sampleStart).TotalMilliseconds
    $after = Send-Command @{id=3; command='inspect'}
    if ($before.state.frame.frame_id -ne $after.state.frame.frame_id -or $before.state.presented -ne $after.state.presented) {
        throw 'Idle viewer performed unsolicited presentation'
    }
    $resized = Send-Command @{id=4; command='resize'; width=317; height=193}
    if ($resized.state.frame.width -ne 317 -or $resized.state.frame.height -ne 193) { throw 'Viewer resize dimensions differ' }
    $resizedCapture = Send-Command @{id=5; command='capture'; name='resized.png'}
    if (($resizedCapture.uniform_rgba -join ',') -ne '16,24,32,255') { throw 'Resized native GPU pixels differ' }
    $null = Send-Command @{id=6; command='stop'}
    if (-not $process.WaitForExit(15000)) { throw 'Viewer did not shut down' }
    if ($process.ExitCode -ne 0) { throw "Viewer exit code $($process.ExitCode)" }
    [ordered]@{status='passed'; pid=$process.Id; executable=$executablePath; actions=$transcript;
        idle_sample=[ordered]@{elapsed_ms=$elapsedMs; process_cpu_ms=($cpuAfter-$cpuBefore); working_set_bytes=$workingSet; handles=$handles;
            method='Process.TotalProcessorTime delta over measured wall time; one sample, includes host and helper threads'};
        cleanup='Owned process exited'; capture='GPU copy into native swap-chain back buffer, followed by diagnostic readback';
        limits=@('Blank output only.', 'Captures do not establish final desktop composition.', 'No GPU time or host input latency measured.');
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $outputPath 'report.json') -Encoding utf8
} finally {
    # Only this process handle is eligible for cleanup. Never kill by name.
    if ($started -and -not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(10000) }
    if (Test-Path -LiteralPath $outputPath) {
        if ($errors -and $errors.IsCompleted) { $errors.Result | Set-Content -LiteralPath (Join-Path $outputPath 'stderr.log') -Encoding utf8 }
        $transcript | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $outputPath 'transcript.json') -Encoding utf8
    }
    $process.Dispose()
}
