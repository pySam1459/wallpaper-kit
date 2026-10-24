param(
    [string]$ScenePath,
    [string]$EngineAssets = 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets',
    [switch]$Software,
    [int]$Port = 0,
    [string]$Output
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $root
try {
    cargo build --locked --example browser-worker
    if ($LASTEXITCODE) { throw 'Browser worker build failed' }
    $arguments = @('tools/browser-test/server.mjs', '--port', $Port)
    if ($ScenePath) { $arguments += @('--scene', $ScenePath, '--engine-assets', $EngineAssets) }
    if ($Software) { $arguments += '--software' }
    if ($Output) { $arguments += @('--output', $Output) }
    & node @arguments
    if ($LASTEXITCODE) { throw 'Browser test server failed' }
} finally { Pop-Location }
