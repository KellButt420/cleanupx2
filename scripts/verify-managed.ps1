$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
dotnet run --project (Join-Path $RepoRoot 'tests/managed/ManagedChecks.csproj') -c Release -p:RestoreLockedMode=true
if ($LASTEXITCODE -ne 0) { throw 'Managed regression checks failed.' }
