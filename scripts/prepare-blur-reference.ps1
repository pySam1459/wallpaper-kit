[CmdletBinding()]
param([Parameter(Mandatory)][string]$EngineAssets, [switch]$HiddenColor)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$engine = (Resolve-Path -LiteralPath $EngineAssets).Path
$output = Join-Path $root ('artifacts/blur-reference-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $output

function Write-Json([string]$Path, $Value) {
    $parent = Split-Path -Parent $Path
    $null = New-Item -ItemType Directory -Path $parent -Force
    $Value | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}

# This directory is an ignored, local input to Wallpaper Engine, never shader
# source for our renderer. Preserve the engine files and record their hashes.
$vendorFiles = @(
    'effects/blurprecise/effect.json',
    'effects/blurprecise/materials/effects/blur_precise_gaussian_x.json',
    'effects/blurprecise/materials/effects/blur_precise_gaussian_y.json',
    'effects/blurprecise/shaders/effects/blur_precise_gaussian.frag',
    'effects/blurprecise/shaders/effects/blur_precise_gaussian.vert'
    'shaders/common_blur.h'
)
$identities = foreach ($relative in $vendorFiles) {
    $source = Join-Path $engine $relative
    $destination = if ($relative -eq 'effects/blurprecise/effect.json') {
        Join-Path $output $relative
    } else { Join-Path $output ($relative -replace '^effects/blurprecise/', '') }
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force
    Copy-Item -LiteralPath $source -Destination $destination
    @{ path = $relative; sha256 = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() }
}

# Opaque grayscale vertical impulses measure one axis without the other axis
# attenuating it. Grayscale avoids JPEG chroma subsampling in desktop captures.
$extent = 65
$pixels = [byte[]]::new($extent * $extent * 4)
for ($y = 0; $y -lt $extent; $y++) {
    for ($x = 0; $x -lt $extent; $x++) {
        $offset = ($y * $extent + $x) * 4
        if ($x -eq 32) { $pixels[$offset] = 255; $pixels[$offset + 1] = 255; $pixels[$offset + 2] = 255 }
        $pixels[$offset + 3] = 255
        if ($HiddenColor -and $x -eq 32) { $pixels[$offset + 3] = 0 }
    }
}
$null = New-Item -ItemType Directory -Path (Join-Path $output 'materials') -Force
$writer = [IO.BinaryWriter]::new([IO.File]::Create((Join-Path $output 'materials/impulse.tex')))
try {
    $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXV0005`0TEXI0001`0"))
    foreach ($value in @(0, 0, $extent, $extent, $extent, $extent, 0)) { $writer.Write([uint32]$value) }
    $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXB0004`0"))
    foreach ($value in @(1, [uint32]::MaxValue, 0, 1, $extent, $extent, 0, $pixels.Length, $pixels.Length)) { $writer.Write([uint32]$value) }
    $writer.Write($pixels)
} finally { $writer.Dispose() }
Write-Json (Join-Path $output 'models/impulse.json') @{ autosize = $true; material = 'materials/impulse.json' }
Write-Json (Join-Path $output 'materials/impulse.json') @{ passes = @(@{ shader = 'genericimage4'; textures = @('impulse'); blending = 'translucent'; cullmode = 'nocull'; depthtest = 'disabled'; depthwrite = 'disabled' }) }

$objects = @()
$probes = @()
foreach ($kernel in @(0, 1, 2)) {
    foreach ($scale in @('0.32', '1', '2')) {
        $index = $objects.Count
        $x = ($index % 3) * $extent
        $y = [int][Math]::Floor($index / 3) * $extent
        $objects += @{ id = $index + 1; name = "kernel=$kernel scale=$scale"; image = 'models/impulse.json'; origin = "$($x + 32.5) $($y + 32.5) 0"; size = '65 65'; effects = @(@{ file = 'effects/blurprecise/effect.json'; visible = $true; passes = @(
            @{ combos = @{ KERNEL = $kernel }; constantshadervalues = @{ scale = "$scale $scale" } },
            @{ combos = @{ KERNEL = $kernel; VERTICAL = 1; ENABLEMASK = 1 }; constantshadervalues = @{ scale = "$scale $scale" } }
        ) }) }
        $probes += @{ object_id = $index + 1; kernel = $kernel; scale = [double]::Parse($scale, [Globalization.CultureInfo]::InvariantCulture); origin = @($x, $y); extent = @($extent, $extent) }
    }
}
Write-Json (Join-Path $output 'scene.json') @{ version = 5; general = @{ orthogonalprojection = @{ width = 195; height = 195 }; ambientcolor = '1 1 1'; clearcolor = '0 0 0'; clearenabled = $true; nearz = 0.01; farz = 10000; zoom = 1; bloom = $false; camerafade = $false; cameraparallax = $false }; camera = @{ center = '0 0 -1'; eye = '0 0 0'; up = '0 1 0' }; objects = $objects }
Write-Json (Join-Path $output 'project.json') @{ type = 'scene'; file = 'scene.json'; title = 'Generated precise-blur reference'; description = 'Local generated impulse probes'; general = @{ properties = @{} } }
Write-Json (Join-Path $output 'probes.json') @{ schema_version = 1; engine_assets = $engine; vendor_inputs = @($identities); canvas = @(195, 195); hidden_color = [bool]$HiddenColor; probes = $probes; notes = 'Generated impulses. Native output should be an integer multiple of 195x195. Desktop JPEG captures are approximate evidence, never exact pixel receipts.' }
$output
