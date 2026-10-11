[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ScenePath,
    [Parameter(Mandatory)][string]$EngineAssets,
    [string]$Executable = 'target/debug/examples/native-viewer.exe',
    [string]$Output = ('artifacts/gojo-particles-' + [Guid]::NewGuid().ToString('N').Substring(0, 8)),
    [ValidateRange(1,3840)][int]$Width = 960,
    [ValidateRange(1,2160)][int]$Height = 540,
    [switch]$Software
)

$ErrorActionPreference = 'Stop'
$outputPath = [IO.Path]::GetFullPath($Output)
$start = [Diagnostics.ProcessStartInfo]::new([IO.Path]::GetFullPath($Executable))
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.WorkingDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
foreach ($argument in @('--output', $outputPath, '--scene', [IO.Path]::GetFullPath($ScenePath), '--engine-assets', [IO.Path]::GetFullPath($EngineAssets))) { $start.ArgumentList.Add($argument) }
if ($Software) { $start.ArgumentList.Add('--software') }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
$transcript = [Collections.Generic.List[object]]::new()
$request = 0
$started = $false
$errors = $null

function Read-Reply {
    $task = $process.StandardOutput.ReadLineAsync()
    if (-not $task.Wait(30000)) { throw 'Gojo viewer response timed out' }
    if ($null -eq $task.Result) { throw 'Owned viewer exited before responding' }
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
    if ($reply.id -ne $Command.id) { throw 'Viewer request identity mismatch' }
    return $reply
}
function Seek([long]$TimeMs) {
    $reply = Send-Command @{command='image_at'; time_ms=$TimeMs}
    if ($reply.state.image_playback.playing -or $reply.state.image_playback.pending_sample -or [decimal]$reply.state.image_playback.presented_sample.time_ns -ne [decimal]$TimeMs * 1000000) { throw 'Exact paused particle seek failed' }
    if ($reply.state.frame.image_layer_count -gt 4128) { throw 'Particle draw admission exceeded' }
    return $reply
}
function Capture([string]$Name) { $null = Send-Command @{command='capture'; name=$Name} }
function Metrics {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $reply = Send-Command @{command='inspect'}
        $pool = $reply.state.output_pool
        if ($null -ne $pool -and $pool.published_frames -eq $reply.state.frame.sequence) { return $reply }
        Start-Sleep -Milliseconds 10
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Completed-frame resource metrics unavailable'
}

try {
    if (-not $process.Start()) { throw 'Viewer launch failed' }
    $started = $true
    $errors = $process.StandardError.ReadToEndAsync()
    $ready = Read-Reply
    if ($ready.event -ne 'ready' -or $ready.state.pid -ne $process.Id) { throw 'Missing owned viewer readiness' }
    $report = $ready.state.image_playback.report
    if (($report.object_ids -join ',') -ne '18,14' -or ($report.particle_objects -join ',') -ne '83,88,1633') { throw 'Gojo authored image/particle order differs' }
    if ($report.package_sha256 -ne 'c6f3e39fb2071f831df076378ceed38c236d49a6c025ffaf82a848f52fb03b92') { throw 'Gojo package changed; renew scene verification' }
    if (-not $report.hdr_bloom -or @($report.omissions | Where-Object { $_ -like '*fireworkshitdistort*' }).Count) { throw 'Gojo HDR bloom or refractive ring was omitted' }
    $null = Seek 0
    $resized = Send-Command @{command='resize'; width=$Width; height=$Height}
    if ($resized.state.frame.width -ne $Width -or $resized.state.frame.height -ne $Height) { throw 'Requested output resolution was not rendered' }
    Capture 'time-00000.png'
    $checkpoints = foreach ($time in @(1500,4000,4500,4750,5000,5500,8000,12000)) {
        $reply = Seek $time
        Capture ('time-{0:D5}.png' -f $time)
        $metrics = Metrics
        @{time_ms=$time; quads=$reply.state.frame.image_layer_count; source_bytes=$metrics.state.output_pool.source_texture_bytes; uploads=$metrics.state.output_pool.texture_uploads}
    }
    if (@($checkpoints | Where-Object { $_.quads -gt 32 }).Count -eq 0) { throw 'Particle layers did not appear' }
    $null = Seek 4750
    Capture 'rewind.png'
    if ((Get-FileHash -LiteralPath (Join-Path $outputPath 'rewind.png')).Hash -ne (Get-FileHash -LiteralPath (Join-Path $outputPath 'time-04750.png')).Hash) { throw 'Particle rewind pixels differ' }
    $before = Metrics
    Start-Sleep -Milliseconds 500
    $after = Metrics
    if ($before.state.presented -ne $after.state.presented -or $before.state.image_playback.clock_ms -ne $after.state.image_playback.clock_ms) { throw 'Paused viewer continued working' }
    $null = Seek 0
    $playing = Send-Command @{command='play_images'}
    Start-Sleep -Milliseconds 8000
    $paused = Send-Command @{command='pause_images'}
    if ([decimal]$paused.state.image_playback.presented_sample.time_ns -lt 7500000000) { throw 'Live particle clock did not advance' }
    $null = Seek 4750
    $settled = Metrics
    if ($settled.state.output_pool.texture_uploads -ne $after.state.output_pool.texture_uploads -or $settled.state.output_pool.texture_uploads -ne 8) { throw 'Particle source textures were uploaded again during playback' }
    Capture 'final.png'
    $null = Send-Command @{command='stop'}
    if (-not $process.WaitForExit(15000) -or $process.ExitCode -ne 0) { throw 'Owned viewer did not exit successfully' }
    $timeline = Get-Content -LiteralPath (Join-Path $outputPath 'image-timeline.json') -Raw | ConvertFrom-Json
    $live = @($timeline.observations | Where-Object { $_.frame_id -gt $playing.state.frame.frame_id -and $_.frame_id -le $paused.state.frame.frame_id })
    if ($live.Count -lt 2) { throw 'Missing live presentation observations' }
    $gaps = for ($i=1; $i -lt $live.Count; $i++) { $live[$i].present_call_return_ms - $live[$i-1].present_call_return_ms }
    $sourceFiles = @(Get-ChildItem src,examples,tests,scripts -File -Recurse) + @(Get-Item Cargo.toml,Cargo.lock)
    $sourceHashes = @($sourceFiles | Sort-Object FullName | ForEach-Object { @{path=[IO.Path]::GetRelativePath($start.WorkingDirectory,$_.FullName); sha256=(Get-FileHash -LiteralPath $_.FullName).Hash} })
    [ordered]@{
        status='passed'; package_sha256=$report.package_sha256; executable_sha256=(Get-FileHash -LiteralPath $start.FileName).Hash;
        source_hashes=$sourceHashes; software=[bool]$Software; adapter=$ready.state.frame.adapter; resolution=@($Width,$Height);
        controlled_checkpoints=$checkpoints; rewind='identical PNG hash'; paused_idle='passed'; texture_upload_reuse='passed';
        live_interval_ms=8000; live_samples=$live.Count; maximum_presentation_call_gap_ms=($gaps | Measure-Object -Maximum).Maximum;
        omissions=$report.omissions; cleanup='Owned process exited';
        hdr_bloom=$report.hdr_bloom; refractive_ring='prepared; normal map retained';
        effect_texture_bytes=$settled.state.output_pool.effect_texture_bytes;
        limits=@('Generated WARP contracts provide independent GPU pixel checks. Gojo captures need comparison against Wallpaper Engine.', 'Presentation-call timing establishes host observations, not display cadence or GPU duration.', 'Image effects, parallax, exact engine randomness and exact bloom filtering remain unverified or unsupported.')
    } | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'report.json') -Encoding utf8
    Write-Output "Evidence: $outputPath"
} finally {
    if ($started -and -not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(10000) }
    if (Test-Path -LiteralPath $outputPath) {
        if ($errors -and $errors.IsCompleted) { $errors.Result | Set-Content -LiteralPath (Join-Path $outputPath 'stderr.log') -Encoding utf8 }
        $transcript | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'transcript.json') -Encoding utf8
    }
    $process.Dispose()
}
