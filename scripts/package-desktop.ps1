param(
    [ValidatePattern('^\d+\.\d+\.\d+$')] [string] $Version = '0.1.1',
    [string] $CompilerPath
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
$output = Join-Path $RepoRoot "artifacts/desktop/$Version"
$publish = Join-Path $output ('app-' + [guid]::NewGuid().ToString('N'))
if (-not $CompilerPath) { $CompilerPath = & (Join-Path $PSScriptRoot 'get-inno-setup.ps1') }
dotnet publish (Join-Path $RepoRoot 'src/frontend/Cleanup.csproj') -c Release -r win-x64 --self-contained true `
    -p:Platform=x64 -p:CleanupDesktop=true -p:RestoreLockedMode=true -p:DebugSymbols=false -p:DebugType=None "-p:Version=$Version" "-p:PublishDir=$publish\"
if ($LASTEXITCODE -ne 0) { throw 'Self-contained desktop publish failed.' }
$runtimeRoot = Join-Path $RepoRoot 'src/frontend/bin/x64/Release/net10.0-windows10.0.26100.0/win-x64'
Copy-Item -LiteralPath (Join-Path $runtimeRoot 'Cleanup.pri') -Destination $publish
foreach ($folder in @('App','Views','Controls')) {
    Copy-Item -LiteralPath (Join-Path $runtimeRoot $folder) -Destination $publish -Recurse
}
foreach ($required in @('Cleanup.exe','cleanup_native.dll','Microsoft.UI.Xaml.dll','coreclr.dll','release.json','Cleanup.pri')) {
    if (-not (Test-Path (Join-Path $publish $required))) { throw "Required desktop payload is missing: $required" }
}
# Include the license and notice files from the exact restored runtime packages.
$assets = Get-Content (Join-Path $RepoRoot 'src/frontend/obj/project.assets.json') -Raw | ConvertFrom-Json
$cache = $assets.packageFolders.PSObject.Properties.Name | Select-Object -First 1
$notices = Join-Path $publish 'Notices'
New-Item -ItemType Directory -Path $notices -Force | Out-Null
foreach ($library in $assets.libraries.PSObject.Properties.Name | Where-Object { $_ -match '^(Microsoft.WindowsAppSDK\.[^/]+|CommunityToolkit.Mvvm)/' }) {
    $package = Join-Path $cache $library.ToLowerInvariant()
    foreach ($notice in Get-ChildItem $package -File | Where-Object { $_.Name -match '(?i)license|notice' }) {
        $name = $library.Replace('/', '-') + '-' + $notice.Name
        Copy-Item -LiteralPath $notice.FullName -Destination (Join-Path $notices $name)
    }
}
# Runtime packs are download dependencies, not entries in assets.libraries.
$runtimeConfig = Get-Content (Join-Path $publish 'Cleanup.runtimeconfig.json') -Raw | ConvertFrom-Json
$runtimeVersion = ($runtimeConfig.runtimeOptions.includedFrameworks | Where-Object name -eq 'Microsoft.NETCore.App').version
if (-not $runtimeVersion) { throw 'The published app must include a self-contained .NET runtime.' }
$runtimePackage = Join-Path $cache "microsoft.netcore.app.runtime.win-x64/$runtimeVersion"
foreach ($name in @('LICENSE.TXT','THIRD-PARTY-NOTICES.TXT')) {
    Copy-Item -LiteralPath (Join-Path $runtimePackage $name) -Destination (Join-Path $notices "Microsoft.NETCore.App-$runtimeVersion-$name")
}
$innoLicense = Join-Path (Split-Path $CompilerPath) 'license.txt'
if (Test-Path $innoLicense) { Copy-Item -LiteralPath $innoLicense -Destination (Join-Path $notices 'Inno-Setup-license.txt') }
$metadata = cargo metadata --manifest-path (Join-Path $RepoRoot 'src/rust/Cargo.toml') --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Rust dependency metadata failed.' }
foreach ($package in $metadata.packages | Where-Object source) {
    foreach ($notice in Get-ChildItem (Split-Path $package.manifest_path) -File | Where-Object { $_.Name -match '(?i)license|notice|copyright' }) {
        Copy-Item -LiteralPath $notice.FullName -Destination (Join-Path $notices "$($package.name)-$($package.version)-$($notice.Name)")
    }
}
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vsRoot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$crt = Get-ChildItem "$vsRoot/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT" -Directory | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $crt) { throw 'Visual C++ redistributable DLLs are required to produce the offline installer.' }
Get-ChildItem $crt.FullName -Filter '*.dll' | Copy-Item -Destination $publish
& $CompilerPath /Qp "/DAppVersion=$Version" "/DSourceDir=$publish" "/DOutputDir=$output" (Join-Path $RepoRoot 'packaging/cleanup.iss')
if ($LASTEXITCODE -ne 0) { throw 'Setup EXE build failed.' }
$portable = Join-Path $output "cleanup-$Version-windows-x64-portable.zip"
Compress-Archive -Path (Join-Path $publish '*') -DestinationPath $portable -CompressionLevel Optimal -Force
Set-Content (Join-Path $output 'publish-directory.txt') $publish -Encoding utf8
Get-ChildItem $output -File | Where-Object { $_.Extension -in '.exe','.zip' } | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
} | Set-Content (Join-Path $output 'SHA256SUMS.txt') -Encoding utf8
Get-ChildItem $output -File | Select-Object FullName,Length
