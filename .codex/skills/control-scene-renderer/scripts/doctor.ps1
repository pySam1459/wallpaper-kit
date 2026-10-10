[CmdletBinding()]
param(
    [string]$ScenePath
)

$ErrorActionPreference = 'Stop'

try {
    $projectRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
    $gitPath = Join-Path $projectRoot '.git'
    $checkoutKind = if (Test-Path -LiteralPath $gitPath -PathType Leaf) {
        'worktree'
    } elseif (Test-Path -LiteralPath $gitPath -PathType Container) {
        'main'
    } else {
        'unknown'
    }

    $toolchain = [ordered]@{}
    foreach ($toolName in @('cargo', 'rustc')) {
        $tool = Get-Command $toolName -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        $version = $null
        if ($tool) {
            $previousAutoInstall = $env:RUSTUP_AUTO_INSTALL
            try {
                $env:RUSTUP_AUTO_INSTALL = '0'
                $version = (& $tool.Source --version | Out-String).Trim()
                if ($LASTEXITCODE -ne 0) { throw "$toolName --version failed" }
            } finally {
                $env:RUSTUP_AUTO_INSTALL = $previousAutoInstall
            }
        }
        $toolchain[$toolName] = [ordered]@{ available = [bool]$tool; version = $version }
    }

    $hasManifest = Test-Path -LiteralPath (Join-Path $projectRoot 'Cargo.toml') -PathType Leaf
    $sceneSpecified = -not [string]::IsNullOrWhiteSpace($ScenePath)
    $sceneExists = $false
    if ($sceneSpecified) { $sceneExists = Test-Path -LiteralPath $ScenePath }

    $receipt = $null
    $fresh = $false
    $evidenceRoot = Join-Path $projectRoot 'artifacts'
    if (Test-Path -LiteralPath $evidenceRoot) {
        $manifests = @(Get-ChildItem -LiteralPath $evidenceRoot -Directory | ForEach-Object {
            $manifestPath = Join-Path $_.FullName 'manifest.json'
            if (Test-Path -LiteralPath $manifestPath) { Get-Item -LiteralPath $manifestPath }
        } | Sort-Object LastWriteTimeUtc -Descending)
        foreach ($manifest in $manifests) {
            $candidate = Get-Content -LiteralPath $manifest.FullName -Raw | ConvertFrom-Json
            if ($candidate.status -ne 'passed' -or $candidate.project_root -ne $projectRoot) { continue }
            $currentSources = @('Cargo.toml', 'Cargo.lock') + @(Get-ChildItem (Join-Path $projectRoot 'src'), (Join-Path $projectRoot 'examples'), (Join-Path $projectRoot 'tests'), (Join-Path $projectRoot 'scripts') -Recurse -File | ForEach-Object { [IO.Path]::GetRelativePath($projectRoot, $_.FullName) })
            $matches = (($currentSources | Sort-Object) -join '|') -eq (($candidate.sources.path | Sort-Object) -join '|') -and @($candidate.binaries).Count -eq 5 -and $candidate.motion_status -eq 'passed' -and $candidate.texture_status -eq 'passed' -and $candidate.texture_region_status -eq 'passed' -and $candidate.image_composition_status -eq 'passed' -and $candidate.sprite_extraction_status -eq 'passed'
            foreach ($source in $candidate.sources) {
                $sourcePath = Join-Path $projectRoot $source.path
                if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf) -or (Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash -ne $source.sha256) { $matches = $false; break }
            }
            foreach ($binary in $candidate.binaries) {
                if (-not (Test-Path -LiteralPath $binary.path -PathType Leaf) -or (Get-FileHash -LiteralPath $binary.path -Algorithm SHA256).Hash -ne $binary.sha256) { $matches = $false; break }
            }
            if ($matches -and (-not $fresh -or $candidate.native_status -eq 'passed')) {
                $receipt = $manifest.FullName; $fresh = $true; $verified = $candidate
                if ($candidate.native_status -eq 'passed') { break }
            }
        }
    }
    $nativeReady = $fresh -and $verified.native_status -eq 'passed'
    $blockers = @()
    if (-not $fresh) { $blockers += 'No successful verification receipt matches the current source and executable hashes. Run scripts/verify.ps1.' }
    elseif (-not $nativeReady) { $blockers += 'Headless verification passed, but the native consumer has not been verified for this build.' }
    if (-not $hasManifest) { $blockers += 'No Cargo.toml exists at the repository root.' }
    foreach ($toolName in @('cargo', 'rustc')) {
        if (-not $toolchain[$toolName].available) { $blockers += "$toolName is not available on PATH." }
    }
    if ($sceneSpecified -and -not $sceneExists) { $blockers += 'The supplied scene path does not exist.' }
    if ($sceneSpecified) { $blockers += 'Full scene rendering is not implemented. Diagnostic selected image snapshots report omitted scene behavior. The verified scope is blank, motion, TEX page/region, fixed-time sprite extraction and image composition.' }
    $ready = $nativeReady -and $blockers.Count -eq 0

    [ordered]@{
        schema_version = 1
        project_root = $projectRoot
        checkout_kind = $checkoutKind
        toolchain = $toolchain
        cargo_manifest_exists = $hasManifest
        scene_input = [ordered]@{ path = $ScenePath; specified = $sceneSpecified; exists = $sceneExists }
        runtime_verification_ready = $ready
        verification_scope = 'blank-motion-texture-regions-extracted-sprites-and-images'
        headless_verified = $fresh
        native_verified = $nativeReady
        verification_receipt = $receipt
        scene_verification_ready = $false
        powershell_version = $PSVersionTable.PSVersion.ToString()
        blockers = $blockers
    } | ConvertTo-Json -Depth 5
    if ($ready) { exit 0 } else { exit 2 }
} catch {
    [ordered]@{ schema_version = 1; error = $_.Exception.Message } | ConvertTo-Json
    exit 1
}
