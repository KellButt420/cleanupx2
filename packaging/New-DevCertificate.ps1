<#
.SYNOPSIS
    Creates (or reuses) the development package-signing certificate.

.DESCRIPTION
    Development packages are signed with a self-signed certificate whose
    subject matches the development manifest publisher ("CN=Cleanup Dev").
    The private key stays in the current user's certificate store and is never
    exported. Release packages use a different identity and certificate.

    Outputs the certificate thumbprint and writes the public certificate to
    artifacts\packaging\CleanupDev.cer.

    Installing a package signed with this certificate requires trusting it
    once, from an elevated PowerShell:
        Import-Certificate -FilePath artifacts\packaging\CleanupDev.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
#>
$ErrorActionPreference = 'Stop'
$subject = 'CN=Cleanup Dev'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

$certificate = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $subject -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date).AddDays(14) } |
    Sort-Object NotAfter -Descending |
    Select-Object -First 1

if (-not $certificate) {
    $certificate = New-SelfSignedCertificate `
        -Type Custom `
        -Subject $subject `
        -FriendlyName 'Cleanup development package signing' `
        -KeyUsage DigitalSignature `
        -KeyAlgorithm RSA `
        -KeyLength 3072 `
        -HashAlgorithm SHA256 `
        -KeyExportPolicy NonExportable `
        -NotAfter (Get-Date).AddYears(1) `
        -CertStoreLocation Cert:\CurrentUser\My `
        -TextExtension @(
            '2.5.29.37={text}1.3.6.1.5.5.7.3.3',  # Enhanced key usage: code signing
            '2.5.29.19={text}'                   # Basic constraints: end entity
        )
}

$outputDirectory = Join-Path $repoRoot 'artifacts\packaging'
New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
Export-Certificate -Cert $certificate -FilePath (Join-Path $outputDirectory 'CleanupDev.cer') | Out-Null
$certificate.Thumbprint
