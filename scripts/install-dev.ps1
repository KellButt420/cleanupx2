<#
.SYNOPSIS
    Installs the development package for the current user.

.DESCRIPTION
    Msix mode installs the signed .msix exactly as a user would. It requires the
    development certificate to be trusted machine-wide once (elevated):
        Import-Certificate -FilePath artifacts\packaging\CleanupDev.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople

    Register mode needs no administrator rights: it unpacks the same signed
    .msix and registers the layout with Developer Mode. The app runs with full
    package identity, but signature validation is skipped and the install folder
    is a normal, user-writable folder rather than the protected WindowsApps
    location.

    Msix is the default. Register must be chosen explicitly and does not satisfy
    the signed-install feasibility acceptance criterion.
#>
param(
    [ValidateSet('Msix', 'Register')] [string] $Mode = 'Msix',
    [string] $Package
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

if (-not $Package) {
    $Package = Get-ChildItem (Join-Path $repoRoot 'artifacts\packages') -Recurse -Filter 'Cleanup_*_x64.msix' |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $Package -or -not (Test-Path $Package)) { throw 'No package found. Run scripts\package-dev.ps1 first.' }

$signer = (Get-AuthenticodeSignature $Package).SignerCertificate
$trusted = $signer -and (
    (Test-Path "Cert:\LocalMachine\TrustedPeople\$($signer.Thumbprint)") -or
    (Test-Path "Cert:\LocalMachine\Root\$($signer.Thumbprint)"))

if ($Mode -eq 'Msix') {
    if (-not $trusted) {
        throw "The development certificate is not trusted. From an elevated PowerShell run:`n  Import-Certificate -FilePath `"$repoRoot\artifacts\packaging\CleanupDev.cer`" -CertStoreLocation Cert:\LocalMachine\TrustedPeople"
    }
    $dependencies = @(Get-ChildItem (Join-Path (Split-Path $Package) 'Dependencies\x64') -File -Filter '*.msix' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
    # Switching from a loose layout needs removal. Normal signed updates keep state.
    $existing = Get-AppxPackage -Name 'Cleanup.Dev'
    if ($existing.IsDevelopmentMode) { $existing | Remove-AppxPackage }
    if ($dependencies.Count) {
        Add-AppxPackage -Path $Package -DependencyPath $dependencies -ForceApplicationShutdown -ForceUpdateFromAnyVersion
    } else {
        Add-AppxPackage -Path $Package -ForceApplicationShutdown -ForceUpdateFromAnyVersion
    }
} else {
    $devMode = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock' -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
    if ($devMode -ne 1) { throw 'Register mode requires Developer Mode (Settings > System > For developers).' }

    $makeAppx = Get-ChildItem (Join-Path $env:USERPROFILE '.nuget\packages\microsoft.windows.sdk.buildtools') -Recurse -Filter makeappx.exe |
        Where-Object { $_.Directory.Name -eq 'x64' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
    if (-not $makeAppx) { throw 'makeappx.exe was not found in the NuGet cache; build the package first.' }

    $layout = Join-Path $repoRoot 'artifacts\layout\Cleanup.Dev'
    $expected = [IO.Path]::GetFullPath((Join-Path $repoRoot 'artifacts\layout\Cleanup.Dev'))
    if ([IO.Path]::GetFullPath($layout) -ne $expected) { throw 'Unexpected layout path.' }
    if (Test-Path -LiteralPath $layout) {
        if ((Get-Item -LiteralPath $layout -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw 'The layout must not be a reparse point.'
        }
    }
    Get-AppxPackage -Name 'Cleanup.Dev' | Remove-AppxPackage
    if (Test-Path -LiteralPath $layout) { Remove-Item -LiteralPath $layout -Recurse -Force }
    & $makeAppx unpack /p $Package /d $layout /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'makeappx unpack failed' }
    Add-AppxPackage -Register (Join-Path $layout 'AppxManifest.xml')
}

$installed = Get-AppxPackage -Name 'Cleanup.Dev'
Write-Host "Installed ($Mode): $($installed.PackageFullName)"
Write-Host "Location: $($installed.InstallLocation)"
