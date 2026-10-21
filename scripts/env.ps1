# Dot-source to put the pinned toolchains on PATH for the current session:
#   . .\scripts\env.ps1
# Nothing is installed or changed outside this PowerShell process.

$ErrorActionPreference = 'Stop'
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# Rust: fall back to rustup's default location when cargo is not on PATH.
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (-not (Test-Path (Join-Path $cargoBin 'cargo.exe'))) {
        throw 'cargo was not found. Install Rust with rustup: https://rustup.rs'
    }
    $env:PATH = "$cargoBin;$env:PATH"
}

# .NET: global.json pins the SDK. Prefer a machine-wide install that has it,
# otherwise use a user-local SDK in %USERPROFILE%\.dotnet.
$requiredSdk = (Get-Content (Join-Path $RepoRoot 'global.json') -Raw | ConvertFrom-Json).sdk.version
$requiredMajorMinor = ($requiredSdk -split '\.')[0..1] -join '.'
$dotnetRoot = $null
foreach ($candidate in @("$env:ProgramFiles\dotnet", "$env:USERPROFILE\.dotnet")) {
    $exe = Join-Path $candidate 'dotnet.exe'
    if ((Test-Path $exe) -and ((& $exe --list-sdks) -match "^$([regex]::Escape($requiredMajorMinor))\.")) {
        $dotnetRoot = $candidate
        break
    }
}
if (-not $dotnetRoot) {
    throw ".NET SDK $requiredSdk (or a later $requiredMajorMinor feature band) was not found. Install it from https://dotnet.microsoft.com/download"
}
$env:DOTNET_ROOT = $dotnetRoot
$env:PATH = "$dotnetRoot;$env:PATH"
$env:DOTNET_NOLOGO = '1'

Write-Host "cargo : $((Get-Command cargo).Source)"
Write-Host "dotnet: $dotnetRoot (SDK $(dotnet --version))"
