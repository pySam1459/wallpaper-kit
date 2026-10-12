param(
    [Parameter(Mandatory)][string]$ScenePath,
    [Parameter(Mandatory)][string]$EngineAssets,
    [Parameter(Mandatory)][long[]]$ExpectedParticles,
    [string]$Output = ('artifacts/particle-profiles-' + [Guid]::NewGuid().ToString('N').Substring(0,8)),
    [int]$Width = 1280,
    [int]$Height = 720
)
$ErrorActionPreference = 'Stop'
$outputPath = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $outputPath) { throw 'Evidence directory already exists' }
$executable = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../target/debug/examples/native-viewer.exe'))
$executableHash = (Get-FileHash -LiteralPath $executable).Hash
$start = [Diagnostics.ProcessStartInfo]::new($executable)
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
    if (!$task.Wait(30000) -or $null -eq $task.Result) { throw 'Owned particle viewer did not respond' }
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
    foreach ($id in $ExpectedParticles) { if ($id -notin $profile.particle_objects) { throw "Particle $id not prepared" } }
    $null = Send-Command @{command='resize';width=$Width;height=$Height}
    $hashes = @()
    $times = @(0,500,1500,4000,0)
    foreach ($time in $times) {
        $reply = Send-Command @{command='image_at';time_ms=$time}
        if ($reply.state.image_playback.playing -or [decimal]$reply.state.image_playback.presented_sample.time_ns -ne [decimal]$time * 1000000) { throw 'Exact paused seek failed' }
        $name = "frame-$($hashes.Count)-$time.png"
        $null = Send-Command @{command='capture';name=$name}
        $hashes += (Get-FileHash -LiteralPath (Join-Path $outputPath $name)).Hash
    }
    if ($hashes[0] -ne $hashes[-1]) { throw 'Particle rewind pixels differ' }
    $before = Send-Command @{command='inspect'}
    $null = Send-Command @{command='play_images'}
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        Start-Sleep -Milliseconds 250
        $live = Send-Command @{command='inspect'}
    } while ([DateTime]::UtcNow -lt $deadline)
    $null = Send-Command @{command='pause_images'}
    $samples = $live.state.frame.sequence - $before.state.frame.sequence
    if ($samples -lt 30) { throw 'Insufficient live playback observations' }
    $settled = Send-Command @{command='inspect'}
    Start-Sleep -Milliseconds 250
    $paused = Send-Command @{command='inspect'}
    if ($settled.state.frame.sequence -ne $paused.state.frame.sequence) { throw 'Paused playback kept drawing' }
    $null = Send-Command @{command='stop'}
    if (!$process.WaitForExit(10000) -or $process.ExitCode -ne 0) { throw 'Owned viewer did not exit cleanly' }
    @{passed=$true;executable_sha256=$executableHash;playback=$profile;times_ms=$times;pixel_sha256=$hashes;changed_pixels=($hashes[0] -ne $hashes[1]);live_observations=$samples;resolution=@($Width,$Height);cleanup='Owned viewer exited';limits=@('Captures and native presentation calls do not establish displayed cadence or engine fidelity.')} | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $outputPath 'report.json')
    Write-Output "Evidence: $outputPath"
} finally {
    if ($started -and !$process.HasExited) { $process.Kill(); $null=$process.WaitForExit(10000) }
    if (Test-Path -LiteralPath $outputPath) {
        $transcript | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $outputPath 'transcript.json')
        if ($errors -and $errors.IsCompleted) { $errors.Result | Set-Content -LiteralPath (Join-Path $outputPath 'stderr.log') }
    }
    $process.Dispose()
}
