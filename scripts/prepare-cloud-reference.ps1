[CmdletBinding()]
param([Parameter(Mandatory)][string]$EngineAssets, [ValidateSet('constant', 'mask', 'ramp', 'motion')][string]$Mode = 'constant')

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$engine = (Resolve-Path -LiteralPath $EngineAssets).Path
$output = Join-Path $root ('artifacts/cloud-reference-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $output
function Write-Json([string]$Path, $Value) {
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force
    $Value | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}
function Write-Tex([string]$Name, [int]$Width, [int]$Height, [byte[]]$Pixels, [switch]$R8) {
    if ($Pixels.Length -ne $Width * $Height * $(if ($R8) { 1 } else { 4 })) { throw 'Generated texture extent differs' }
    $path = Join-Path $output "materials/$Name.tex"
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force
    $writer = [IO.BinaryWriter]::new([IO.File]::Create($path))
    try {
        $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXV0005`0TEXI0001`0"))
        foreach ($value in @($(if ($R8) { 9 } else { 0 }), 0, $Width, $Height, $Width, $Height, 0)) { $writer.Write([uint32]$value) }
        $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXB0004`0"))
        foreach ($value in @(1, [uint32]::MaxValue, 0, 1, $Width, $Height, 0, $Pixels.Length, $Pixels.Length)) { $writer.Write([uint32]$value) }
        $writer.Write($Pixels)
    } finally { $writer.Dispose() }
}

# Local reference inputs for the installed engine, never renderer shader source.
$identities = foreach ($relative in @('effect.json', 'materials/effects/cloudmotion.json', 'shaders/effects/cloudmotion.vert', 'shaders/effects/cloudmotion.frag')) {
    $source = Join-Path $engine "effects/cloudmotion/$relative"
    $destination = Join-Path $output $(if ($relative -eq 'effect.json') { 'effects/cloudmotion/effect.json' } else { $relative })
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force
    Copy-Item -LiteralPath $source -Destination $destination
    @{ path = "effects/cloudmotion/$relative"; sha256 = (Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant() }
}
$common = Join-Path $engine 'shaders/common.h'
Copy-Item -LiteralPath $common -Destination (Join-Path $output 'shaders/common.h')
$identities += @{ path='shaders/common.h'; sha256=(Get-FileHash -LiteralPath $common).Hash.ToLowerInvariant() }
$extent = 128
foreach ($axis in @('x', 'y', 'step')) {
    $pixels = [byte[]]::new($extent * $extent * 4)
    for ($y = 0; $y -lt $extent; $y++) {
        for ($x = 0; $x -lt $extent; $x++) {
            $offset = ($y * $extent + $x) * 4
            $value = if ($axis -eq 'step') { if ($y -lt 64) { 0 } else { 255 } } else { 2 * $(if ($axis -eq 'x') { $x } else { $y }) }
            $pixels[$offset] = $value; $pixels[$offset + 1] = $value; $pixels[$offset + 2] = $value; $pixels[$offset + 3] = 255
        }
    }
    Write-Tex "gradient-$axis" $extent $extent $pixels
    Write-Json (Join-Path $output "models/gradient-$axis.json") @{ autosize = $true; material = "materials/gradient-$axis.json" }
    Write-Json (Join-Path $output "materials/gradient-$axis.json") @{ passes = @(@{ shader = 'genericimage4'; textures = @("gradient-$axis"); blending = 'translucent'; cullmode = 'nocull'; depthtest = 'disabled'; depthwrite = 'disabled' }) }
}
$cases = @(
    @{ noise=@(0,0,0); direction=0 },
    @{ noise=@(255,255,255); direction=0 },
    @{ noise=@(128,128,128); direction=0 },
    @{ noise=@(255,0,0); direction=0 },
    @{ noise=@(0,255,0); direction=0 },
    @{ noise=@(0,0,255); direction=0 },
    @{ noise=@(255,255,255); direction=([Math]::PI / 2) },
    @{ noise=@(255,255,255); direction=[Math]::PI },
    @{ noise=@(255,255,255); direction=(3 * [Math]::PI / 2) },
    @{ noise=@(255,255,255); direction=0; mask=0 },
    @{ noise=@(255,255,255); direction=0; mask=128 },
    @{ noise=@(255,255,255); direction=0; mask=255 }
)
if ($Mode -eq 'mask') {
    $cases = foreach ($r8 in @($false, $true)) {
        foreach ($value in @(0, 64, 128, 192, 255)) { @{ noise=@(255,255,255); direction=0; mask=$value; r8=$r8 } }
    }
}
if ($Mode -in @('ramp', 'motion')) {
    $cases = @()
    foreach ($axis in @('x', 'y')) {
        foreach ($grain in @(@(2,0.5), @(0.5,0.5), @(2,0.1), @(2,1))) { $cases += @{ noise_axis=$axis; direction=0; grain=$grain } }
        $cases += @{ noise_axis=$axis; direction=([Math]::PI / 2); grain=@(2,0.5) }
        $cases += @{ noise_axis=$axis; direction=0; grain=@(2,0.5); speed=0.02 }
    }
    if ($Mode -eq 'motion') { foreach ($case in $cases) { $case.speed = 0.02 } }
}
$objects = @(); $probes = @()
foreach ($caseIndex in 0..($cases.Count - 1)) {
    $case = $cases[$caseIndex]
    if ($case.ContainsKey('noise_axis')) {
        $pixels = [byte[]]::new($extent * $extent * 4)
        for ($ny = 0; $ny -lt $extent; $ny++) {
            for ($nx = 0; $nx -lt $extent; $nx++) {
                $offset = ($ny * $extent + $nx) * 4
                $pixels[$offset] = 2 * $(if ($case.noise_axis -eq 'x') { $nx } else { $ny }); $pixels[$offset + 3] = 255
            }
        }
        Write-Tex "noise-$caseIndex" $extent $extent $pixels
    } else { Write-Tex "noise-$caseIndex" 1 1 ([byte[]]@($case.noise[0], $case.noise[1], $case.noise[2], 255)) }
    $mask = $null
    if ($case.ContainsKey('mask')) {
        $mask = "mask-$caseIndex"
        if ($case.r8) { Write-Tex $mask 1 1 ([byte[]]@($case.mask)) -R8 }
        else { Write-Tex $mask 1 1 ([byte[]]@($case.mask, $case.mask, $case.mask, 255)) }
    }
    foreach ($axis in $(if ($Mode -eq 'mask') { @('y', 'step') } else { @('x', 'y') })) {
        $index = $objects.Count; $x = ($index % 4) * $extent; $y = [int][Math]::Floor($index / 4) * $extent
        $grain = if ($case.ContainsKey('grain')) { $case.grain } else { @(2,0.5) }
        $speed = if ($case.ContainsKey('speed')) { $case.speed } else { 0 }
        $constants = @{ ui_editor_properties_amount=0.1; ui_editor_properties_direction=$case.direction; ui_editor_properties_granularity=$grain[0]; ui_editor_properties_granularity_horizontal=$grain[1]; ui_editor_properties_speed=$speed }
        $objects += @{ id=$index + 1; image="models/gradient-$axis.json"; origin="$($x + 64) $($y + 64) 0"; size='128 128'; effects=@(@{ file='effects/cloudmotion/effect.json'; visible=$true; passes=@(@{ constantshadervalues=$constants; textures=@($null,$mask,"noise-$caseIndex") }) }) }
        $probes += @{ id=$index + 1; case=$caseIndex; axis=$axis; origin=@($x,$y); noise=$case.noise; noise_axis=$case.noise_axis; grain=$grain; speed=$speed; direction=$case.direction; mask=$mask; mask_value=$case.mask; r8=[bool]$case.r8; amount=0.1 }
    }
}
Write-Json (Join-Path $output 'scene.json') @{ version=5; general=@{ orthogonalprojection=@{ width=512; height=768 }; clearcolor='0 0 0'; clearenabled=$true; nearz=0.01; farz=10000; zoom=1; bloom=$false; camerafade=$false; cameraparallax=$false }; camera=@{ center='0 0 -1'; eye='0 0 0'; up='0 1 0' }; objects=$objects }
Write-Json (Join-Path $output 'project.json') @{ type='scene'; file='scene.json'; title='Generated cloud-motion reference'; general=@{ properties=@{} } }
Write-Json (Join-Path $output 'probes.json') @{ schema_version=1; mode=$Mode; canvas=@(512,768); extent=$extent; vendor_inputs=@($identities); probes=$probes; notes='Generated gradients isolate displacement, mask response and noise-coordinate motion. Desktop JPEG limits precision.' }
$output
