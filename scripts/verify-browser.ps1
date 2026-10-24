param(
    [string]$ScenePath,
    [string]$EngineAssets = 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets',
    [switch]$Hardware,
    [switch]$Install,
    [switch]$TurbulenceFixture,
    [string]$Output
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $root
try {
    cargo build --locked --example browser-worker
    if ($LASTEXITCODE) { throw 'Browser worker build failed' }
    if ($Install) {
        npm ci --prefix tools/browser-test --ignore-scripts --no-audit --no-fund
        if ($LASTEXITCODE) { throw 'Browser test dependencies failed to install' }
    }
    $arguments = @('tools/browser-test/verify.mjs')
    if ($ScenePath) { $arguments += @('--scene', $ScenePath, '--engine-assets', $EngineAssets) }
    if ($Hardware) { $arguments += '--hardware' }
    if ($TurbulenceFixture) { $arguments += '--turbulence-fixture' }
    if ($Output) { $arguments += @('--output', $Output) }
    & node @arguments
    if ($LASTEXITCODE) { throw 'Playwright / Chrome DevTools verification failed' }
} finally { Pop-Location }
