[CmdletBinding()]
param([switch]$Software, [switch]$SkipNative)

$ErrorActionPreference = 'Stop'
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
    Invoke-Cargo @('build', '--locked', '--examples')
    & "$PSScriptRoot/check-boundaries.ps1" -SelfTest
    $metadata = & cargo metadata --locked --offline --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $target = Join-Path $metadata.target_directory 'debug/examples'
    $headlessOutput = Join-Path $output 'headless'
    $headlessArgs = @('--output', $headlessOutput)
    if ($Software) { $headlessArgs += '--software' }
    & (Join-Path $target 'headless.exe') @headlessArgs
    if ($LASTEXITCODE -ne 0) { throw 'Headless verification failed' }
    if (-not $SkipNative) {
        & "$PSScriptRoot/verify-native.ps1" -Executable (Join-Path $target 'native-viewer.exe') -Output (Join-Path $output 'native') -Software:$Software
    }
    $status = & git status --porcelain=v1 --untracked-files=all
    $revision = & git rev-parse --verify HEAD 2>$null
    if ($LASTEXITCODE -ne 0) { $revision = $null }
    $sourceFiles = @('Cargo.toml', 'Cargo.lock') + @(Get-ChildItem src, examples, tests, scripts -Recurse -File | ForEach-Object { [IO.Path]::GetRelativePath($root, $_.FullName) })
    $sourceHashes = @($sourceFiles | Sort-Object | ForEach-Object { [ordered]@{path=$_; sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash} })
    $binaries = @('headless.exe', 'native-viewer.exe') | ForEach-Object {
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
        limits=@('Blank rendering only.', 'Captures use diagnostic readback.', 'No scene, GPU timing or host latency claims.');
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'manifest.json') -Encoding utf8
    Write-Output "Evidence: $output"
} catch {
    [ordered]@{status='failed'; error=$_.Exception.Message} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'failure.json') -Encoding utf8
    throw
} finally { Pop-Location }
