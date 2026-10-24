$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
foreach ($relative in @('target/debug/examples/browser-worker.exe', 'tools/browser-test/node_modules/chrome-devtools-mcp/build/src/bin/chrome-devtools-mcp.js')) {
    if (!(Test-Path -LiteralPath (Join-Path $root $relative))) { throw 'Run scripts/verify-browser.ps1 -Install first' }
}
$config = Join-Path $root '.codex/config.toml'
if ((Test-Path -LiteralPath $config) -and (Get-Content -LiteralPath $config -Raw) -match '(?m)^\[mcp_servers\.wallpaper_kit_browser\]') {
    Write-Output 'Existing wallpaper_kit_browser MCP configuration preserved.'
    return
}
$node = (Get-Command node -CommandType Application | Select-Object -First 1).Source | ConvertTo-Json -Compress
$entry = (Join-Path $root 'tools/browser-test/mcp.mjs') | ConvertTo-Json -Compress
$block = @"

[mcp_servers.wallpaper_kit_browser]
command = $node
args = [$entry]
startup_timeout_sec = 60
tool_timeout_sec = 60
"@
Add-Content -LiteralPath $config -Value $block -Encoding utf8
Write-Output "Configured project MCP server: $config"
Write-Output 'Reload the MCP connection or start a new coding session to expose its tools.'
