[CmdletBinding()]
param([Parameter(Mandatory)][string]$EngineAssets)
$ErrorActionPreference = 'Stop'
$output = Join-Path (Get-Location) ('artifacts/particle-render-reference-' + [Guid]::NewGuid().ToString('N').Substring(0,8))
$null = New-Item -ItemType Directory -Path $output
function Write-Json($Path,$Value) {
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force
    $Value | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}
function Write-Tex($Path,[int]$Width,[int]$Height,[byte[]]$Pixels) {
    if ($Pixels.Length -ne $Width*$Height*4) { throw 'Invalid generated pixels' }
    $null = New-Item -ItemType Directory -Path (Split-Path -Parent $Path) -Force
    $writer = [IO.BinaryWriter]::new([IO.File]::Create($Path))
    try {
        $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXV0005`0TEXI0001`0"))
        foreach($v in @(0,0,$Width,$Height,$Width,$Height,0)){ $writer.Write([uint32]$v) }
        $writer.Write([Text.Encoding]::ASCII.GetBytes("TEXB0004`0"))
        foreach($v in @(1,[uint32]::MaxValue,0,1,$Width,$Height,0,$Pixels.Length,$Pixels.Length)){ $writer.Write([uint32]$v) }
        $writer.Write($Pixels)
    } finally { $writer.Dispose() }
}
$cases = @(
    @{name='baseline'; amount=0; normal=@(255,128,255,255)},
    @{name='refract-x'; amount=0.05; normal=@(255,128,255,255)},
    @{name='refract-y'; amount=0.05; normal=@(128,255,255,255)},
    @{name='refract-half'; amount=0.05; normal=@(255,128,255,255); alpha=0.5},
    @{name='refract-rotated'; amount=0.05; normal=@(255,128,255,255); angle=90},
    @{name='hdr-off'; hdr=$false; brightness=4},
    @{name='hdr-on'; hdr=$true; brightness=4},
    @{name='hdr-unit'; hdr=$true; brightness=1},
    @{name='hdr-threshold'; hdr=$true; brightness=4; threshold=8}
)
$probes = foreach($case in $cases) {
    $dir=Join-Path $output $case.name
    $pixels=[byte[]]::new(256*256*4)
    for($y=0;$y -lt 256;$y++){ for($x=0;$x -lt 256;$x++){
        $p=($y*256+$x)*4
        $pixels[$p]=if($case.ContainsKey('amount')){ $x }else{0}
        $pixels[$p+1]=if($case.ContainsKey('amount')){ $y }else{0}
        $pixels[$p+3]=255
    } }
    Write-Tex (Join-Path $dir 'materials/base.tex') 256 256 $pixels
    Write-Tex (Join-Path $dir 'materials/white.tex') 1 1 ([byte[]]@(255,255,255,255))
    Write-Json (Join-Path $dir 'models/base.json') @{material='materials/base.json'; autosize=$true}
    $state=@{cullmode='nocull';depthtest='disabled';depthwrite='disabled'}
    Write-Json (Join-Path $dir 'materials/base.json') (@{passes=@($state+@{shader='genericimage4';blending='translucent';combos=@{};textures=@('base')})})
    $particle=@{maxcount=64;starttime=0;material='materials/particle.json'; emitter=@(@{name='sphererandom';instantaneous=1;rate=0;distancemin=0;distancemax=0;directions='1 1 0'}); initializer=@(@{name='lifetimerandom';min=120;max=120},@{name='sizerandom';min=128;max=128},@{name='colorrandom';min='255 255 255';max='255 255 255'}); renderer=@(@{name='sprite'}); operator=@()}
    $pass=$state+@{shader='genericparticle';blending='additive';textures=@('white');constantshadervalues=@{ui_editor_properties_overbright=1}}
    if($case.ContainsKey('amount')) {
        Write-Tex (Join-Path $dir 'materials/normal.tex') 1 1 ([byte[]]$case.normal)
        $pass.blending='translucent';$pass.textures=@('white','normal');$pass.combos=@{REFRACT=1}
        $pass.constantshadervalues=@{ui_editor_properties_refract_amount=$case.amount}
        if($case.alpha){ $particle.initializer+=@{name='alpharandom';min=$case.alpha;max=$case.alpha} }
    } else { $pass.constantshadervalues.ui_editor_properties_overbright=$case.brightness; $particle.initializer[1].min=16;$particle.initializer[1].max=16 }
    Write-Json (Join-Path $dir 'materials/particle.json') @{passes=@($pass)}
    $particle.flags=0
    $particle.children=$null
    $particle.controlpoint=@(0..7 | ForEach-Object { @{id=$_;flags=0;offset='0 0 0'} })
    $componentId=1
    foreach($field in @('renderer','initializer','emitter')) { foreach($component in $particle[$field]) {$component.id=$componentId;$componentId++} }
    $particle.emitter[0].origin='0 0 0'
    Write-Json (Join-Path $dir 'particles/probe.json') $particle
    $general=@{orthogonalprojection=@{width=256;height=256}; ambientcolor='0.3 0.3 0.3';skylightcolor='0.3 0.3 0.3';fov=50;clearcolor='0 0 0';clearenabled=$true;nearz=0.01;farz=10000;zoom=1;bloom=$false;hdr=[bool]$case.hdr;camerafade=$false;cameraparallax=$false;bloomhdrstrength=1;bloomhdrthreshold=1;bloomhdrfeather=0;bloomhdrscatter=0.7;bloomhdriterations=4}
    if($case.threshold){$general.bloomhdrthreshold=$case.threshold}
    $angle=if($case.angle){$case.angle}else{0}
    Write-Json (Join-Path $dir 'scene.json') @{version=5;general=$general;camera=@{center='0 0 -1';eye='0 0 0';up='0 1 0'};objects=@(@{id=1;image='models/base.json';origin='128 128 0';size='256 256';color='1 1 1';alignment='center';perspective=$false},@{id=2;particle='particles/probe.json';origin='128 128 0';angles="0 0 $angle"})}
    Write-Json (Join-Path $dir 'project.json') @{type='scene';file='scene.json';title="Particle probe $($case.name)";general=@{properties=@{}}}
    # Independent renderer input contains generated data only. The reference
    # resolves installed engine shaders, which are never copied into source.
    $files=@(Get-ChildItem $dir -File -Recurse | Where-Object Name -ne 'project.json' | Sort-Object FullName)
    $writer=[IO.BinaryWriter]::new([IO.File]::Create((Join-Path $dir 'library.pkg')))
    try {
        $signature=[Text.Encoding]::UTF8.GetBytes('PKGV0016');$writer.Write([uint32]$signature.Length);$writer.Write($signature);$writer.Write([uint32]$files.Count)
        $offset=0
        foreach($file in $files){$key=[Text.Encoding]::UTF8.GetBytes([IO.Path]::GetRelativePath($dir,$file.FullName).Replace('\','/'));$writer.Write([uint32]$key.Length);$writer.Write($key);$writer.Write([uint32]$offset);$writer.Write([uint32]$file.Length);$offset+=$file.Length}
        foreach($file in $files){$writer.Write([IO.File]::ReadAllBytes($file.FullName))}
    } finally {$writer.Dispose()}
    # The engine's loose-project route resolves shader includes locally. Keep
    # these installed vendor inputs in ignored reference folders, outside PKG.
    Copy-Item -LiteralPath (Join-Path $EngineAssets 'shaders') -Destination $dir -Recurse
    @{name=$case.name;project=Join-Path $dir 'project.json';package=Join-Path $dir 'library.pkg';case=$case}
}
Write-Json (Join-Path $output 'probes.json') @{probes=@($probes);engine_assets=(Resolve-Path $EngineAssets).Path;limits='Generated constant particles remove timing and randomness from material comparisons.'}
$output
