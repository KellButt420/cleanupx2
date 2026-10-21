<#
.SYNOPSIS
    Builds the development MSIX (x64), signed with the development certificate.

.PARAMETER UseLocalFeed
    Restore through scripts\restore-via-local-feed.ps1 instead of nuget.org
    directly (for machines where the .NET SDK cannot reach the network).
#>
param(
    [ValidateSet('Release', 'Debug')] [string] $Configuration = 'Release',
    [switch] $UseLocalFeed
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')

$project = Join-Path $RepoRoot 'src\frontend\Cleanup.csproj'
$packageDirectory = Join-Path $RepoRoot 'artifacts\packages\'

if ($UseLocalFeed) {
    & (Join-Path $PSScriptRoot 'restore-via-local-feed.ps1') -Project $project -RestoreArguments '-p:Platform=x64'
} else {
    dotnet restore $project -p:Platform=x64
    if ($LASTEXITCODE -ne 0) { throw 'dotnet restore failed' }
}

$thumbprint = & (Join-Path $RepoRoot 'packaging\New-DevCertificate.ps1')

# The MSIX targets only look for mspdbcmf.exe under a Visual Studio install
# root, which `dotnet build` does not set. Point them at the C++ build tools.
$extraProperties = @()
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) {
    $vsRoot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($vsRoot) {
        $pdbCmf = Get-ChildItem "$vsRoot\VC\Tools\MSVC\*\bin\HostX64\x64\mspdbcmf.exe" -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending |
            Select-Object -First 1 -ExpandProperty FullName
        if ($pdbCmf) { $extraProperties += "-p:MsPdbCmfExeFullpath=$pdbCmf" }
    }
}

dotnet build $project --no-restore -c $Configuration -p:Platform=x64 @extraProperties `
    -p:GenerateAppxPackageOnBuild=true `
    -p:AppxBundle=Never `
    -p:UapAppxPackageBuildMode=SideloadOnly `
    -p:AppxSymbolPackageEnabled=false `
    -p:AppxPackageSigningEnabled=true `
    -p:PackageCertificateThumbprint=$thumbprint `
    -p:AppxPackageDir=$packageDirectory
if ($LASTEXITCODE -ne 0) { throw 'MSIX build failed' }

$msix = Get-ChildItem $packageDirectory -Recurse -Filter 'Cleanup_*_x64.msix' |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
Write-Host "Package: $($msix.FullName) ($([math]::Round($msix.Length / 1MB, 1)) MB)"
$msix.FullName
