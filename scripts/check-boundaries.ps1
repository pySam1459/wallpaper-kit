[CmdletBinding()]
param([switch]$SelfTest)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Assert-CoreBoundary($Metadata) {
    $packages = @{}
    $nodes = @{}
    foreach ($package in $Metadata.packages) { $packages[$package.id] = $package.name }
    foreach ($node in $Metadata.resolve.nodes) { $nodes[$node.id] = $node }
    $pending = [Collections.Generic.Queue[string]]::new()
    $seen = [Collections.Generic.HashSet[string]]::new()
    $pending.Enqueue($Metadata.resolve.root)
    $forbidden = @('winit', 'gpui', 'gpui_platform', 'napi', 'napi-sys', 'wry', 'tao', 'cef', 'electron')
    while ($pending.Count) {
        $id = $pending.Dequeue()
        if (-not $seen.Add($id)) { continue }
        if ($packages[$id] -in $forbidden) { throw "Core depends on forbidden host package: $($packages[$id])" }
        foreach ($dependency in $nodes[$id].deps) {
            if (@($dependency.dep_kinds | Where-Object { $null -eq $_.kind -or $_.kind -eq 'build' }).Count) {
                $pending.Enqueue($dependency.pkg)
            }
        }
    }
    return @($seen | ForEach-Object { $packages[$_] } | Sort-Object)
}

Push-Location $root
try {
    $raw = & cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $metadata = ($raw | Out-String) | ConvertFrom-Json
    $normal = Assert-CoreBoundary $metadata
    if ($SelfTest) {
        # Turn the real viewer-only edge into a production edge. The checker
        # must reject it, rather than merely printing a dependency list.
        $rootNode = $metadata.resolve.nodes | Where-Object { $_.id -eq $metadata.resolve.root }
        $edge = $rootNode.deps | Where-Object { $_.name -eq 'winit' }
        if (-not $edge) { throw 'Negative fixture requires the winit example dependency' }
        $edge.dep_kinds[0].kind = $null
        $rejected = $false
        try { $null = Assert-CoreBoundary $metadata } catch {
            if ($_.Exception.Message -notlike '*forbidden host package: winit*') { throw }
            $rejected = $true
        }
        if (-not $rejected) { throw 'Dependency checker accepted the forbidden edge' }
    }
    [ordered]@{status='passed'; production_packages=$normal; negative_fixture_checked=[bool]$SelfTest} | ConvertTo-Json -Depth 3
} finally { Pop-Location }
