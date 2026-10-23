param([ValidatePattern('^\d+\.\d+\.\d+$')] [string] $Version = '0.1.2')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $root "artifacts/desktop/$Version/Cleanup.exe"
if (-not (Test-Path $exe)) { throw "Portable app was not found: $exe" }
$run = Join-Path $root ('artifacts/portable-qa/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$reportPath = Join-Path $run 'launch.json'
$fixtures = Join-Path $run 'moves'
# Extract into an empty directory so this run exercises a true first launch
# rather than reusing a cached extraction under %TEMP%\.net.
$extractBase = Join-Path $run 'extract'
$previousExtractBase = $env:DOTNET_BUNDLE_EXTRACT_BASE_DIR
try {
    $env:DOTNET_BUNDLE_EXTRACT_BASE_DIR = $extractBase
    $process = Start-Process -FilePath $exe -ArgumentList @(
        '--selftest', '--report', ('"' + $reportPath + '"'), '--fixture-dir', ('"' + $fixtures + '"')
    ) -WindowStyle Hidden -PassThru
} finally {
    if ($null -eq $previousExtractBase) { Remove-Item Env:DOTNET_BUNDLE_EXTRACT_BASE_DIR -ErrorAction SilentlyContinue }
    else { $env:DOTNET_BUNDLE_EXTRACT_BASE_DIR = $previousExtractBase }
}
if (-not $process.WaitForExit(60000)) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw 'Portable app startup/storage self-test timed out.'
}
if (-not (Test-Path $reportPath)) { throw "Portable app did not write its self-test report (exit $($process.ExitCode))." }
$report = Get-Content $reportPath -Raw | ConvertFrom-Json
if ($report.fatal -or $report.errors.Count -or $report.process.packaged -ne $false -or $report.nativeCore.abiVersion -ne 2) {
    throw 'Portable startup, unpackaged state, or native ABI verification failed.'
}
if ($report.process.startupToWindowLoadedMs -le 0 -or -not $report.inventory.summary -or $report.inventory.summary.sourceErrors.Count) {
    throw 'WinUI window startup or installed-app inventory scan failed.'
}
if (-not $report.fileMoves.moved -or -not $report.fileMoves.movedBack -or -not $report.fileMoves.existingFileUnchanged) {
    throw 'Portable file move/undo storage verification failed.'
}
$expectedState = Join-Path $env:LOCALAPPDATA 'Cleanup'
if ([IO.Path]::GetFullPath($report.process.localStateDirectory) -ne [IO.Path]::GetFullPath($expectedState)) {
    throw 'Portable app data is not stored under the current user LocalAppData directory.'
}
if ($report.nativeCore.environment.elevated) { throw 'Portable app self-test unexpectedly ran elevated.' }
# The .NET runtime is linked into the single-file host, so coreclr.dll is not
# extracted; WinUI and the Rust core must load from the fresh extraction.
$extractDirectory = Split-Path $report.nativeCore.environment.nativeLibraryPath
if (-not [IO.Path]::GetFullPath($extractDirectory).StartsWith([IO.Path]::GetFullPath($extractBase), [StringComparison]::OrdinalIgnoreCase)) {
    throw "The Rust core loaded from outside the fresh extraction directory: $extractDirectory"
}
foreach ($required in @('cleanup_native.dll', 'Microsoft.UI.Xaml.dll', 'Microsoft.WindowsAppRuntime.dll', 'Cleanup.pri')) {
    if (-not (Test-Path (Join-Path $extractDirectory $required))) {
        throw "A self-extracted portable dependency is missing: $required"
    }
}
foreach ($binary in Get-ChildItem $extractDirectory -Recurse -File -Include *.dll, *.exe) {
    $text = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($binary.FullName))
    if ($text -match '(?i)(vcruntime140|msvcp140|concrt140|vccorlib140)[_0-9a-z]*\.dll') {
        throw "$($binary.Name) depends on the Visual C++ redistributable ($($Matches[0]))."
    }
}
Write-Output 'Passed: direct Cleanup.exe first launch, fresh self-extraction of WinUI/Windows App SDK/Rust files, no Visual C++ redistributable dependency, AppData state, inventory scan, and disposable file move/undo.'
Write-Output "Private verification report: $reportPath"
