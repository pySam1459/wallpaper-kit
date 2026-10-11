[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Output,
    [switch]$Software,
    [string]$ScenePath,
    [string]$EngineAssets
)

$ErrorActionPreference = 'Stop'
if ([bool]$ScenePath -ne [bool]$EngineAssets) { throw 'ScenePath and EngineAssets must be supplied together' }
$outputPath = [IO.Path]::GetFullPath($Output)
$start = [Diagnostics.ProcessStartInfo]::new([IO.Path]::GetFullPath($Executable))
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.WorkingDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$start.ArgumentList.Add('--output')
$start.ArgumentList.Add($outputPath)
if ($Software) { $start.ArgumentList.Add('--software') }
if ($ScenePath) {
    foreach ($argument in @('--scene', $ScenePath, '--engine-assets', $EngineAssets)) { $start.ArgumentList.Add($argument) }
} else { $start.ArgumentList.Add('--image-fixture') }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
$transcript = [Collections.Generic.List[object]]::new()
$started = $false
$errors = $null
$requestId = 0

function Read-Reply {
    $task = $process.StandardOutput.ReadLineAsync()
    if (-not $task.Wait(30000)) { throw 'Image viewer response timed out' }
    if ($null -eq $task.Result) { throw 'Image viewer exited before responding' }
    $reply = $task.Result | ConvertFrom-Json
    $transcript.Add($reply)
    if ($reply.event -eq 'failed' -or $reply.ok -eq $false) { throw "Image viewer error: $($task.Result)" }
    return $reply
}
function Send-Command([hashtable]$Command) {
    $script:requestId += 1
    $Command.id = $script:requestId
    $process.StandardInput.WriteLine(($Command | ConvertTo-Json -Compress))
    $process.StandardInput.Flush()
    $reply = Read-Reply
    if ($reply.id -ne $Command.id) { throw 'Unexpected image viewer request identity' }
    return $reply
}
function Check-Selection($Reply, [long]$TimeMs) {
    $sample = $Reply.state.image_playback.presented_sample
    if ([decimal]$sample.time_ns -ne [decimal]$TimeMs * 1000000) { throw 'Image seek replied before the requested time was presented' }
    if ($Reply.state.continuous_rendering -or $Reply.state.image_playback.pending_sample) { throw 'Controlled image sample is not settled and paused' }
    $expected = if ($ScenePath) {
        # Target assets have 480 frames of authored f32 0.06 seconds each.
        $index = [int][Math]::Floor(($TimeMs % 28799.99952) / 59.999999)
        @($index, $index)
    } else { @([int](($TimeMs % 375) -ge 125), [int](($TimeMs % 500) -ge 250)) }
    if (($sample.frame_indices -join ',') -ne ($expected -join ',')) { throw 'Common-time sprite indices differ' }
}
function Capture([string]$Name) {
    $reply = Send-Command @{command='capture'; name=$Name}
    if (-not $ScenePath -and $reply.layers_check -ne $true) { throw 'Combined image fixture pixels differ' }
    return $reply
}
function Changed-Pixels([string]$First, [string]$Later) {
    Add-Type -AssemblyName System.Drawing
    $a = [Drawing.Bitmap]::new((Join-Path $outputPath $First))
    $b = [Drawing.Bitmap]::new((Join-Path $outputPath $Later))
    try {
        if ($a.Width -ne $b.Width -or $a.Height -ne $b.Height) { throw 'Cloud comparison dimensions differ' }
        $changed = 0
        for ($y = 0; $y -lt $a.Height; $y++) {
            for ($x = 0; $x -lt $a.Width; $x++) {
                $firstColor = $a.GetPixel($x,$y); $laterColor = $b.GetPixel($x,$y)
                if ($firstColor.ToArgb() -ne $laterColor.ToArgb()) { $changed += 1 }
            }
        }
        return $changed
    } finally { $a.Dispose(); $b.Dispose() }
}

try {
    if (-not $process.Start()) { throw 'Image viewer launch failed' }
    $started = $true
    $errors = $process.StandardError.ReadToEndAsync()
    $ready = Read-Reply
    $expectedLayers = if ($ScenePath) { 7 } else { 4 }
    if ($ready.event -ne 'ready' -or $ready.state.pid -ne $process.Id -or $ready.state.frame.image_layer_count -ne $expectedLayers) {
        throw 'Image readiness must identify a completed composition in the owned process'
    }
    if (@($ready.state.image_playback.report.animated_assets).Count -ne 2 -or @($ready.state.image_playback.report.omissions).Count -eq 0) {
        throw 'Playback source identity or explicit omissions missing'
    }
    if ($ScenePath) {
        if (($ready.state.image_playback.report.object_ids -join ',') -ne '28,32,22,72,89,77,86') { throw 'Target authored image order differs' }
        if (@($ready.state.image_playback.report.omissions | Where-Object { $_ -like 'Object 22*not selected*' }).Count -ne 0) { throw 'Target wire layer was omitted' }
        $blurs = $ready.state.image_playback.report.precise_blurs
        if (@($blurs.PSObject.Properties).Count -ne 2 -or @($blurs.'22').Count -ne 2 -or @($blurs.'72').Count -ne 2) { throw 'Target precise-blur dispatch differs' }
        foreach ($axis in 0..1) {
            if ([Math]::Abs($blurs.'22'[$axis] - 0.32) -gt 0.0000001 -or $blurs.'72'[$axis] -ne 2) { throw 'Target authored precise-blur scales differ' }
        }
        if (@($ready.state.image_playback.report.omissions | Where-Object { $_ -like '*blurprecise*' }).Count) { throw 'Target precise blur was omitted' }
        $clouds = $ready.state.image_playback.report.cloud_motions
        if (@($clouds.PSObject.Properties).Count -ne 2 -or $null -eq $clouds.'28' -or $null -eq $clouds.'32') { throw 'Target cloud-motion dispatch differs' }
        foreach ($entry in @(@('28',0.01,0.6,0.013),@('32',0.02,0.5,0.025))) {
            $cloud = $clouds.($entry[0])
            $settings = $cloud.settings
            if ([Math]::Abs($settings.amount - $entry[1]) -gt 0.0000001 -or [Math]::Abs($settings.direction_radians - 1.5707964) -gt 0.0000001 -or @($settings.granularity).Count -ne 2 -or [Math]::Abs($settings.granularity[0] - $entry[2]) -gt 0.0000001 -or [Math]::Abs($settings.granularity[1] - 0.1) -gt 0.0000001 -or [Math]::Abs($settings.speed - $entry[3]) -gt 0.0000001 -or $cloud.noise_asset -ne 'materials/util/perlin_256.tex') { throw 'Target authored cloud settings or noise differ' }
        }
        if ($null -ne $clouds.'28'.mask_asset -or $clouds.'32'.mask_asset -ne 'materials/masks/cloudmotion_mask_ce1abe94.tex') { throw 'Target cloud masks differ' }
        if (@($ready.state.image_playback.report.omissions | Where-Object { $_ -like '*cloudmotion*' }).Count) { throw 'Target cloud motion was omitted' }
    }
    $times = if ($ScenePath) { @(0, 125, 960, 1080, 1500, 28799, 28800, 29000) } else { @(0, 124, 125, 249, 250, 374, 375, 500, 1500) }
    foreach ($time in $times) {
        $selected = Send-Command @{command='image_at'; time_ms=$time}
        Check-Selection $selected $time
        $null = Capture ('image-{0:D5}.png' -f $time)
    }
    $cloudChangedPixels = $null
    if ($ScenePath) {
        # Both sprites select frame zero at 0 and 28800 ms. With the other
        # current layers static, only the two cloud effects can change pixels.
        $cloudChangedPixels = Changed-Pixels 'image-00000.png' 'image-28800.png'
        if ($cloudChangedPixels -lt 64) { throw 'Target cloud motion remained frozen across a sprite loop' }
        if ((Changed-Pixels 'image-00000.png' 'image-00000.png') -ne 0) { throw 'Cloud difference check accepts identical pixels' }
        $rewound = Send-Command @{command='image_at'; time_ms=0}
        Check-Selection $rewound 0
        $null = Capture 'cloud-rewind.png'
        if ((Changed-Pixels 'image-00000.png' 'cloud-rewind.png') -ne 0) { throw 'Cloud seek did not restore time-zero pixels' }
        $restored = Send-Command @{command='image_at'; time_ms=$times[-1]}
        Check-Selection $restored $times[-1]
    }
    $before = Send-Command @{command='inspect'}
    # Defined paused interval after an acknowledged completed seek.
    Start-Sleep -Milliseconds 500
    $after = Send-Command @{command='inspect'}
    if ($before.state.frame.frame_id -ne $after.state.frame.frame_id -or $before.state.presented -ne $after.state.presented) { throw 'Paused image viewer performed unsolicited presentation' }
    $resized = Send-Command @{command='resize'; width=317; height=193}
    if ($resized.state.frame.width -ne 317 -or $resized.state.frame.height -ne 193) { throw 'Image resize dimensions differ' }
    Check-Selection $resized $times[-1]
    $null = Capture 'resized.png'
    $null = Send-Command @{command='image_at'; time_ms=0}
    $playing = Send-Command @{command='play_images'}
    # Live interval has no capture/readback. Fixture covers two combined loops;
    # the actual target crosses several sprite page boundaries, not two loops.
    Start-Sleep -Milliseconds 4200
    $paused = Send-Command @{command='freeze_images'}
    $clockNs = [decimal]$paused.state.image_playback.presented_sample.time_ns
    if ($clockNs -lt 4000000000) { throw 'Live image playback did not advance for the observed interval' }
    if (-not $paused.state.image_playback.frozen -or $paused.state.continuous_rendering) { throw 'Freeze did not stop scheduling' }
    $null = Capture 'frozen-before.png'
    Start-Sleep -Milliseconds 500
    $held = Send-Command @{command='inspect'}
    if ($held.state.frame.frame_id -ne $paused.state.frame.frame_id -or
        $held.state.image_playback.presented_sample.time_ns -ne $paused.state.image_playback.presented_sample.time_ns -or
        $held.state.image_playback.clock_ms -ne $paused.state.image_playback.clock_ms) { throw 'Freeze changed the last presented sample or clock' }
    $null = Capture 'frozen-after.png'
    if ((Get-FileHash -LiteralPath (Join-Path $outputPath 'frozen-before.png')).Hash -ne
        (Get-FileHash -LiteralPath (Join-Path $outputPath 'frozen-after.png')).Hash) { throw 'Frozen pixels changed' }
    $frozenResize = Send-Command @{command='resize'; width=320; height=180}
    if ($frozenResize.state.image_playback.presented_sample.time_ns -ne $paused.state.image_playback.presented_sample.time_ns) { throw 'Frozen resize used newer content' }
    $null = Capture 'frozen-resized.png'
    $resumed = Send-Command @{command='play_images'}
    if (-not $resumed.state.continuous_rendering -or $resumed.state.image_playback.frozen) { throw 'Frozen playback did not resume' }
    $null = Send-Command @{command='image_at'; time_ms=1500}
    $null = Capture 'paused.png'
    $null = Send-Command @{command='stop'}
    if (-not $process.WaitForExit(15000)) { throw 'Image viewer did not shut down' }
    if ($process.ExitCode -ne 0) { throw "Image viewer exit code $($process.ExitCode)" }
    $timeline = Get-Content -LiteralPath (Join-Path $outputPath 'image-timeline.json') -Raw | ConvertFrom-Json
    $live = @($timeline.observations | Where-Object { $_.frame_id -gt $playing.state.frame.frame_id -and $_.frame_id -le $paused.state.frame.frame_id })
    if ($live.Count -lt 2) { throw 'Missing live image presentation observations' }
    [ordered]@{status='passed'; pid=$process.Id; scene=[bool]$ScenePath; image_layers=$expectedLayers;
        controlled_indices='passed'; pixel_oracle=$(if ($ScenePath) {'not available'} else {'passed'}); cloud_changed_pixels=$cloudChangedPixels;
        paused_idle='passed'; resize='passed'; freeze_last_frame='passed'; frozen_resize='passed'; freeze_resume='passed'; live_interval_ms=4200; live_samples=$live.Count; latest_live_time_ns=$clockNs.ToString();
        actions=$transcript; cleanup='Owned process exited';
        limits=@('Selected images only. Omissions are recorded in image-playback.json.', 'Presentation-call return times do not establish display cadence or GPU duration.', 'Target captures need comparison against Wallpaper Engine. Host impact is unmeasured.');
    } | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'report.json') -Encoding utf8
} finally {
    if ($started -and -not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(10000) }
    if (Test-Path -LiteralPath $outputPath) {
        if ($errors -and $errors.IsCompleted) { $errors.Result | Set-Content -LiteralPath (Join-Path $outputPath 'stderr.log') -Encoding utf8 }
        $transcript | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'transcript.json') -Encoding utf8
    }
    $process.Dispose()
}
