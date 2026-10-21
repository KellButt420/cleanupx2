<#
.SYNOPSIS
    Runs the Phase 0 feasibility checks against the installed development package.

.DESCRIPTION
    1. Builds and registers disposable fixtures (never real applications):
         - a per-user Win32 application with an official uninstaller (HKCU)
         - a per-user Windows Installer (MSI) product
         - a Developer-Mode MSIX package
         - with -IncludeElevated, a per-machine application whose uninstaller
           requires administrator approval (installing it shows a UAC prompt,
           and so does removing it)
    2. Launches the installed Cleanup package twice through its execution alias:
         run A: probes and inventory
         run B: probes, inventory, and sequential official uninstall of the fixtures
    3. Verifies from outside the package that the fixtures were really removed and
       that the uninstallers ran outside the package container.
    4. Removes fixture leftovers.

    Windows Installer shows progress without rebooting the machine. This script
    never sends clicks to installer dialogs or approves UAC prompts.

    Results are written to artifacts\phase0\<timestamp>\. Run scripts\package-dev.ps1
    and scripts\install-dev.ps1 first.
#>
param(
    [switch] $IncludeElevated,
    [switch] $RequireSignedPackage
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1') | Out-Null

$installedPackage = Get-AppxPackage -Name 'Cleanup.Dev'
if (-not $installedPackage) { throw 'Cleanup.Dev is not installed. Run scripts\install-dev.ps1 first.' }
if ($RequireSignedPackage -and $installedPackage.IsDevelopmentMode) {
    throw 'A loose development layout is registered. Install with scripts\install-dev.ps1 -Mode Msix before running this acceptance check.'
}
if (-not (Get-Command cleanup-dev.exe -ErrorAction SilentlyContinue)) { throw 'The cleanup-dev.exe execution alias is not available.' }

$run = Join-Path $RepoRoot ("artifacts\phase0\" + (Get-Date -Format 'yyyyMMdd-HHmmss'))
$diag = Join-Path $run 'fixture-diagnostics'
New-Item -ItemType Directory -Force -Path $diag | Out-Null

# --- Fixtures -------------------------------------------------------------
cargo build --release --quiet --manifest-path (Join-Path $RepoRoot 'tests\fixtures\disposable-app\Cargo.toml') --target-dir (Join-Path $RepoRoot 'artifacts\target\fixtures')
if ($LASTEXITCODE -ne 0) { throw 'Fixture build failed' }
$fixtureBin = Join-Path $RepoRoot 'artifacts\target\fixtures\release'
$fixture = Join-Path $fixtureBin 'cleanup-fixture.exe'
$fixtureApp = Join-Path $fixtureBin 'cleanup-disposable-app.exe'

function Get-FixtureStatus([string] $Scope) { & $fixture status --scope $Scope | ConvertFrom-Json }

function Remove-FixtureLayout {
    # Verify the final absolute path before a recursive delete.
    $expected = [IO.Path]::GetFullPath((Join-Path $RepoRoot 'artifacts\fixtures\disposable-msix'))
    $resolved = [IO.Path]::GetFullPath($msixLayout)
    if ($resolved -ne $expected) { throw "Unexpected fixture layout: $resolved" }
    if (Test-Path -LiteralPath $resolved) {
        $item = Get-Item -LiteralPath $resolved -Force
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Fixture layout must not be a reparse point.' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}

& $fixture purge --scope user
& $fixture install --scope user --diag-dir $diag
if ($LASTEXITCODE -ne 0) { throw 'Per-user fixture install failed' }

if ($IncludeElevated) {
    Write-Host 'Installing the per-machine fixture (approve the UAC prompt)...'
    $p = Start-Process $fixture -Verb RunAs -Wait -PassThru -ArgumentList 'install', '--scope', 'machine', '--diag-dir', "`"$diag`""
    if ($p.ExitCode -ne 0) { throw 'Per-machine fixture install failed' }
}

$msiProductCode = '{6E1C3A52-7B1D-4F0A-9C3E-2D5B8A41F7C9}'
$msiPath = Join-Path $RepoRoot 'artifacts\fixtures\cleanup-disposable-test.msi'
& (Join-Path $RepoRoot 'tests\fixtures\disposable-msi\New-DisposableMsi.ps1') -OutputPath $msiPath | Out-Null
if (-not ('MsiProductState' -as [type])) { Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class MsiProductState { [DllImport("msi.dll", CharSet = CharSet.Unicode)] public static extern int MsiQueryProductStateW(string productCode); }' }
function Test-MsiFixtureInstalled { [MsiProductState]::MsiQueryProductStateW($msiProductCode) -eq 5 }
if (Test-MsiFixtureInstalled) { Start-Process msiexec.exe -Wait -ArgumentList '/x', $msiProductCode, '/qn' }
$msiInstall = Start-Process msiexec.exe -Wait -PassThru -ArgumentList '/i', "`"$msiPath`"", '/qn', '/l*v', "`"$(Join-Path $run 'msi-install.log')`""
if ($msiInstall.ExitCode -ne 0) { throw "MSI fixture install failed ($($msiInstall.ExitCode))" }

$msixLayout = Join-Path $RepoRoot 'artifacts\fixtures\disposable-msix'
Get-AppxPackage -Name 'Cleanup.DisposableTestPackage' | Remove-AppxPackage
Remove-FixtureLayout
New-Item -ItemType Directory -Force -Path (Join-Path $msixLayout 'Assets') | Out-Null
Copy-Item (Join-Path $RepoRoot 'tests\fixtures\disposable-msix\AppxManifest.xml') $msixLayout
Copy-Item $fixtureApp $msixLayout
Copy-Item (Join-Path $RepoRoot 'src\frontend\Assets\*.png') (Join-Path $msixLayout 'Assets')
Add-AppxPackage -Register (Join-Path $msixLayout 'AppxManifest.xml')

$before = [ordered]@{
    user = Get-FixtureStatus 'user'
    machine = if ($IncludeElevated) { Get-FixtureStatus 'machine' } else { $null }
    msiInstalled = Test-MsiFixtureInstalled
    msixRegistered = [bool](Get-AppxPackage -Name 'Cleanup.DisposableTestPackage')
}

# --- Runs inside the package ---------------------------------------------
function Invoke-SelfTest([string] $Name, [string[]] $Extra) {
    $report = Join-Path $run "$Name.json"
    $arguments = @('--selftest', '--report', "`"$report`"", '--fixture-dir', "`"$(Join-Path $run "$Name-moves")`"") + $Extra
    $process = Start-Process cleanup-dev.exe -ArgumentList $arguments -PassThru
    if (-not $process.WaitForExit(300000)) { throw "$Name did not finish within 5 minutes" }
    if (-not (Test-Path $report)) { throw "$Name produced no report" }
    Get-Content $report -Raw | ConvertFrom-Json
}

$runA = Invoke-SelfTest 'run-a' @()

$uninstallArguments = @('--uninstall-fixtures') + $(if ($IncludeElevated) { @('--allow-elevation') } else { @() })
$runB = Invoke-SelfTest 'run-b' $uninstallArguments

# --- Verification from outside the package --------------------------------
function Read-Diagnostics([string] $Scope, [string] $Stage) {
    $path = Join-Path $diag "$Scope-$Stage.json"
    if (Test-Path $path) { Get-Content $path -Raw | ConvertFrom-Json } else { $null }
}

$after = [ordered]@{
    user = Get-FixtureStatus 'user'
    machine = if ($IncludeElevated) { Get-FixtureStatus 'machine' } else { $null }
    msiInstalled = Test-MsiFixtureInstalled
    msiMarkerPresent = Test-Path 'HKCU:\Software\CleanupDisposableTestMsi'
    msixRegistered = [bool](Get-AppxPackage -Name 'Cleanup.DisposableTestPackage')
}
$uninstallers = [ordered]@{
    userStage1 = Read-Diagnostics 'user' 'stage1'
    userStage2 = Read-Diagnostics 'user' 'stage2'
    userStage2Done = Read-Diagnostics 'user' 'stage2-done'
    machineStage1 = Read-Diagnostics 'machine' 'stage1'
    machineStage2 = Read-Diagnostics 'machine' 'stage2'
}

$checks = [ordered]@{
    packagedWithIdentity = [bool]$runA.process.packaged -and [bool]$runA.nativeCore.environment.packageFullName
    nativeLibraryLoadedFromPackage = [string]$runA.nativeCore.environment.nativeLibraryPath -like "$($runA.process.installedPath)*"
    inventoryFoundAllFixtures = (@($runB.inventory.fixtures).Count -eq $(if ($IncludeElevated) { 4 } else { 3 }))
    win32FixtureRemoved = $before.user.registered -and -not $after.user.registered -and -not $after.user.installDirExists
    win32UninstallerOutsideContainer = $uninstallers.userStage1 -and -not $uninstallers.userStage1.packageFullName -and $uninstallers.userStage2 -and -not $uninstallers.userStage2.packageFullName
    msiFixtureRemoved = $before.msiInstalled -and -not $after.msiInstalled -and -not $after.msiMarkerPresent
    msixFixtureRemoved = $before.msixRegistered -and -not $after.msixRegistered
    coreVerifiedEveryRemoval = @($runB.uninstall.results).Count -eq $(if ($IncludeElevated) { 4 } else { 3 }) -and @($runB.uninstall.results | Where-Object { $_.report.status -ne 'removed' }).Count -eq 0
    persistedAcrossRestart = [int]$runB.persistence.previousEntries -ge 1
    fileMovesSafe = [bool]$runA.fileMoves.moved -and [bool]$runA.fileMoves.overwriteRefused -and [bool]$runA.fileMoves.existingFileUnchanged -and [bool]$runA.fileMoves.movedBack
    noErrors = (@($runA.errors).Count + @($runB.errors).Count) -eq 0
}
if ($IncludeElevated) {
    $checks.machineFixtureRemoved = $before.machine.registered -and -not $after.machine.registered
    $checks.machineUninstallerElevated = [bool]$uninstallers.machineStage2.elevated
}

# How long after the uninstaller's last process finished its work did the core
# report completion? (Launch start + duration, minus the fixture's own timestamp.)
$win32Result = $runB.uninstall.results | Where-Object { $_.report.mechanism -eq 'registeredUninstaller' } | Select-Object -First 1
$completionLagMs = if ($win32Result -and $uninstallers.userStage2Done) {
    [long]$win32Result.report.launch.startedAtUnixMs + [long]$win32Result.report.launch.durationMs - [long]$uninstallers.userStage2Done.timeUnixMs
} else { $null }

$summary = [ordered]@{
    installation = [ordered]@{
        packageFullName = $installedPackage.PackageFullName
        installLocation = $installedPackage.InstallLocation
        isDevelopmentMode = [bool]$installedPackage.IsDevelopmentMode
        signatureKind = [string]$installedPackage.SignatureKind
    }
    signedInstallAcceptance = -not [bool]$installedPackage.IsDevelopmentMode
    checks = $checks
    win32CompletionLagMs = $completionLagMs
    before = $before
    after = $after
    uninstallerDiagnostics = $uninstallers
    uninstallResults = $runB.uninstall.results
    metrics = [ordered]@{
        startupToWindowLoadedMs = @($runA.process.startupToWindowLoadedMs, $runB.process.startupToWindowLoadedMs)
        idle = $runA.idle
        inventoryElapsedMs = @($runA.inventory.elapsedMs, $runB.inventory.elapsedMs)
        inventoryRecords = $runA.inventory.records
    }
}
$summary | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $run 'summary.json')

# --- Cleanup -------------------------------------------------------------
& $fixture purge --scope user
if (Test-MsiFixtureInstalled) { Start-Process msiexec.exe -Wait -ArgumentList '/x', $msiProductCode, '/qn' }
Get-AppxPackage -Name 'Cleanup.DisposableTestPackage' | Remove-AppxPackage
Remove-FixtureLayout
if ($IncludeElevated -and ((Get-FixtureStatus 'machine').registered -or (Get-FixtureStatus 'machine').dataDirExists)) {
    Write-Host 'Removing per-machine fixture leftovers (approve the UAC prompt)...'
    Start-Process $fixture -Verb RunAs -Wait -ArgumentList 'purge', '--scope', 'machine' | Out-Null
}

$checks.GetEnumerator() | ForEach-Object { '{0,-36} {1}' -f $_.Key, $(if ($_.Value) { 'PASS' } else { 'FAIL' }) }
Write-Host "Results: $run"
Write-Host "Signed MSIX installation acceptance: $(-not [bool]$installedPackage.IsDevelopmentMode)"
if (@($checks.Values | Where-Object { -not $_ }).Count) { throw 'Phase 0 feasibility checks failed. See summary.json.' }
