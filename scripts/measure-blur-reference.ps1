[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Probes,
    [Parameter(Mandatory)][string]$Capture,
    [Parameter(Mandatory)][int]$ClientX,
    [Parameter(Mandatory)][int]$ClientY,
    [Parameter(Mandatory)][ValidateRange(1, 16)][int]$PixelScale
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$probe = Get-Content -LiteralPath $Probes -Raw | ConvertFrom-Json
if ($probe.schema_version -ne 1 -or ($probe.canvas -join ',') -ne '195,195') { throw 'Unrecognized probe layout' }
$defaultProbes = @($probe.probes | Where-Object kernel -eq 0 | Sort-Object object_id)
if ($defaultProbes.Count -ne 3 -or $probe.hidden_color -isnot [bool]) { throw 'Expected three default-kernel probes and Boolean hidden_color' }
foreach ($index in 0..2) {
    $item = $defaultProbes[$index]
    if ($item.object_id -ne $index + 1 -or $item.scale -ne @(0.32, 1.0, 2.0)[$index] -or ($item.extent -join ',') -ne '65,65' -or ($item.origin -join ',') -ne "$($index * 65),0") { throw 'Default-kernel scales or placement differ from generated probes' }
}
$bitmap = [Drawing.Bitmap]::new([IO.Path]::GetFullPath($Capture))
try {
    if ($ClientX -lt 0 -or $ClientY -lt 0 -or $ClientX + 195 * $PixelScale -gt $bitmap.Width -or $ClientY + 195 * $PixelScale -gt $bitmap.Height) { throw 'Client area is outside capture' }
    $measurements = foreach ($item in $defaultProbes) {
        $values = foreach ($dx in -12..12) {
            $x = $ClientX + ([int]$item.origin[0] + 32 + $dx) * $PixelScale + [int][Math]::Floor($PixelScale / 2)
            # Scene Y increases upward. Keep sample rows far from layer edges.
            $y = $ClientY + (195 - [int]$item.origin[1] - 33) * $PixelScale
            $sum = 0
            foreach ($dy in 0..19) { $sum += [int]$bitmap.GetPixel($x, $y + $dy).R }
            $observed = $sum / 20
            # Independently evaluate the licensed kernel's impulse response.
            $expected = if ($dx -eq 0) { 0.1964825501511404 } else { 0.0 }
            foreach ($pair in @(@(1.411764705882353, 0.2969069646728344), @(3.2941176470588234, 0.09447039785044732), @(5.176470588235294, 0.010381362401148057))) {
                foreach ($sign in @(-1, 1)) {
                    $distance = [double]($dx + $sign * $pair[0] * $item.scale)
                    $expected += $pair[1] * [Math]::Max(0.0, 1.0 - [Math]::Abs($distance))
                }
            }
            $expected = [Math]::Round($expected * 255)
            if ($probe.hidden_color) { $expected *= 1 - $expected / 255 }
            [ordered]@{ offset = $dx; observed = $observed; expected = $expected; absolute_error = [Math]::Abs($observed - $expected) }
        }
        $maximum = ($values.absolute_error | Measure-Object -Maximum).Maximum
        [ordered]@{ kernel = 0; scale = $item.scale; samples = @($values); max_error = $maximum; within_four_codes = $maximum -le 4 }
    }
    $report = [ordered]@{
        schema_version = 1; status = $(if (@($measurements | Where-Object { -not $_.within_four_codes }).Count) { 'failed' } else { 'passed approximate reference' });
        capture = [IO.Path]::GetFullPath($Capture); capture_sha256 = (Get-FileHash -LiteralPath $Capture).Hash.ToLowerInvariant();
        probes_sha256 = (Get-FileHash -LiteralPath $Probes).Hash.ToLowerInvariant();
        client_origin = @($ClientX, $ClientY); pixel_scale = $PixelScale; hidden_color = $probe.hidden_color;
        measurements = @($measurements);
        limits = 'Static default-kernel impulse comparison only. Desktop JPEG and final linear scaling limit precision; this is not a lossless pixel receipt or full scene fidelity.'
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path (Split-Path -Parent ([IO.Path]::GetFullPath($Probes))) 'comparison.json') -Encoding utf8NoBOM
    $report.status
    if ($report.status -eq 'failed') { throw 'Default kernel differs beyond four code values' }
} finally { $bitmap.Dispose() }
