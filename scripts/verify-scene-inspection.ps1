[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$ScenePath,
    [Parameter(Mandatory)][string]$EngineAssets,
    [Parameter(Mandatory)][string]$Output
)

$ErrorActionPreference = 'Stop'
$sceneHash = (Get-FileHash -LiteralPath $ScenePath -Algorithm SHA256).Hash
$expectedHash = '551F74EECA12B2F760BFF5A9C79F9F28C623C1E1D31C3642719B67633FF91F14'
if ($sceneHash -ne $expectedHash) { throw 'Target package identity changed; re-inventory before updating the acceptance fixture' }
& $Executable --package $ScenePath --assets $EngineAssets --output $Output
if ($LASTEXITCODE -ne 0) { throw 'Scene inspection failed or reported blocking findings' }
$report = Get-Content -LiteralPath (Join-Path $Output 'report.json') -Raw | ConvertFrom-Json
if ($report.rendering_supported -or $report.schema_version -ne 1) { throw 'Inspection must not claim scene rendering support' }
if ($report.package.sha256 -ne $sceneHash -or $report.package.signature -ne 'PKGV0024' -or $report.package.entries -ne 42) { throw 'Unexpected package metadata' }
if ((Get-FileHash -LiteralPath $ScenePath -Algorithm SHA256).Hash -ne $sceneHash) { throw 'Package changed during inspection' }
if ($report.scene.version -ne 5 -or ($report.scene.canvas -join ',') -ne '3840,2160') { throw 'Unexpected scene version or canvas' }
if (($report.scene.objects.id -join ',') -ne '28,32,22,72,89,71,103,364,77,96,100,86') { throw 'Authored composition order changed' }
$expectedKinds = @{'image'=7; 'particle'=1; 'shape'=1; 'composition'=1; 'sound'=2}
foreach ($kind in $expectedKinds.Keys) {
    if (@($report.scene.objects | Where-Object kind -eq $kind).Count -ne $expectedKinds[$kind]) { throw "Incorrect object count for $kind" }
}
if (@($report.diagnostics | Where-Object { $_.severity -in @('error', 'unsupported') }).Count) { throw 'Blocking asset/profile findings' }
if (@($report.diagnostics | Where-Object code -eq 'muted-audio').Count -ne 2) { throw 'Audio omission must be explicit' }
if (($report.effects.replacement_key | Sort-Object) -join ',' -ne 'blur_precise,chromatic_aberration,cloudmotion,lightshafts') { throw 'Required effects were not all inspected' }
$sprites = @($report.textures | Where-Object frames -gt 0)
if ($sprites.Count -ne 2) { throw 'Expected two distinct shared sprite assets' }
foreach ($sprite in $sprites) {
    if ($sprite.frames -ne 480 -or [Math]::Abs($sprite.duration_seconds - 28.8) -gt 0.00001) { throw 'Sprite timing differs from the reference inventory' }
}
$birds = $sprites | Where-Object path -eq 'materials/Oiseaux optimized_00000.tex'
$character = $sprites | Where-Object path -eq 'materials/Resize Yuta_00000.tex'
if ($birds.pages.Count -ne 27 -or $birds.pages[0].decoded_bytes -ne 248832000 -or $character.pages.Count -ne 30 -or $character.pages[0].decoded_bytes -ne 55259616) { throw 'Sprite page metadata differs from the reference inventory' }
$blur = $report.effects | Where-Object replacement_key -eq 'blur_precise'
if ($blur.passes.Count -ne 2 -or $blur.passes[1].bindings.Count -ne 2 -or $blur.passes[1].bindings[1].source -ne 'previous') { throw 'Blur must retain the previous input as well as its first pass' }
$external = @($report.assets | Where-Object origin -eq 'engine')
foreach ($asset in $external) {
    if ((Get-FileHash -LiteralPath (Join-Path $EngineAssets $asset.path) -Algorithm SHA256).Hash -ne $asset.sha256) { throw "External asset hash mismatch: $($asset.path)" }
}
[ordered]@{
    status='passed'; package_sha256=$sceneHash; resolved_assets=$report.assets.Count;
    external_assets=$external.Count; textures=$report.textures.Count; visual_objects=10;
    sprite_frames=480; rendering_supported=$false;
    limits=@('Declared and conditional reference graph; no GLSL preprocessing or shader-equivalence proof.', 'Payloads are hashed but not decompressed or decoded.', 'Scene rendering and visual fidelity remain unimplemented.');
} | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $Output 'verification.json') -Encoding utf8
