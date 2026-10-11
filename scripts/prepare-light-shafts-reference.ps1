[CmdletBinding()]
param([Parameter(Mandatory)][string]$EngineAssets)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$engine = (Resolve-Path -LiteralPath $EngineAssets).Path
$output = Join-Path $root ('artifacts/light-shafts-reference-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$null = New-Item -ItemType Directory -Path $output
function Write-Json([string]$Path, $Value) {
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force
    $Value | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}
function Write-Tex([string]$Path, [int]$Width, [int]$Height, [byte[]]$Pixels) {
    if ($Pixels.Length -ne $Width * $Height * 4) { throw 'Generated texture extent differs' }
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force
    $writer = [IO.BinaryWriter]::new([IO.File]::Create($Path))
    try {
        $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXV0005`0TEXI0001`0"))
        foreach ($value in @(0, 0, $Width, $Height, $Width, $Height, 0)) { $writer.Write([uint32]$value) }
        $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXB0004`0"))
        foreach ($value in @(1, [uint32]::MaxValue, 0, 1, $Width, $Height, 0, $Pixels.Length, $Pixels.Length)) { $writer.Write([uint32]$value) }
        $writer.Write($Pixels)
    } finally { $writer.Dispose() }
}

# Copy vendor inputs only into ignored local reference projects. The renderer
# does not consume these shaders. Hashes pin the experiment to installed inputs.
$vendorFiles = @(
    'effects/lightshafts/effect.json',
    'effects/lightshafts/materials/effects/lightshafts.json',
    'effects/lightshafts/shaders/effects/lightshafts.vert',
    'effects/lightshafts/shaders/effects/lightshafts.frag',
    'shaders/common.h', 'shaders/common_perspective.h',
    'shaders/common_fragment.h', 'shaders/common_blending.h',
    'materials/util/noise.tex', 'materials/gradient/gradient_iridescent.tex'
)
$identities = foreach ($relative in $vendorFiles) {
    $source = Join-Path $engine $relative
    @{ path=$relative; sha256=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant() }
}
$identityPoints = @('0 0', '1 0', '1 1', '0 1')
$cases = @(
    @{ name='identity'; points=$identityPoints; noise=@(255,255,255,255); gradient=@(255,255,255,255) },
    @{ name='sized'; points=$identityPoints; size='128 128'; noise=@(255,255,255,255); gradient=@(255,255,255,255) },
    @{ name='inset'; points=@('0.25 0.25','0.75 0.25','0.75 0.75','0.25 0.75'); noise=@(255,255,255,255); gradient=@(255,255,255,255) },
    @{ name='trapezoid'; points=@('0.4 0.1','0.6 0.1','0.9 0.9','0.1 0.9'); noise=@(255,255,255,255); gradient_axis='x' },
    @{ name='zero-noise'; points=$identityPoints; noise=@(0,0,0,255); gradient=@(255,255,255,255) },
    @{ name='red-noise'; points=$identityPoints; noise=@(255,0,0,255); gradient=@(255,255,255,255) },
    @{ name='green-noise'; points=$identityPoints; noise=@(0,255,0,255); gradient=@(255,255,255,255) },
    @{ name='blue-noise'; points=$identityPoints; noise=@(0,0,255,255); gradient=@(255,255,255,255) },
    @{ name='half-noise'; points=$identityPoints; noise=@(128,128,128,255); gradient=@(255,255,255,255) },
    @{ name='transparent-gradient'; points=$identityPoints; noise=@(255,255,255,255); gradient=@(255,255,255,0) },
    @{ name='half-gradient'; points=$identityPoints; noise=@(255,255,255,255); gradient=@(128,128,128,255) },
    @{ name='gradient-x'; points=$identityPoints; noise=@(255,255,255,255); gradient_axis='x' },
    @{ name='gradient-y'; points=$identityPoints; noise=@(255,255,255,255); gradient_axis='y' },
    @{ name='mask-zero'; points=$identityPoints; noise=@(255,255,255,255); gradient=@(255,255,255,255); mask=0 },
    @{ name='mask-half'; points=$identityPoints; noise=@(255,255,255,255); gradient=@(255,255,255,255); mask=128 },
    @{ name='motion'; points=$identityPoints; noise_axis='x'; gradient=@(255,255,255,255); speed=0.12 },
    @{ name='target'; points=@('-1.36286 1.35948','-0.57913 -0.17720','1.41440 -0.06330','1.48965 1.00833'); target=$true }
)
$probes = foreach ($case in $cases) {
    $directory = Join-Path $output $case.name
    foreach ($relative in $vendorFiles) {
        $destination = Join-Path $directory ($relative -replace '^effects/lightshafts/(materials|shaders)/', '$1/')
        $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force
        Copy-Item -LiteralPath (Join-Path $engine $relative) -Destination $destination
    }
    foreach ($kind in @('noise','gradient')) {
        $axis = $case["${kind}_axis"]
        if ($axis) {
            $extent = 128
            $pixels = [byte[]]::new($extent * $extent * 4)
            for ($y = 0; $y -lt $extent; $y++) {
                for ($x = 0; $x -lt $extent; $x++) {
                    $offset = ($y * $extent + $x) * 4
                    $value = 2 * $(if ($axis -eq 'x') { $x } else { $y })
                    $pixels[$offset] = $value; $pixels[$offset+1] = $value; $pixels[$offset+2] = $value; $pixels[$offset+3] = 255
                }
            }
            Write-Tex (Join-Path $directory "materials/probe-$kind.tex") $extent $extent $pixels
        } elseif ($case.ContainsKey($kind)) {
            Write-Tex (Join-Path $directory "materials/probe-$kind.tex") 1 1 ([byte[]]$case[$kind])
        }
    }
    $constants = @{
        point0=$case.points[0]; point1=$case.points[1]; point2=$case.points[2]; point3=$case.points[3]
        colorastart='1 1 1'; colorend='1 1 1'; colorwexponent=1; colorwintensity=0.5
        noiseamount=0; noisescale=0; rayfeather='0 0'; rayradius=0.12; rayscale='0.17 0.43'; raysmoothness=1; rayspeed=0
    }
    if ($case.speed) { $constants.rayspeed = $case.speed }
    $combos = @{ DIRECTDRAW=1; RAYCORNER=1; RAYMODE=2; RENDERING=1 }
    $textures = @($null, 'probe-noise', 'probe-gradient')
    if ($case.ContainsKey('mask')) {
        Write-Tex (Join-Path $directory 'materials/probe-mask.tex') 1 1 ([byte[]]@($case.mask,$case.mask,$case.mask,255))
        $combos.MASK = 1; $textures += 'probe-mask'
    }
    if ($case.target) {
        $constants.colorend = '0.43529411764705883 0.8862745098039215 1'
        $constants.colorwexponent = 0.2; $constants.colorwintensity = 0.53; $constants.rayspeed = 0.12
        $textures = @($null, 'util/noise', 'gradient/gradient_iridescent')
    }
    $width = if ($case.target) { 512 } else { 256 }
    $height = if ($case.target) { 288 } else { 256 }
    $shape = @{ id=1; shape='quad'; origin="$($width/2) $($height/2) 0"; effects=@(@{ file='effects/lightshafts/effect.json'; visible=$true; passes=@(@{ combos=$combos; constantshadervalues=$constants; textures=$textures }) }) }
    if ($case.size) { $shape.size = $case.size }
    Write-Json (Join-Path $directory 'scene.json') @{ version=5; general=@{ orthogonalprojection=@{ width=$width; height=$height }; clearcolor='0 0 0'; clearenabled=$true; nearz=0.01; farz=10000; zoom=1; bloom=$false; camerafade=$false; cameraparallax=$false }; camera=@{ center='0 0 -1'; eye='0 0 0'; up='0 1 0' }; objects=@($shape) }
    Write-Json (Join-Path $directory 'project.json') @{ type='scene'; file='scene.json'; title="Generated light-shafts $($case.name)"; general=@{ properties=@{} } }
    @{ name=$case.name; project="$($case.name)/project.json"; canvas=@($width,$height); case=$case; constants=$constants; combinations=$combos; textures=$textures }
}
Write-Json (Join-Path $output 'probes.json') @{ schema_version=1; vendor_inputs=@($identities); probes=@($probes); notes='Independent generated inputs for black-box geometry, noise channels, gradient, mask and time observations. No renderer pixel model is asserted yet. Target uses the authored settings at reduced resolution, with other scene layers absent.' }
$output
