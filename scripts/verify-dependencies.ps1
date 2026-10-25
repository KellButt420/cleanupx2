param([switch] $InstallAuditTool)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
$toolsRoot = Join-Path $RepoRoot 'artifacts/audit-tools'
$audit = Join-Path $toolsRoot 'bin/cargo-audit.exe'
if ($InstallAuditTool) {
    cargo install cargo-audit --version 0.22.2 --locked --root $toolsRoot
    if ($LASTEXITCODE -ne 0) { throw 'Rust advisory scanner installation failed; the advisory check did not run.' }
}
if (-not (Test-Path -LiteralPath $audit)) { throw 'Run with -InstallAuditTool to install the pinned advisory scanner into project artifacts.' }
& $audit audit --file (Join-Path $RepoRoot 'src/rust/Cargo.lock') --deny warnings
if ($LASTEXITCODE -ne 0) { throw 'Rust advisory check failed.' }
dotnet restore (Join-Path $RepoRoot 'src/frontend/Cleanup.csproj') --locked-mode -p:Platform=x64 -p:NuGetAudit=true -p:NuGetAuditMode=all '-p:WarningsAsErrors=NU1900%3BNU1901%3BNU1902%3BNU1903%3BNU1904'
if ($LASTEXITCODE -ne 0) { throw 'NuGet advisory refresh failed or reported a vulnerable dependency.' }
$output = & dotnet list (Join-Path $RepoRoot 'src/frontend/Cleanup.csproj') package --vulnerable --include-transitive --format json
if ($LASTEXITCODE -ne 0) { throw 'NuGet advisory check failed.' }
$report = ($output -join "`n") | ConvertFrom-Json
if ($report.problems.Count -or -not $report.projects.Count) { throw 'NuGet did not complete the advisory check.' }
foreach ($project in $report.projects) {
    foreach ($framework in $project.frameworks) {
        foreach ($package in @($framework.topLevelPackages) + @($framework.transitivePackages)) {
            if ($package.vulnerabilities.Count) { throw "Known vulnerable package: $($package.id) $($package.resolvedVersion)" }
        }
    }
}
Write-Output 'Rust and NuGet advisory checks passed.'
