[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Probes,
    [Parameter(Mandatory)][string]$Capture,
    [string]$LaterCapture,
    [string]$CaptureIdentities,
    [Parameter(Mandatory)][int]$ClientX,
    [Parameter(Mandatory)][int]$ClientY
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$probe = Get-Content -LiteralPath $Probes -Raw | ConvertFrom-Json
if ($probe.schema_version -ne 1 -or ($probe.canvas -join ',') -ne '512,768' -or $probe.extent -ne 128 -or $probe.mode -notin @('constant','mask','ramp','motion')) { throw 'Unrecognized cloud probe layout' }
$count = if ($probe.mode -eq 'mask') { 20 } else { 24 }
$items = @($probe.probes | Sort-Object id)
if ($items.Count -ne $count) { throw 'Incomplete cloud probes' }
foreach ($i in 0..($count - 1)) {
    $item = $items[$i]
    $axis = if ($probe.mode -eq 'mask') { @('y','step')[$i % 2] } else { @('x','y')[$i % 2] }
    if ($item.id -ne $i + 1 -or $item.case -ne [Math]::Floor($i / 2) -or $item.axis -ne $axis -or ($item.origin -join ',') -ne "$($i % 4 * 128),$([int][Math]::Floor($i / 4) * 128)" -or $item.amount -ne 0.1) { throw 'Generated cloud placement or amount differs' }
    if ($probe.mode -eq 'mask' -and $item.mask_value -ne @(0,64,128,192,255)[[int]$item.case % 5]) { throw 'Mask probe values differ' }
    if ($probe.mode -in @('ramp','motion') -and ($item.noise_axis -notin @('x','y') -or @($item.grain).Count -ne 2 -or $item.speed -notin @(0,0.02))) { throw 'Ramp probe settings differ' }
    if ($probe.mode -eq 'motion' -and $item.speed -ne 0.02) { throw 'Motion probes must advance at 0.02 periods/second' }
}
$paths = @($Capture)
$elapsed = 0.0
if ($probe.mode -eq 'motion') {
    if (-not $LaterCapture -or -not $CaptureIdentities) { throw 'Timed cloud probes require both captures and their observation intervals' }
    $paths += $LaterCapture
    $intervals = @(Get-Content -LiteralPath $CaptureIdentities -Raw | ConvertFrom-Json)
    if ($intervals.Count -ne 2 -or $intervals[0].end_ms -lt $intervals[0].start_ms -or $intervals[1].end_ms -lt $intervals[1].start_ms) { throw 'Unrecognized capture observation intervals' }
    $elapsed = ($intervals[1].start_ms + $intervals[1].end_ms - $intervals[0].start_ms - $intervals[0].end_ms) / 2000.0
    if ($elapsed -lt 7 -or $elapsed -gt 20) { throw 'Timed captures must be seven to twenty seconds apart' }
    if ((Get-FileHash -LiteralPath $Capture).Hash -eq (Get-FileHash -LiteralPath $LaterCapture).Hash) { throw 'Timed captures must differ' }
}
$bitmaps = @()
try {
    foreach ($path in $paths) {
        $bitmap = [Drawing.Bitmap]::new([IO.Path]::GetFullPath($path)); $bitmaps += $bitmap
        if ($ClientX -lt 0 -or $ClientY -lt 0 -or $ClientX + 512 -gt $bitmap.Width -or $ClientY + 768 -gt $bitmap.Height) { throw 'Client area lies outside capture' }
    }
    function Observed($Item, [int]$X, [int]$Y, [int]$Index) {
        $bitmaps[$Index].GetPixel(($ClientX + $Item.origin[0] + $X),($ClientY + 768 - $Item.origin[1] - 128 + $Y)).R
    }
    # A ramp texture's red sample, including bilinear repeat across its seam.
    function Ramp([double]$Coordinate) {
        $position = $Coordinate * 128 - 0.5
        $base = [Math]::Floor($position); $weight = $position - $base
        $first = (($base % 128) + 128) % 128; $next = (($base + 1) % 128 + 128) % 128
        (2 * $first * (1 - $weight) + 2 * $next * $weight) / 255.0
    }
    $phase = 0.0
    if ($probe.mode -eq 'motion') {
        # Global engine time is unknown. Fit only the initial phase from three
        # first-case samples, then predict the second capture from elapsed time.
        $estimates = foreach ($x in @(32,64,96)) {
            $difference = (Observed $items[1] $x 64 0) - 128
            $coordinate = (255 * ($difference / 25.6 + 1) / 2 + 1) / 256
            $value = $coordinate - ($x + 0.5) / 128
            (($value % 1) + 1) % 1
        }
        $phase = @($estimates | Sort-Object)[1]
    }
    $samples = @()
    $frozenDetected = $false
    foreach ($index in 0..($bitmaps.Count - 1)) {
        foreach ($item in $items) {
            if ($probe.mode -eq 'ramp' -and $item.speed -ne 0) { continue }
            $points = if ($item.axis -eq 'step') { @(48..67 | ForEach-Object { ,@(64,$_) }) } else { @(@(32,32),@(64,64),@(96,96),@(32,96),@(96,32)) }
            foreach ($point in $points) {
                $x = $point[0]; $y = $point[1]
                $u = ($x + 0.5) / 128; $v = ($y + 0.5) / 128
                $noise = if ($probe.mode -in @('ramp','motion')) {
                    $coordinate = if ($item.noise_axis -eq 'x') { $u * $item.grain[0] * $item.grain[1] + $phase + $elapsed * $index * $item.speed } else { $v * $item.grain[0] }
                    Ramp $coordinate
                } else { $item.noise[0] / 255.0 }
                $mask = if ($probe.mode -eq 'mask') { $item.mask_value / 255.0 } elseif ($item.mask) { @(0,128,255)[[int]$item.case - 9] / 255.0 } else { 1.0 }
                $delta = ($noise * 2 - 1) * $mask * $mask * 0.1 * 128
                $direction = if ($item.axis -eq 'x') { -[Math]::Sin($item.direction) } else { [Math]::Cos($item.direction) }
                $position = $(if ($item.axis -eq 'x') { $x } else { $y }) + $direction * $delta
                $expected = if ($item.axis -eq 'step') { [Math]::Clamp($position - 63.0, 0.0, 1.0) * 255 } else { [Math]::Clamp($position,0.0,127.0) * 2 }
                $expected = [Math]::Round($expected)
                $observed = Observed $item $x $y $index
                $pixelError = [Math]::Abs($observed - $expected)
                # At a 255-code step, tiny positional differences dominate code
                # error. Measure its transition in source pixels separately.
                $tolerance = if ($item.axis -eq 'step') { 255 * 0.05 } else { 6 }
                if ($probe.mode -eq 'motion' -and $index -eq 1 -and [Math]::Abs((Observed $item $x $y 0) - $expected) -gt $tolerance) { $frozenDetected = $true }
                $samples += [ordered]@{ capture=$index; id=$item.id; x=$x; y=$y; observed=$observed; expected=$expected; absolute_error=$pixelError; tolerance=$tolerance; passed=($pixelError -le $tolerance) }
            }
        }
    }
    $expectedCount = switch ($probe.mode) { 'mask' { 250 }; 'ramp' { 100 }; 'motion' { 240 }; 'constant' { 120 } }
    if ($samples.Count -ne $expectedCount) { throw 'Incomplete cloud measurements' }
    $maximum = ($samples.absolute_error | Measure-Object -Maximum).Maximum
    $failed = @($samples | Where-Object { -not $_.passed }).Count
    if ($probe.mode -eq 'motion' -and -not $frozenDetected) { throw 'Timed probes cannot distinguish frozen motion' }
    $stepMaximum = ($samples | Where-Object { $_.tolerance -gt 6 } | Measure-Object absolute_error -Maximum).Maximum
    $report = [ordered]@{
        schema_version=1; status=$(if ($failed -eq 0) { 'passed approximate reference' } else { 'failed' }); mode=$probe.mode;
        captures=@($paths | ForEach-Object { @{ path=[IO.Path]::GetFullPath($_); sha256=(Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant() } });
        probes_sha256=(Get-FileHash -LiteralPath $Probes).Hash.ToLowerInvariant(); client_origin=@($ClientX,$ClientY);
        inferred_initial_phase=$phase; elapsed_seconds=$elapsed; frozen_motion_detected=$frozenDetected; max_error=$maximum; max_step_position_error=$(if ($null -eq $stepMaximum) { $null } else { $stepMaximum / 255.0 }); samples=$samples;
        limits='Generated gradient/step comparison only. Gradient tolerance is six codes; sharp-step tolerance is 0.05 source pixel. JPEG, filtering and quantization limit fidelity; step residuals do not establish an exact filtering match. Initial engine time is unknown; timed evidence checks relative phase progression, not exact time-zero pixels. Full scene fidelity, arbitrary chains, mip selection and host impact remain unverified.'
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path (Split-Path -Parent ([IO.Path]::GetFullPath($Probes))) 'comparison.json') -Encoding utf8NoBOM
    $report.status
    if ($failed -ne 0) { throw "Cloud probes outside gradient/step tolerances: $failed" }
} finally { foreach ($bitmap in $bitmaps) { $bitmap.Dispose() } }
