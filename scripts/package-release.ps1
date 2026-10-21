<# Builds a Release MSIX with a separate, stable identity. No certificate is created or trusted. #>
param(
    [Parameter(Mandatory)] [ValidatePattern('^\d+\.\d+\.\d+\.0$')] [string] $Version,
    [Parameter(Mandatory)] [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9.-]{2,49}$')] [string] $IdentityName,
    [Parameter(Mandatory)] [string] $Publisher,
    [string] $CertificateThumbprint,
    [uri] $TimestampServer = 'http://timestamp.digicert.com',
    [switch] $Unsigned
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
if ($IdentityName -match '(?i)\.dev$' -or $Publisher -eq 'CN=Cleanup Dev') { throw 'Release builds require a separate production identity and publisher.' }
if (-not $Publisher.StartsWith('CN=')) { throw 'Publisher must match the signing certificate subject, starting with CN=.' }
$parsedVersion = [version]$Version
if ($parsedVersion.Major -gt 65535 -or $parsedVersion.Minor -gt 65535 -or $parsedVersion.Build -gt 65535) { throw 'MSIX version components must fit in 16 bits.' }
if (-not $Unsigned) {
    if (-not $CertificateThumbprint) { throw 'Supply a trusted production code-signing certificate thumbprint, or -Unsigned for a non-installable release candidate.' }
    $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
    if (-not $certificate.HasPrivateKey -or $certificate.Subject -ne $Publisher -or $certificate.NotAfter -le (Get-Date)) { throw 'Certificate subject, private key, or validity does not match the release.' }
    if ($certificate.Issuer -eq $certificate.Subject) { throw 'A self-signed development certificate cannot sign public releases.' }
}
$releaseRoot = Join-Path $RepoRoot "artifacts\release\$Version"
New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
[xml]$manifest = Get-Content (Join-Path $RepoRoot 'src\frontend\Package.appxmanifest') -Raw
$manifest.Package.Identity.Name = $IdentityName
$manifest.Package.Identity.Publisher = $Publisher
$manifest.Package.Identity.Version = $Version
$manifest.Package.Properties.PublisherDisplayName = '(wired-in) community'
$alias = $manifest.SelectSingleNode("//*[local-name()='ExecutionAlias']")
$alias.SetAttribute('Alias', 'cleanup.exe')
$manifestPath = Join-Path $releaseRoot 'Package.appxmanifest'
$manifest.Save($manifestPath)
$project = Join-Path $RepoRoot 'src\frontend\Cleanup.csproj'
dotnet restore $project -p:Platform=x64 --locked-mode
if ($LASTEXITCODE -ne 0) { throw 'Locked restore failed.' }
$extra = @()
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path $vswhere) {
    $vsRoot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($vsRoot) {
        $pdbTool = Get-ChildItem "$vsRoot\VC\Tools\MSVC\*\bin\HostX64\x64\mspdbcmf.exe" | Sort-Object FullName -Descending | Select-Object -First 1
        if ($pdbTool) { $extra += "-p:MsPdbCmfExeFullpath=$($pdbTool.FullName)" }
    }
}
$signing = if ($Unsigned) { 'false' } else { 'true' }
if (-not $Unsigned) { $extra += "-p:PackageCertificateThumbprint=$CertificateThumbprint"; $extra += "-p:AppxPackageSigningTimestampServerUrl=$($TimestampServer.AbsoluteUri)" }
dotnet build $project --no-restore -c Release -p:Platform=x64 @extra `
    "-p:CleanupManifestPath=$manifestPath" "-p:Version=$Version" `
    -p:GenerateAppxPackageOnBuild=true -p:AppxBundle=Never -p:UapAppxPackageBuildMode=SideloadOnly `
    -p:AppxSymbolPackageEnabled=false "-p:AppxPackageSigningEnabled=$signing" "-p:AppxPackageDir=$releaseRoot\packages\"
if ($LASTEXITCODE -ne 0) { throw 'Release package build failed.' }
$package = Get-ChildItem "$releaseRoot\packages" -Recurse -Filter '*.msix' | Where-Object { $_.FullName -notmatch '[\\/]Dependencies[\\/]' } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $package) { throw 'Release package was not produced.' }
$archive = [IO.Compression.ZipFile]::OpenRead($package.FullName)
try {
    $reader = [IO.StreamReader]::new($archive.GetEntry('AppxManifest.xml').Open())
    try { [xml]$builtManifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
    $builtIdentity = $builtManifest.Package.Identity
    if ($builtIdentity.Name -ne $IdentityName -or $builtIdentity.Publisher -ne $Publisher -or $builtIdentity.Version -ne $Version) { throw 'Built package identity does not match the release configuration.' }
} finally { $archive.Dispose() }
$suffix = if ($Unsigned) { '-unsigned' } else { '' }
$destination = Join-Path $releaseRoot "cleanup-$Version-x64$suffix.msix"
Copy-Item -LiteralPath $package.FullName -Destination $destination
if (-not $Unsigned -and (Get-AuthenticodeSignature -LiteralPath $destination).Status -ne 'Valid') { throw 'Release signature validation failed.' }
$checksum = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
"$checksum  $([IO.Path]::GetFileName($destination))" | Set-Content (Join-Path $releaseRoot 'SHA256SUMS.txt') -Encoding utf8
Write-Output $destination
