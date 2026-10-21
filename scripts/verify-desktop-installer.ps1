param([string]$Version = '0.1.1')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$setup = Join-Path $root "artifacts/desktop/$Version/cleanup-$Version-windows-x64-setup.exe"
$registration = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{DB9EB688-8CC9-4CA8-908E-5658412F8E25}_is1'
if (Test-Path $registration) { throw 'A desktop installation already exists. Use a disposable Windows user or VM for installer verification.' }
$testRoot = Join-Path $root ('artifacts/installer-qa/' + [guid]::NewGuid().ToString('N'))
$installDirectory = Join-Path $testRoot 'installed'
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
$installed = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOICONS',('/DIR="' + $installDirectory + '"'),('/LOG="' + (Join-Path $testRoot 'install.log') + '"')) -WindowStyle Hidden -PassThru
if (-not $installed.WaitForExit(60000) -or $installed.ExitCode -ne 0) { throw 'Desktop installation failed.' }
if (-not (Test-Path (Join-Path $installDirectory 'Cleanup.exe')) -or -not (Test-Path $registration)) { throw 'Installation did not create the app and per-user registration.' }
$reportPath = Join-Path $testRoot 'launch.json'
$process = Start-Process -FilePath (Join-Path $installDirectory 'Cleanup.exe') -ArgumentList @('--selftest','--report',('"' + $reportPath + '"'),'--fixture-dir',('"' + (Join-Path $testRoot 'moves') + '"')) -WindowStyle Hidden -PassThru
if (-not $process.WaitForExit(45000)) { throw 'Installed app startup test timed out.' }
if (-not (Test-Path $reportPath)) { throw "Installed app did not write its test report (exit $($process.ExitCode))." }
$report = Get-Content $reportPath -Raw | ConvertFrom-Json
if ($report.fatal -or $report.errors.Count -or $report.process.packaged -ne $false -or $report.nativeCore.abiVersion -ne 2) { throw 'Installed app failed unpackaged WinUI/native-core verification. Inspect the private report in artifacts/installer-qa.' }
$dataDirectory = $report.process.localStateDirectory
if (-not $dataDirectory) { $dataDirectory = Join-Path $env:LOCALAPPDATA 'Cleanup' }
$marker = Join-Path $dataDirectory ('installer-preservation-' + [guid]::NewGuid().ToString('N') + '.txt')
New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
Set-Content -LiteralPath $marker 'installer data preservation test'
try {
    $uninstaller = Join-Path $installDirectory 'unins000.exe'
    $uninstalled = Start-Process -FilePath $uninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -WindowStyle Hidden -PassThru
    if (-not $uninstalled.WaitForExit(45000) -or $uninstalled.ExitCode -ne 0) { throw 'Uninstaller failed.' }
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ((Test-Path (Join-Path $installDirectory 'Cleanup.exe')) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
    if (Test-Path (Join-Path $installDirectory 'Cleanup.exe')) { throw 'Uninstall left the application executable behind.' }
    if (Test-Path $registration) { throw 'Uninstall left per-user registration behind.' }
    if (-not (Test-Path $marker)) { throw 'Uninstall removed app data.' }
} finally {
    if (Test-Path $marker) { Remove-Item -LiteralPath $marker }
}
Write-Output 'Passed: per-user setup, unpackaged WinUI startup, native ABI, inventory scan, disposable file moves, persistence, uninstall, and user-data preservation.'
Write-Output "Private verification report: $reportPath"
