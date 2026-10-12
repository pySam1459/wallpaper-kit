param(
    [Parameter(Mandatory)][string]$ScenePath,
    [Parameter(Mandatory)][string]$EngineAssets,
    [string]$Output = ('artifacts/image-motion-' + [Guid]::NewGuid().ToString('N').Substring(0,8))
)
$ErrorActionPreference = 'Stop'
$outputPath = [IO.Path]::GetFullPath($Output)
$start = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSScriptRoot '../target/debug/examples/native-viewer.exe'))
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
foreach ($argument in @('--output',$outputPath,'--scene',[IO.Path]::GetFullPath($ScenePath),'--engine-assets',[IO.Path]::GetFullPath($EngineAssets),'--linear-images')) { $start.ArgumentList.Add($argument) }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
$transcript = [Collections.Generic.List[object]]::new()
$request = 0
$started = $false
function Read-Reply {
    $task = $process.StandardOutput.ReadLineAsync()
    if (!$task.Wait(30000) -or $null -eq $task.Result) { throw 'Owned image viewer did not respond' }
    $reply = $task.Result | ConvertFrom-Json
    $transcript.Add($reply)
    if ($reply.event -eq 'failed' -or $reply.ok -eq $false) { throw "Viewer failed: $($task.Result)" }
    return $reply
}
function Send-Command([hashtable]$Command) {
    $script:request += 1
    $Command.id = $script:request
    $process.StandardInput.WriteLine(($Command | ConvertTo-Json -Compress))
    $process.StandardInput.Flush()
    $reply = Read-Reply
    if ($reply.id -ne $Command.id) { throw 'Viewer reply identity mismatch' }
    return $reply
}
try {
    if (!$process.Start()) { throw 'Viewer launch failed' }
    $started = $true
    $errors = $process.StandardError.ReadToEndAsync()
    $ready = Read-Reply
    if ($ready.event -ne 'ready' -or $ready.state.pid -ne $process.Id) { throw 'Missing owned viewer readiness' }
    $profile = $ready.state.image_playback.report
    if (@($profile.image_motions.PSObject.Properties).Count -eq 0) { throw 'No image-motion profiles were prepared' }
    $null = Send-Command @{command='resize';width=1280;height=720}
    $hashes = @()
    foreach ($time in @(0,1500,4000,0)) {
        $reply = Send-Command @{command='image_at';time_ms=$time}
        if ($reply.state.image_playback.playing -or [decimal]$reply.state.image_playback.presented_sample.time_ns -ne [decimal]$time * 1000000) { throw 'Exact paused seek failed' }
        $name = "frame-$($hashes.Count)-$time.png"
        $null = Send-Command @{command='capture';name=$name}
        $hashes += (Get-FileHash -LiteralPath (Join-Path $outputPath $name)).Hash
    }
    if ($hashes[0] -eq $hashes[1] -or $hashes[0] -ne $hashes[3]) { throw 'Image motion must change pixels and rewind exactly' }
    $before = Send-Command @{command='inspect'}
    $null = Send-Command @{command='play_images'}
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        Start-Sleep -Milliseconds 250
        $live = Send-Command @{command='inspect'}
    } while ([DateTime]::UtcNow -lt $deadline)
    $null = Send-Command @{command='pause_images'}
    $timeline = @($transcript | Where-Object { $_.state.image_playback.playing -and $_.state.image_playback.presented_sample } | ForEach-Object {
        @{sequence=$_.state.frame.sequence;sample_time_ns=$_.state.image_playback.presented_sample.time_ns}
    })
    $samples = $live.state.frame.sequence - $before.state.frame.sequence
    if ($samples -lt 30) { throw 'Insufficient live playback observations' }
    $null = Send-Command @{command='image_at';time_ms=1500}
    $settled = Send-Command @{command='inspect'}
    Start-Sleep -Milliseconds 250
    $paused = Send-Command @{command='inspect'}
    if ($settled.state.frame.sequence -ne $paused.state.frame.sequence) { throw 'Paused playback kept drawing' }
    $null = Send-Command @{command='resize';width=3840;height=2160}
    $large = Send-Command @{command='image_at';time_ms=1500}
    if ($large.state.frame.width -ne 3840 -or $large.state.frame.height -ne 2160) { throw 'Full-resolution frame missing' }
    $null = Send-Command @{command='capture';name='4k-1500.png'}
    $null = Send-Command @{command='stop'}
    if (!$process.WaitForExit(10000) -or $process.ExitCode -ne 0) { throw 'Owned viewer did not exit cleanly' }
    @{passed=$true;image_motions=$profile.image_motions;settings=$profile.settings;package_sha256=$profile.package_sha256;pixel_sha256=$hashes;live_observations=$samples;live_timeline=$timeline;resolutions=@(@(1280,720),@(3840,2160));omissions=$profile.omissions;cleanup='Owned viewer exited';limits=@('Controlled captures prove content and rewind. Live observations do not measure display cadence. Engine phase and complete scene fidelity require separate comparison.')} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'report.json')
    Write-Output "Evidence: $outputPath"
} finally {
    if ($started -and !$process.HasExited) { $process.Kill(); $null=$process.WaitForExit(10000) }
    if (Test-Path -LiteralPath $outputPath) {
        $transcript | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'transcript.json')
        if ($errors -and $errors.IsCompleted) { $errors.Result | Set-Content -LiteralPath (Join-Path $outputPath 'stderr.log') }
    }
    $process.Dispose()
}
