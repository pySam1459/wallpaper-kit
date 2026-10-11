[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ScenePath,
    [Parameter(Mandatory)][string]$EngineAssets,
    [string]$Executable = 'target/debug/examples/native-viewer.exe',
    [string]$Output = ('artifacts/workshop-playback-' + [Guid]::NewGuid().ToString('N').Substring(0,8)),
    [ValidateRange(1,3840)][int]$Width = 960,
    [ValidateRange(1,2160)][int]$Height = 540,
    [switch]$Software
)
$ErrorActionPreference='Stop'
$start=[Diagnostics.ProcessStartInfo]::new([IO.Path]::GetFullPath($Executable))
$start.UseShellExecute=$false
$start.CreateNoWindow=$true
$start.RedirectStandardInput=$true
$start.RedirectStandardOutput=$true
$start.RedirectStandardError=$true
$start.WorkingDirectory=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$outputPath=[IO.Path]::GetFullPath($Output)
foreach($argument in @('--output',$outputPath,'--scene',[IO.Path]::GetFullPath($ScenePath),'--engine-assets',[IO.Path]::GetFullPath($EngineAssets),'--linear-images')) { $start.ArgumentList.Add($argument) }
if($Software) { $start.ArgumentList.Add('--software') }
$process=[Diagnostics.Process]::new()
$process.StartInfo=$start
$transcript=[Collections.Generic.List[object]]::new()
$request=0
$started=$false
$errors=$null
function Read-Reply {
    $read=$process.StandardOutput.ReadLineAsync()
    if(-not $read.Wait(30000)) { throw 'Owned viewer timed out' }
    if($null -eq $read.Result) { throw 'Owned viewer exited before replying' }
    $reply=$read.Result|ConvertFrom-Json
    $transcript.Add($reply)
    if($reply.event -eq 'failed' -or $reply.ok -eq $false) { throw "Viewer failed: $($read.Result)" }
    return $reply
}
function Send([hashtable]$Command) {
    $script:request+=1
    $Command.id=$script:request
    $process.StandardInput.WriteLine(($Command|ConvertTo-Json -Compress))
    $process.StandardInput.Flush()
    $reply=Read-Reply
    if($reply.id -ne $Command.id) { throw 'Unexpected request identity' }
    return $reply
}
function Seek([long]$Time) {
    $reply=Send @{command='image_at';time_ms=$Time}
    if($reply.state.image_playback.playing -or $reply.state.image_playback.pending_sample -or [decimal]$reply.state.image_playback.presented_sample.time_ns -ne [decimal]$Time*1000000) { throw 'Paused scene seek failed' }
    return $reply
}
try {
    if(-not $process.Start()) { throw 'Viewer launch failed' }
    $started=$true
    $errors=$process.StandardError.ReadToEndAsync()
    $ready=Read-Reply
    if($ready.event -ne 'ready' -or $ready.state.pid -ne $process.Id) { throw 'Missing owned viewer readiness' }
    $null=Seek 0
    $null=Send @{command='resize';width=$Width;height=$Height}
    $checkpoints=foreach($time in @(0,1500,4000,8000)) {
        $reply=Seek $time
        $null=Send @{command='capture';name=('time-{0:D5}.png' -f $time)}
        @{time_ms=$time;quads=$reply.state.frame.image_layer_count}
    }
    $null=Seek 1500
    $null=Send @{command='capture';name='rewind.png'}
    if((Get-FileHash (Join-Path $outputPath 'rewind.png')).Hash -ne (Get-FileHash (Join-Path $outputPath 'time-01500.png')).Hash) { throw 'Controlled scene rewind differs; check wall-clock text crossing a minute boundary' }
    $before=Send @{command='inspect'}
    Start-Sleep -Milliseconds 500
    $after=Send @{command='inspect'}
    if($before.state.presented -ne $after.state.presented) { throw 'Paused viewer continued rendering' }
    $null=Seek 0
    $playing=Send @{command='play_images'}
    Start-Sleep -Milliseconds 8000
    $paused=Send @{command='pause_images'}
    $null=Send @{command='capture';name='live-final.png'}
    $null=Send @{command='stop'}
    if(-not $process.WaitForExit(15000) -or $process.ExitCode -ne 0) { throw 'Owned viewer did not exit successfully' }
    $timeline=Get-Content (Join-Path $outputPath 'image-timeline.json') -Raw|ConvertFrom-Json
    $live=@($timeline.observations|Where-Object { $_.frame_id -gt $playing.state.frame.frame_id -and $_.frame_id -le $paused.state.frame.frame_id })
    if($live.Count -lt 2) { throw 'Missing live observations' }
    $gaps=for($i=1;$i -lt $live.Count;$i++) { $live[$i].present_call_return_ms-$live[$i-1].present_call_return_ms }
    [ordered]@{status='passed';package_sha256=$ready.state.image_playback.report.package_sha256;playback=$ready.state.image_playback.report;adapter=$ready.state.frame.adapter;resolution=@($Width,$Height);checkpoints=$checkpoints;live_samples=$live.Count;maximum_presentation_call_gap_ms=($gaps|Measure-Object -Maximum).Maximum;paused_idle='passed';rewind='identical PNG hash';cleanup='Owned process exited';executable_sha256=(Get-FileHash $start.FileName).Hash;limits=@('Controlled capture checks deterministic scene sampling; clock text follows local wall time.','Presentation-call observations do not establish display cadence or exact engine effect fidelity.')}|ConvertTo-Json -Depth 12|Set-Content (Join-Path $outputPath 'report.json') -Encoding utf8
    Write-Output "Evidence: $outputPath"
} finally {
    if($started -and -not $process.HasExited) { $process.Kill();$null=$process.WaitForExit(10000) }
    if(Test-Path $outputPath) {
        $transcript|ConvertTo-Json -Depth 12|Set-Content (Join-Path $outputPath 'transcript.json') -Encoding utf8
        if($errors -and $errors.IsCompleted) { $errors.Result|Set-Content (Join-Path $outputPath 'stderr.log') -Encoding utf8 }
    }
    $process.Dispose()
}
