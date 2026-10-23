# Runs the portable EXE and the setup inside Windows Sandbox: a clean, disposable
# Windows 11 with no developer tools, .NET, Windows App SDK, or Visual C++
# redistributable. Requires Windows 11 Pro/Enterprise with the feature enabled
# once from an elevated PowerShell (then reboot):
#   Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All
param([ValidatePattern('^\d+\.\d+\.\d+$')] [string] $Version = '0.1.2', [int] $TimeoutSeconds = 300)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$sandbox = Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
if (-not (Test-Path $sandbox)) {
    throw 'Windows Sandbox is not enabled. In an elevated PowerShell run: Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM -All, then reboot.'
}
if (Get-Process WindowsSandboxClient, WindowsSandboxRemoteSession -ErrorAction SilentlyContinue) { throw 'Close the running Windows Sandbox first; only one can run at a time.' }
$artifacts = Join-Path $root "artifacts/desktop/$Version"
$setup = Join-Path $artifacts "cleanup-$Version-windows-x64-setup.exe"
foreach ($file in @((Join-Path $artifacts 'Cleanup.exe'), $setup)) { if (-not (Test-Path $file)) { throw "Build artifacts first: $file is missing." } }

$run = Join-Path $root ('artifacts/sandbox-qa/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
Copy-Item (Join-Path $artifacts 'Cleanup.exe'), $setup -Destination $run
# Runs inside the sandbox as its local account (Windows PowerShell 5.1).
@'
$ErrorActionPreference = 'Continue'
$qa = 'C:\qa'
$result = [ordered]@{ vcRuntimeInSystem32 = Test-Path "$env:WINDIR\System32\vcruntime140.dll"; dotnetInstalled = Test-Path "$env:ProgramFiles\dotnet\shared\Microsoft.NETCore.App" }
function Invoke-SelfTest([string] $exe, [string] $name) {
    $report = Join-Path $qa "$name.json"
    $process = Start-Process $exe -ArgumentList @('--selftest', '--report', "`"$report`"", '--fixture-dir', "`"$env:TEMP\qa-$name`"") -PassThru
    $finished = $process.WaitForExit(120000)
    if (-not $finished) { Stop-Process -Id $process.Id -Force }
    $json = if (Test-Path $report) { Get-Content $report -Raw | ConvertFrom-Json } else { $null }
    [ordered]@{
        exited = $finished; exitCode = $(if ($finished) { $process.ExitCode } else { $null })
        windowLoadedMs = $json.process.startupToWindowLoadedMs; nativeAbi = $json.nativeCore.abiVersion
        fileMoves = [bool]($json.fileMoves.moved -and $json.fileMoves.movedBack -and $json.fileMoves.existingFileUnchanged)
        errors = @($json.errors) + @($json.fatal | Where-Object { $_ })
        passed = [bool]($json -and -not $json.fatal -and -not $json.errors.Count -and $json.nativeCore.abiVersion -eq 2 -and $json.fileMoves.moved -and $json.fileMoves.movedBack)
    }
}
$portable = Join-Path $env:USERPROFILE 'Downloads\Cleanup.exe'
New-Item -ItemType Directory (Split-Path $portable) -Force | Out-Null
Copy-Item (Join-Path $qa 'Cleanup.exe') $portable
$result.portable = Invoke-SelfTest $portable 'portable'
$setup = Get-ChildItem $qa -Filter '*-setup.exe' | Select-Object -First 1
$install = Start-Process $setup.FullName -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru
$result.setupExitCode = if ($install.WaitForExit(180000)) { $install.ExitCode } else { 'timeout' }
$result.setup = Invoke-SelfTest "$env:LOCALAPPDATA\Programs\cleanup\Cleanup.exe" 'setup'
$result | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $qa 'result.json') -Encoding UTF8
'@ | Set-Content (Join-Path $run 'run.ps1') -Encoding UTF8
@"
<Configuration>
  <Networking>Disable</Networking>
  <MappedFolders><MappedFolder><HostFolder>$([Security.SecurityElement]::Escape((Resolve-Path $run).Path))</HostFolder><SandboxFolder>C:\qa</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\qa\run.ps1</Command></LogonCommand>
</Configuration>
"@ | Set-Content (Join-Path $run 'cleanup.wsb') -Encoding UTF8

Start-Process (Join-Path $run 'cleanup.wsb')
$resultPath = Join-Path $run 'result.json'
$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while (-not (Test-Path $resultPath) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Seconds 3 }
if (-not (Test-Path $resultPath)) { throw "Sandbox run did not finish within $TimeoutSeconds s. Inspect $run." }
Start-Sleep -Seconds 1
$result = Get-Content $resultPath -Raw | ConvertFrom-Json
$result | ConvertTo-Json -Depth 5
if (-not $result.portable.passed -or -not $result.setup.passed -or $result.setupExitCode -ne 0) { throw 'Clean-sandbox verification failed; see the result above.' }
Write-Output 'Passed on clean Windows Sandbox: portable first launch and setup install, each with WinUI startup, native core, and file move/undo. Close the Sandbox window when done.'
