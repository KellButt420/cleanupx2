<#
.SYNOPSIS
    Restores NuGet packages through a local folder feed that PowerShell populates.

.DESCRIPTION
    Workaround for development machines where processes running on the .NET 10
    runtime (including the SDK's own NuGet client) cannot open outbound
    connections, while PowerShell can. This was observed while bootstrapping
    Phase 0; see docs/phase-0-report.md.

    The script resolves the package closure the way NuGet does (nearest target
    framework group, lowest version satisfying each range), downloads the
    .nupkg files from nuget.org with Invoke-WebRequest, and then runs an offline
    `dotnet restore` against the folder feed. NuGet still validates the
    nuget.org repository signature of every package when it extracts it.

    Normal machines do not need this: `dotnet restore` works directly.
#>
param(
    [Parameter(Mandatory)] [string] $Project,
    [string] $Feed = (Join-Path $env:LOCALAPPDATA 'cleanup-nuget-local-feed'),
    [string[]] $RestoreArguments = @()
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot 'env.ps1') | Out-Null

$FlatContainer = 'https://api.nuget.org/v3-flatcontainer'
New-Item -ItemType Directory -Force -Path $Feed | Out-Null

function ConvertTo-NormalizedVersion([string] $Version) {
    $core, $suffix = $Version -split '(?=[-+])', 2
    $parts = [System.Collections.Generic.List[string]]($core -split '\.')
    while ($parts.Count -lt 3) { $parts.Add('0') }
    if ($parts.Count -eq 4 -and $parts[3] -eq '0') { $parts.RemoveAt(3) }
    $normalized = ($parts | ForEach-Object { [string][long]$_ }) -join '.'
    if ($suffix -and $suffix.StartsWith('-')) { $normalized += ($suffix -split '\+')[0] }
    $normalized.ToLowerInvariant()
}

$versionIndex = @{}
function Get-AvailableVersions([string] $Id) {
    $key = $Id.ToLowerInvariant()
    if (-not $versionIndex.ContainsKey($key)) {
        $versionIndex[$key] = (Invoke-RestMethod "$FlatContainer/$key/index.json").versions
    }
    $versionIndex[$key]
}

function ConvertTo-ComparableVersion([string] $Version) {
    $core = ($Version -split '[-+]')[0]
    if ($core -notmatch '\.') { $core += '.0' }
    [version]$core
}

# Lowest *available* version satisfying a NuGet range such as "1.2.3" (>= 1.2.3),
# "[1.2.3, )", "[1.2.3]" or "(1.0, 2.0]", matching NuGet's lowest-applicable rule.
# Upper bounds are not enforced; the lowest match is always within them in practice.
function Resolve-MinimumVersion([string] $Id, [string] $Range) {
    $Range = $Range.Trim()
    if ($Range -match '^\[\s*([^,\]\s]+)\s*\]$') { return ConvertTo-NormalizedVersion $Matches[1] }
    $inclusive = $true
    if ($Range -match '^[\d]') { $lower = $Range }
    elseif ($Range -match '^\[\s*([^,\s]+)\s*,') { $lower = $Matches[1] }
    elseif ($Range -match '^\(\s*([^,\s]+)\s*,') { $lower = $Matches[1]; $inclusive = $false }
    else { throw "Cannot resolve version range '$Range' for $Id" }

    $available = Get-AvailableVersions $Id
    $normalizedLower = ConvertTo-NormalizedVersion $lower
    if ($inclusive -and $available -contains $normalizedLower) { return $normalizedLower }
    $lowerVersion = ConvertTo-ComparableVersion $lower
    $candidate = $available |
        Where-Object { $_ -notmatch '-' } |
        Where-Object { $v = ConvertTo-ComparableVersion $_; if ($inclusive) { $v -ge $lowerVersion } else { $v -gt $lowerVersion } } |
        Sort-Object { ConvertTo-ComparableVersion $_ } |
        Select-Object -First 1
    if (-not $candidate) { throw "No available version of $Id satisfies '$Range'" }
    $candidate
}

# Picks the dependency group NuGet would use for a net10.0-windows project.
function Select-DependencyGroup($Groups) {
    $best = $null; $bestScore = -1
    foreach ($group in $Groups) {
        $tfm = [string]$group.targetFramework
        $score = switch -Regex ($tfm) {
            '^$' { 100; break }
            '^net(\d+)\.(\d+)-windows' { if ([int]$Matches[1] -le 10) { 4000 + [int]$Matches[1] * 10 + [int]$Matches[2] } else { -1 }; break }
            '^net(\d+)\.(\d+)$' { if ([int]$Matches[1] -ge 5 -and [int]$Matches[1] -le 10) { 3000 + [int]$Matches[1] * 10 + [int]$Matches[2] } else { -1 }; break }
            '^\.NETCoreApp(\d+)\.(\d+)$' { 3000 + [int]$Matches[1] * 10 + [int]$Matches[2]; break }
            '^\.NETStandard(\d+)\.(\d+)$' { 2000 + [int]$Matches[1] * 10 + [int]$Matches[2]; break }
            default { -1 }
        }
        if ($score -gt $bestScore) { $best = $group; $bestScore = $score }
    }
    $best
}

function Get-PackageFile([string] $Id, [string] $Version) {
    $id = $Id.ToLowerInvariant()
    $path = Join-Path $Feed "$id.$Version.nupkg"
    if (-not (Test-Path $path)) {
        Write-Host "  download $id $Version"
        Invoke-WebRequest "$FlatContainer/$id/$Version/$id.$Version.nupkg" -OutFile "$path.partial"
        Move-Item "$path.partial" $path
    }
    $path
}

function Get-Dependencies([string] $PackagePath) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($PackagePath)
    try {
        $entry = $zip.Entries | Where-Object { $_.FullName -notmatch '/' -and $_.Name -like '*.nuspec' } | Select-Object -First 1
        $reader = New-Object System.IO.StreamReader($entry.Open())
        [xml]$nuspec = $reader.ReadToEnd()
        $reader.Dispose()
    } finally { $zip.Dispose() }
    $deps = $nuspec.package.metadata.dependencies
    if (-not $deps) { return @() }
    $groups = @($deps.group)
    if ($groups.Count -gt 0 -and $groups[0]) {
        $group = Select-DependencyGroup $groups
        if ($group) { return @($group.dependency | Where-Object { $_ }) }
        return @()
    }
    @($deps.dependency | Where-Object { $_ })
}

$seen = @{}
function Add-Closure([string] $Id, [string] $Version) {
    $key = "$($Id.ToLowerInvariant())/$Version"
    if ($seen.ContainsKey($key)) { return }
    $seen[$key] = $true
    $path = Get-PackageFile $Id $Version
    foreach ($dependency in Get-Dependencies $path) {
        $range = if ($dependency.version) { $dependency.version } else { '0.0.0' }
        Add-Closure $dependency.id (Resolve-MinimumVersion $dependency.id $range)
    }
}

$feedConfig = Join-Path $Feed 'nuget.config'
@"
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="local-feed" value="$Feed" />
  </packageSources>
</configuration>
"@ | Set-Content $feedConfig

for ($attempt = 1; $attempt -le 6; $attempt++) {
    Write-Host "Restore attempt $attempt"
    $output = & dotnet restore $Project --configfile $feedConfig -p:NuGetAudit=false @RestoreArguments 2>&1 | Out-String
    if ($LASTEXITCODE -eq 0) {
        Write-Host 'Restore succeeded.'
        return
    }

    $wanted = [System.Collections.Generic.List[object]]::new()
    $specPath = Get-ChildItem (Join-Path (Split-Path $Project) 'obj') -Filter '*.nuget.dgspec.json' | Select-Object -First 1
    if ($specPath) {
        $spec = Get-Content $specPath.FullName -Raw | ConvertFrom-Json
        foreach ($projectSpec in $spec.projects.PSObject.Properties.Value) {
            foreach ($framework in $projectSpec.frameworks.PSObject.Properties.Value) {
                foreach ($dependency in $framework.dependencies.PSObject.Properties) {
                    if ($dependency.Value.version) { $wanted.Add(@($dependency.Name, $dependency.Value.version)) }
                }
                foreach ($download in $framework.downloadDependencies) { $wanted.Add(@($download.name, $download.version)) }
            }
        }
    }
    foreach ($match in [regex]::Matches($output, 'Unable to find package (\S+) with version \(([^)]+)\)')) {
        $range = $match.Groups[2].Value -replace '^>=\s*', '' -replace '^=\s*', ''
        $wanted.Add(@($match.Groups[1].Value, $range))
    }
    foreach ($item in $wanted) {
        Add-Closure $item[0] (Resolve-MinimumVersion $item[0] $item[1])
    }
    if ($attempt -eq 6) {
        Write-Host $output
        throw 'Restore did not succeed after populating the local feed.'
    }
}
