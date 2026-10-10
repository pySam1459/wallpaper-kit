[CmdletBinding()]
param([switch]$Software, [switch]$SkipNative, [string]$ScenePath, [string]$EngineAssets)

$ErrorActionPreference = 'Stop'
if ([bool]$ScenePath -ne [bool]$EngineAssets) { throw 'ScenePath and EngineAssets must be supplied together' }
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$runId = '{0}-{1}' -f (Get-Date -Format 'yyyyMMdd-HHmmss'), ([Guid]::NewGuid().ToString('N').Substring(0, 8))
$output = Join-Path $root "artifacts/$runId"
$null = New-Item -ItemType Directory -Path $output

function Invoke-Cargo([string[]]$Arguments) {
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') failed" }
}

Push-Location $root
try {
    Invoke-Cargo @('fmt', '--all', '--', '--check')
    Invoke-Cargo @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
    Invoke-Cargo @('test', '--locked')
    Invoke-Cargo @('doc', '--locked', '--no-deps')
    Invoke-Cargo @('build', '--locked', '--examples')
    & "$PSScriptRoot/check-boundaries.ps1" -SelfTest | Tee-Object -FilePath (Join-Path $output 'boundaries.json')
    $metadata = & cargo metadata --locked --offline --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $target = Join-Path $metadata.target_directory 'debug/examples'
    $inspectionStatus = 'skipped'
    if ($ScenePath) {
        & "$PSScriptRoot/verify-scene-inspection.ps1" -Executable (Join-Path $target 'scene-inspect.exe') -ScenePath $ScenePath -EngineAssets $EngineAssets -Output (Join-Path $output 'scene-inspection')
        $inspectionStatus = 'passed'
    }
    $headlessOutput = Join-Path $output 'headless'
    $headlessArgs = @('--output', $headlessOutput)
    if ($Software) { $headlessArgs += '--software' }
    & (Join-Path $target 'headless.exe') @headlessArgs
    if ($LASTEXITCODE -ne 0) { throw 'Headless verification failed' }
    $motionArgs = @('--output', (Join-Path $output 'motion'))
    if ($Software) { $motionArgs += '--software' }
    & (Join-Path $target 'motion-checks.exe') @motionArgs
    if ($LASTEXITCODE -ne 0) { throw 'Motion verification failed' }
    $textureArgs = @('--output', (Join-Path $output 'textures'))
    if ($Software) { $textureArgs += '--software' }
    if ($ScenePath) { $textureArgs += @('--scene', $ScenePath, '--engine-assets', $EngineAssets) }
    & (Join-Path $target 'texture-checks.exe') @textureArgs
    if ($LASTEXITCODE -ne 0) { throw 'Texture verification failed' }
    if (-not $SkipNative) {
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native') -Software:$Software
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-texture') -Software:$Software -TextureFixture
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-region') -Software:$Software -RegionFixture
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-layers') -Software:$Software -LayersFixture
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-extracted') -Software:$Software -ExtractedFixture
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-paged') -Software:$Software -PagedFixture
        & "$PSScriptRoot/verify-image-playback.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-image-playback') -Software:$Software
        if ($ScenePath) {
            & "$PSScriptRoot/verify-image-playback.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native-target-images') -Software:$Software -ScenePath $ScenePath -EngineAssets $EngineAssets
        }
    }
    $status = & git status --porcelain=v1 --untracked-files=all
    $revision = & git rev-parse --verify HEAD 2>$null
    if ($LASTEXITCODE -ne 0) { $revision = $null }
    $sourceFiles = @('Cargo.toml', 'Cargo.lock') + @(Get-ChildItem src, examples, tests, scripts -Recurse -File | ForEach-Object { [IO.Path]::GetRelativePath($root, $_.FullName) })
    $sourceHashes = @($sourceFiles | Sort-Object | ForEach-Object { [ordered]@{path=$_; sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash} })
    $binaries = @('headless.exe', 'native-viewer.exe', 'motion-checks.exe', 'scene-inspect.exe', 'texture-checks.exe') | ForEach-Object {
        $path = Join-Path $target $_
        [ordered]@{path=$path; sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}
    }
    $video = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, AdapterRAM)
    $cpu = @(Get-CimInstance Win32_Processor | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors)
    $os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber
    [ordered]@{
        schema_version=1; status='passed'; run_id=$runId; project_root=$root; revision=$revision;
        working_tree=@($status); sources=$sourceHashes; binaries=@($binaries); rustc=(& rustc --version); os=$os; gpu=$video; cpu=$cpu;
        native_status=$(if ($SkipNative) {'skipped'} else {'passed'}); software=[bool]$Software;
        motion_status='passed';
        texture_status='passed';
        texture_region_status='passed';
        image_composition_status='passed';
        gaussian_blur_status='passed';
        precise_blur_status='passed';
        image_playback_status='passed';
        native_image_playback_status=$(if ($SkipNative) {'skipped'} else {'passed'});
        target_image_playback_status=$(if ($SkipNative -or -not $ScenePath) {'skipped'} else {'passed'});
        sprite_extraction_status='passed';
        sprite_worker_status='passed';
        sprite_prefetch_status='passed';
        scene_inspection_status=$inspectionStatus;
        limits=@('Blank, diagnostic motion, TEX page/region, sprite extraction/background preparation/prefetch and selected image composition/playback only.', 'Paired scene roots also exercise two-loop headless sprite readiness and native playback of seven selected target images when native checks run. Full scene rendering remains unsupported.', 'Controlled captures use diagnostic readback; live sprite timing does not.', 'No scene fidelity, GPU timing or host latency claims.');
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'manifest.json') -Encoding utf8
    Write-Output "Evidence: $output"
} catch {
    [ordered]@{status='failed'; error=$_.Exception.Message} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'failure.json') -Encoding utf8
    throw
} finally { Pop-Location }
