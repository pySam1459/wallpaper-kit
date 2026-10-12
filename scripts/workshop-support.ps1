[CmdletBinding()]
param(
    [string]$WorkshopRoot = 'C:/Program Files (x86)/Steam/steamapps/workshop/content/431960',
    [string]$EngineAssets = 'C:/Program Files (x86)/Steam/steamapps/common/wallpaper_engine/assets',
    [string]$Output = ('artifacts/workshop-support-' + [Guid]::NewGuid().ToString('N').Substring(0,8)),
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
if (-not $SkipBuild) {
    & cargo build --locked --example workshop-check | ForEach-Object { [Console]::Error.WriteLine($_) }
    if ($LASTEXITCODE -ne 0) { throw 'workshop-check build failed' }
}
$root = (Resolve-Path -LiteralPath $WorkshopRoot).Path
$engine = (Resolve-Path -LiteralPath $EngineAssets).Path
$folders = @(Get-ChildItem -LiteralPath $root -Directory | Where-Object Name -Match '^\d+$' | Sort-Object { [long]$_.Name })
if ($folders.Count -eq 0) { throw 'No numbered Workshop folders found' }
$outputPath = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $outputPath) { throw 'Evidence directory already exists' }
$null = New-Item -ItemType Directory -Path $outputPath
$buildExecutable = [IO.Path]::GetFullPath('target/debug/examples/workshop-check.exe')
$executable = Join-Path $outputPath 'workshop-check.exe'
# Pin one build for the entire collection and keep Cargo's binary unlocked.
Copy-Item -LiteralPath $buildExecutable -Destination $executable
$results = foreach ($folder in $folders) {
    $result = [ordered]@{ id = $folder.Name; supported = $false; type = $null; error = $null; evidence = $null }
    try {
        $project = Get-Content -LiteralPath (Join-Path $folder.FullName 'project.json') -Raw | ConvertFrom-Json
        $result.type = $project.type
        if ($project.type -notin @('scene', 'Scene', 'video')) { throw "Unsupported wallpaper type: $($project.type)" }
        $start = [Diagnostics.ProcessStartInfo]::new($executable)
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $packageName = if ($project.type -eq 'video') { $project.file } else { [IO.Path]::ChangeExtension($project.file, '.pkg') }
        if ([IO.Path]::GetFileName($packageName) -ne $packageName) { throw 'Project file must be a local filename' }
        foreach ($argument in @((Join-Path $folder.FullName $packageName), $engine, (Join-Path $outputPath $folder.Name))) { $start.ArgumentList.Add($argument) }
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $start
        try {
            if (-not $process.Start()) { throw 'Probe launch failed' }
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(90000)) { $process.Kill($true); $process.WaitForExit(); throw 'Probe exceeded 90 second process limit' }
            $probe = $stdout.Result | ConvertFrom-Json
            if ($null -eq $probe -or $probe.supported -isnot [bool]) { throw "Invalid probe response: $($stderr.Result)" }
            $result.supported = $probe.supported -and $process.ExitCode -eq 0
            $result.error = $probe.error
            $result.evidence = $probe
        } finally {
            if ($process.Id -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
            $process.Dispose()
        }
    } catch { $result.error = $_.Exception.Message }
    [Console]::Error.WriteLine("$($folder.Name): $(if ($result.supported) {'rendered'} else {$result.error})")
    $result
}
$passed = @($results | Where-Object supported).Count
$percent = [math]::Round(100.0 * $passed / $folders.Count, 4)
$unsupported = @($results | Where-Object { -not $_.supported } | ForEach-Object id)
$report = [ordered]@{
    schema_version = 1; supported = $passed; total = $folders.Count; percentage = $percent
    criterion = 'Actual package load, nonempty GPU pixels at 0/1500/4000 ms, exact time-zero rewind and three progressing live samples; unsupported features are reported, not counted as load failures.'
    workshop_root = $root; engine_assets = $engine; executable_sha256 = (Get-FileHash -LiteralPath $executable).Hash
    unsupported_ids = $unsupported; wallpapers = @($results)
}
$report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $outputPath 'report.json') -Encoding utf8
$unsupported | Set-Content -LiteralPath (Join-Path $outputPath 'unsupported.txt') -Encoding utf8
[Console]::Error.WriteLine("$passed/$($folders.Count); unsupported: $($unsupported -join ','); report: $outputPath/report.json")
[Console]::WriteLine($percent.ToString('0.0000', [Globalization.CultureInfo]::InvariantCulture))
