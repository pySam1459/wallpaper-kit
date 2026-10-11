[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Probes,
    [Parameter(Mandatory)][string]$CaptureIdentities,
    [Parameter(Mandatory)][int]$ClientX,
    [Parameter(Mandatory)][int]$ClientY
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$probe = Get-Content -LiteralPath $Probes -Raw | ConvertFrom-Json
$captures = @(Get-Content -LiteralPath $CaptureIdentities -Raw | ConvertFrom-Json)
$required = @('identity','sized','inset','trapezoid','zero-noise','red-noise','green-noise','blue-noise','half-noise','transparent-gradient','half-gradient','gradient-x','gradient-y','mask-zero','mask-half','motion-first','motion-later','target-first','target-later')
if ($probe.schema_version -ne 1 -or @($probe.probes).Count -ne 17) { throw 'Unrecognized light-shafts probe set' }
if ($captures.Count -ne $required.Count -or @($captures.name | Sort-Object -Unique).Count -ne $required.Count) { throw 'Incomplete or duplicate captures' }
$directory = Split-Path -Parent ([IO.Path]::GetFullPath($Probes))
$bitmaps = @{}
$sourceIdentities = foreach ($item in $probe.probes) {
    $projectPath = Join-Path $directory $item.project
    $scenePath = Join-Path (Split-Path -Parent $projectPath) 'scene.json'
    $scene = Get-Content -LiteralPath $scenePath -Raw | ConvertFrom-Json
    $pass = $scene.objects[0].effects[0].passes[0]
    foreach ($property in $item.constants.PSObject.Properties) {
        if ($pass.constantshadervalues.($property.Name) -ne $property.Value) { throw "Probe constants changed: $($item.name)/$($property.Name)" }
    }
    foreach ($property in $item.combinations.PSObject.Properties) {
        if ($pass.combos.($property.Name) -ne $property.Value) { throw "Probe combinations changed: $($item.name)/$($property.Name)" }
    }
    if ($scene.objects.Count -ne 1 -or $scene.objects[0].shape -ne 'quad' -or $scene.general.orthogonalprojection.width -ne $item.canvas[0] -or $scene.general.orthogonalprojection.height -ne $item.canvas[1]) { throw "Probe geometry changed: $($item.name)" }
    Get-ChildItem -LiteralPath (Split-Path -Parent $projectPath) -File -Recurse | ForEach-Object {
        @{ path=[IO.Path]::GetRelativePath($directory,$_.FullName); sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant() }
    }
}
try {
    foreach ($name in $required) {
        $capture = @($captures | Where-Object name -eq $name)
        if ($capture.Count -ne 1 -or $capture[0].end_ms -lt $capture[0].start_ms) { throw "Missing capture or invalid interval: $name" }
        $bitmap = [Drawing.Bitmap]::new([IO.Path]::GetFullPath($capture[0].path))
        $bitmaps[$name] = $bitmap
        $width = if ($name -like 'target-*') { 512 } else { 256 }
        $height = if ($name -like 'target-*') { 288 } else { 256 }
        if ($ClientX -lt 0 -or $ClientY -lt 0 -or $ClientX+$width -gt $bitmap.Width -or $ClientY+$height -gt $bitmap.Height) { throw "Client area lies outside capture: $name" }
    }
    function Pixel([string]$Name, [int]$X, [int]$Y) {
        $bitmaps[$Name].GetPixel(($ClientX+$X),($ClientY+$Y)).R
    }
    $checks = @()
    foreach ($name in @('red-noise','transparent-gradient','half-gradient','gradient-y','mask-half')) {
        $factor = switch ($name) { 'half-gradient' { 128/255.0 }; 'gradient-y' { 127/255.0 }; 'mask-half' { [Math]::Pow(128/255.0,2) }; default { 1.0 } }
        $maximum = 0.0
        foreach ($y in @(16,32,64,96,128,160,192,224,240)) {
            foreach ($x in @(16,32,64,96,128,160,192,224,240)) {
                $maximum = [Math]::Max($maximum,[Math]::Abs((Pixel $name $x $y) - (Pixel 'identity' $x $y)*$factor))
            }
        }
        $checks += @{ name=$name; expected_factor=$factor; samples=81; max_error=$maximum; tolerance=4; passed=($maximum -le 4) }
    }
    foreach ($name in @('zero-noise','green-noise','blue-noise','mask-zero')) {
        $maximum = 0
        for ($y=8; $y -lt 248; $y+=4) {
            for ($x=8; $x -lt 248; $x+=4) { $maximum=[Math]::Max($maximum,(Pixel $name $x $y)) }
        }
        $checks += @{ name=$name; expected='dark'; samples=3600; max_error=$maximum; tolerance=4; passed=($maximum -le 4) }
    }
    $maximum = 0; $outside = 0; $visible = 0
    for ($y=8; $y -lt 248; $y+=4) {
        for ($x=8; $x -lt 248; $x+=4) {
            $value = Pixel 'inset' $x $y
            $maximum=[Math]::Max($maximum,[Math]::Abs($value - (Pixel 'sized' $x $y)))
            if ($x -lt 60 -or $x -gt 195 -or $y -lt 60 -or $y -gt 195) { $outside=[Math]::Max($outside,$value) }
            if ($value -gt 32) { $visible++ }
        }
    }
    $checks += @{ name='size-and-perspective'; samples=3600; max_error=$maximum; outside_inset_maximum=$outside; visible_samples=$visible; tolerance=4; passed=($maximum -le 4 -and $outside -le 4 -and $visible -ge 32) }
    $baseline = Pixel 'identity' 240 16
    $halfNoise = Pixel 'half-noise' 240 16
    $checks += @{ name='nonlinear-noise-control'; baseline=$baseline; half_noise=$halfNoise; expected='visible baseline and half noise much darker than linear scaling'; passed=($baseline -ge 100 -and $halfNoise -le 16) }
    $temporal = foreach ($kind in @('motion','target')) {
        $first = $captures | Where-Object name -eq "$kind-first"
        $later = $captures | Where-Object name -eq "$kind-later"
        $elapsed = ($later.start_ms+$later.end_ms-$first.start_ms-$first.end_ms)/2000.0
        if ($elapsed -lt 3 -or $elapsed -gt 60) { throw 'Temporal captures must be three to sixty seconds apart' }
        $width = if ($kind -eq 'target') { 512 } else { 256 }
        $height = if ($kind -eq 'target') { 288 } else { 256 }
        $changed=0; $maximum=0
        for ($y=8; $y -lt $height-8; $y++) {
            for ($x=8; $x -lt $width-8; $x++) {
                $a=$bitmaps["$kind-first"].GetPixel(($ClientX+$x),($ClientY+$y))
                $b=$bitmaps["$kind-later"].GetPixel(($ClientX+$x),($ClientY+$y))
                $difference=[Math]::Max([Math]::Abs([int]$a.R-$b.R),[Math]::Max([Math]::Abs([int]$a.G-$b.G),[Math]::Abs([int]$a.B-$b.B)))
                $maximum=[Math]::Max($maximum,$difference)
                if ($difference -gt 6) { $changed++ }
            }
        }
        @{ name=$kind; elapsed_seconds=$elapsed; changed_pixels_above_six_codes=$changed; max_change=$maximum; status=$(if ($changed -ge 64) { 'visible change' } else { 'inconclusive' }) }
    }
    $targetMotion = $temporal | Where-Object name -eq 'target'
    $checks += @{ name='target-motion-control'; changed_pixels_above_six_codes=$targetMotion.changed_pixels_above_six_codes; passed=($targetMotion.changed_pixels_above_six_codes -ge 64) }
    $samples = foreach ($name in $required) {
        $values = foreach ($y in @(16,64,128,192,240)) {
            foreach ($x in @(16,64,128,192,240)) { @{ x=$x; y=$y; red=(Pixel $name $x $y) } }
        }
        @{ name=$name; values=@($values) }
    }
    $report = @{
        schema_version=1; status=$(if (@($checks | Where-Object { -not $_.passed }).Count) { 'failed' } else { 'passed reference relations' })
        client_origin=@($ClientX,$ClientY); probes_sha256=(Get-FileHash -LiteralPath $Probes).Hash.ToLowerInvariant()
        capture_identities_sha256=(Get-FileHash -LiteralPath $CaptureIdentities).Hash.ToLowerInvariant()
        captures=@($captures | ForEach-Object { @{ name=$_.name; path=$_.path; sha256=(Get-FileHash -LiteralPath $_.path).Hash.ToLowerInvariant() } })
        generated_inputs=@($sourceIdentities); checks=$checks; temporal=@($temporal); samples=@($samples)
        limits='Approximate JPEG relations, not a complete independent ray oracle. Motion checks only changed pixels; speed, phase and smooth playback are unverified. Inconclusive probes remain visible.'
    }
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $directory 'comparison.json') -Encoding utf8NoBOM
    $report.status
    $temporal | Select-Object name,status,changed_pixels_above_six_codes,max_change,elapsed_seconds | ConvertTo-Json
    if ($report.status -eq 'failed') { throw 'Light-shafts relations or target motion control failed' }
} finally { foreach ($bitmap in $bitmaps.Values) { $bitmap.Dispose() } }
