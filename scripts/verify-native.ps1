[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [switch]$Software,
    [switch]$TextureFixture,
    [switch]$RegionFixture,
    [switch]$LayersFixture,
    [switch]$ExtractedFixture,
    [switch]$PagedFixture
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
if ($TextureFixture) { $start.ArgumentList.Add('--texture-fixture') }
if ($RegionFixture) { $start.ArgumentList.Add('--region-fixture') }
if ($LayersFixture) { $start.ArgumentList.Add('--layers-fixture') }
if ($ExtractedFixture) { $start.ArgumentList.Add('--extracted-fixture') }
if ($PagedFixture) { $start.ArgumentList.Add('--paged-fixture') }
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

$metadataId = 1000
function Assert-EffectBytes([long]$Expected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ($true) {
        $script:metadataId += 1
        $reply = Send-Command @{id=$script:metadataId; command='inspect'}
        $stats = $reply.state.output_pool
        if ($null -ne $stats -and $stats.published_frames -eq $reply.state.frame.sequence) {
            if ($stats.effect_texture_bytes -ne $Expected) { throw 'Native blur scratch accounting differs' }
            return
        }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Native blur statistics timed out' }
        Start-Sleep -Milliseconds 10
    }
}

try {
    if (-not $process.Start()) { throw 'Viewer launch failed' }
    $started = $true
    $errors = $process.StandardError.ReadToEndAsync()
    $ready = Read-Reply
    if ($ready.event -ne 'ready' -or $ready.state.pid -ne $process.Id) { throw 'Viewer readiness identity mismatch' }
    $initialCapture = Send-Command @{id=1; command='capture'; name='initial.png'}
    if ($LayersFixture) {
        if ($initialCapture.layers_check -ne $true) { throw 'Initial native composition pixels differ' }
    } elseif ($TextureFixture -or $RegionFixture -or $ExtractedFixture -or $PagedFixture) {
        if ($initialCapture.texture_check -ne $true) { throw 'Initial native texture pixels differ' }
    } elseif (($initialCapture.uniform_rgba -join ',') -ne '16,24,32,255') { throw 'Initial native GPU pixels differ' }
    $before = Send-Command @{id=2; command='inspect'}
    if ($LayersFixture) { Assert-EffectBytes 288 }
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
    if ($LayersFixture) {
        if ($resizedCapture.layers_check -ne $true) { throw 'Resized native composition pixels differ' }
    } elseif ($TextureFixture -or $RegionFixture -or $ExtractedFixture -or $PagedFixture) {
        if ($resizedCapture.texture_check -ne $true) { throw 'Resized native texture pixels differ' }
    } elseif (($resizedCapture.uniform_rgba -join ',') -ne '16,24,32,255') { throw 'Resized native GPU pixels differ' }
    $null = Send-Command @{id=6; command='fixture_at'; time_ms=0}
    if ($LayersFixture) { Assert-EffectBytes 0 }
    $motionInitial = Send-Command @{id=7; command='capture'; name='motion-0000.png'}
    if ($motionInitial.motion_check -ne $true) { throw 'Native fixture start differs' }
    $null = Send-Command @{id=8; command='fixture_at'; time_ms=1000}
    $motionMiddle = Send-Command @{id=9; command='capture'; name='motion-1000.png'}
    if ($motionMiddle.motion_check -ne $true) { throw 'Native fixture midpoint differs' }
    $null = Send-Command @{id=10; command='play_fixture'}
    # Defined live-playback interval, with no capture/readback during measurement.
    Start-Sleep -Milliseconds 4200
    $null = Send-Command @{id=11; command='pause_fixture'}
    # A controlled step settles any in-flight request before checking paused idle.
    $paused = Send-Command @{id=12; command='fixture_at'; time_ms=1000}
    Start-Sleep -Milliseconds 250
    $pausedAfter = Send-Command @{id=13; command='inspect'}
    if ($paused.state.frame.frame_id -ne $pausedAfter.state.frame.frame_id -or $pausedAfter.state.continuous_rendering) {
        throw 'Paused fixture scheduled new work'
    }
    $null = Send-Command @{id=14; command='stop'}
    if (-not $process.WaitForExit(15000)) { throw 'Viewer did not shut down' }
    if ($process.ExitCode -ne 0) { throw "Viewer exit code $($process.ExitCode)" }
    $motion = Get-Content -LiteralPath (Join-Path $outputPath 'motion-timeline.json') -Raw | ConvertFrom-Json
    $live = @($motion.samples | Where-Object mode -eq 'live')
    if ($live.Count -lt 2 -or $live[-1].playback_ms - $live[0].playback_ms -lt 4000) {
        throw 'Native live fixture did not cover two loops'
    }
    [ordered]@{status='passed'; pid=$process.Id; executable=$executablePath; actions=$transcript;
        idle_sample=[ordered]@{elapsed_ms=$elapsedMs; process_cpu_ms=($cpuAfter-$cpuBefore); working_set_bytes=$workingSet; handles=$handles;
            method='Process.TotalProcessorTime delta over measured wall time; one sample, includes host and helper threads'};
        cleanup='Owned process exited'; capture='GPU copy into native swap-chain back buffer, followed by diagnostic readback';
        texture_fixture=[bool]$TextureFixture;
        region_fixture=[bool]$RegionFixture;
        layers_fixture=[bool]$LayersFixture;
        extracted_fixture=[bool]$ExtractedFixture;
        paged_fixture=[bool]$PagedFixture;
        motion_fixture=[ordered]@{checkpoint_pixels='passed'; live_samples=$live.Count; two_loops='passed'; paused_idle='passed'};
        limits=@('Blank, diagnostic texture pages/regions, extracted frames, image composition and motion only.', 'Captures do not establish final desktop composition or live sprite playback.', 'No GPU time or host input latency measured.');
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
