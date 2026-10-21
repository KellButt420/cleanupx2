# Non-destructive release guards; no certificate is created, trusted, or installed.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
function Assert-Refused([scriptblock]$Action, [string]$Expected) {
    try { & $Action | Out-Null; throw 'Operation should have been refused.' }
    catch { if ($_.Exception.Message -notlike "*$Expected*") { throw } }
}
foreach ($script in @('package-release.ps1', 'new-appinstaller.ps1')) {
    $tokens = $null
    $errors = $null
    [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $script), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw "PowerShell parse failure: $script" }
}
Assert-Refused { & "$PSScriptRoot/package-release.ps1" -Version 0.1.0.0 -IdentityName Cleanup.Dev -Publisher 'CN=Cleanup Dev' -Unsigned } 'separate production identity'
Assert-Refused { & "$PSScriptRoot/package-release.ps1" -Version 0.1.0.0 -IdentityName WiredIn.Cleanup -Publisher 'CN=wiredincorp' } 'trusted production code-signing certificate'
Assert-Refused { & "$PSScriptRoot/new-appinstaller.ps1" -Package 'missing.msix' -ReleaseBaseUrl 'http://example.com' -FeedUrl 'https://example.com/cleanup.appinstaller' } 'HTTPS'
$candidate = Join-Path $root 'artifacts/release/0.1.0.0/cleanup-0.1.0.0-x64-unsigned.msix'
if (Test-Path $candidate) {
    Assert-Refused { & "$PSScriptRoot/new-appinstaller.ps1" -Package $candidate -ReleaseBaseUrl 'https://example.com/v0.1.0.0' -FeedUrl 'https://example.com/cleanup.appinstaller' } 'valid trusted signature'
    $actual = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
    $expected = (Get-Content (Join-Path (Split-Path $candidate) 'SHA256SUMS.txt') -Raw).Split(' ')[0]
    if ($actual -ne $expected) { throw 'Candidate hash mismatch.' }
}
Write-Output 'Passed: PowerShell syntax, separate identity, required signing, HTTPS feed, unsigned-package rejection, and candidate checksum.'
