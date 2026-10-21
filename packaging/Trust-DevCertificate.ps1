#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Trusts only this checkout's development package signer in TrustedPeople.
.DESCRIPTION
    Run through an explicit UAC prompt. This is development setup, not part of
    application startup. The app itself always runs asInvoker. The private key
    is never exported. Use -Remove when development trust is no longer needed.
#>
param([switch] $Remove)
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$path = Join-Path $repoRoot 'artifacts\packaging\CleanupDev.cer'
$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($path)
if ($certificate.Subject -ne 'CN=Cleanup Dev') { throw 'Unexpected certificate subject.' }
if ($certificate.NotAfter -le (Get-Date)) { throw 'The development certificate has expired.' }
$eku = $certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' }
if (@($eku.EnhancedKeyUsages | ForEach-Object Value) -notcontains '1.3.6.1.5.5.7.3.3') {
    throw 'The certificate must be a code-signing certificate.'
}
$constraints = $certificate.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.19' }
if (-not $constraints -or $constraints.CertificateAuthority) { throw 'A CA certificate must not be trusted by this script.' }
$storePath = "Cert:\LocalMachine\TrustedPeople\$($certificate.Thumbprint)"
if ($Remove) {
    if (Test-Path -LiteralPath $storePath) { Remove-Item -LiteralPath $storePath }
    Write-Host "Removed development trust: $($certificate.Thumbprint)"
} else {
    Import-Certificate -FilePath $path -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null
    Write-Host "Trusted development signer: $($certificate.Thumbprint)"
}
