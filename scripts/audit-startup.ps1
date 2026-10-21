# Warm-launch and idle measurements of the normal installed UI, never the selftest window.
# Two states per run: the Core landing page after launch, and Applications after its inventory loads.
param([int] $Runs=3)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'ui-automation.ps1')
$package=Get-AppxPackage Cleanup.Dev
$report=Join-Path $env:LOCALAPPDATA ('Packages/'+$package.PackageFamilyName+'/LocalState/performance-audit.json')
$out=Join-Path (Resolve-Path (Join-Path $PSScriptRoot '../artifacts')).Path 'performance/startup'
New-Item -ItemType Directory -Path $out -Force | Out-Null
function Read-Report { try { Get-Content -LiteralPath $report -Raw | ConvertFrom-Json } catch { $null } }
function Get-IdleSample([string] $State, $Process, $Snapshot) {
    # No UIA traversal during the idle sample, and no working-set trimming or forced collection.
    Start-Sleep -Seconds 3
    $Process.Refresh(); $startCpu=$Process.CPU; $watch=[Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds 5
    $Process.Refresh()
    [pscustomobject]@{state=$State;windowLoadedMs=[math]::Round($Snapshot.WindowLoadedMs,1);inventoryMs=[math]::Round($Snapshot.InventoryMs,1);inventoryCount=$Snapshot.InventoryCount;workingSetMiB=[math]::Round($Process.WorkingSet64/1MB,1);privateMiB=[math]::Round($Process.PrivateMemorySize64/1MB,1);managedHeapMiB=$Snapshot.ManagedHeapMiB;gcCommittedMiB=$Snapshot.GcCommittedMiB;cpuPercent=[math]::Round(($Process.CPU-$startCpu)/$watch.Elapsed.TotalSeconds/[Environment]::ProcessorCount*100,3);idleSeconds=[math]::Round($watch.Elapsed.TotalSeconds,3);handles=$Process.HandleCount;threads=$Process.Threads.Count}
}
$results=@()
for($run=1;$run -le $Runs;$run++) {
    $old=Get-Process Cleanup -ErrorAction SilentlyContinue | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
    if($old) {
        Connect-CleanupUi $old.Id
        $script:UiWindow.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close()
        if(-not $old.WaitForExit(5000)){throw 'The app did not close; no process was killed.'}
    }
    Remove-Item -LiteralPath $report -ErrorAction SilentlyContinue
    Start-Process -FilePath (Join-Path $env:LOCALAPPDATA 'Microsoft/WindowsApps/cleanup-dev.exe') -ArgumentList '--performance-audit'
    $deadline=[DateTime]::UtcNow.AddSeconds(12)
    $process=$null; $snapshot=$null
    do {
        Start-Sleep -Milliseconds 100
        $process=Get-Process Cleanup -ErrorAction SilentlyContinue | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1
        if($process) {
            $candidate=Read-Report
            if($candidate -and $candidate.ProcessId -eq $process.Id -and $candidate.WindowLoadedMs -gt 0){$snapshot=$candidate;break}
        }
    } while([DateTime]::UtcNow -lt $deadline)
    if(-not $snapshot){throw 'Normal-UI audit report did not become ready.'}
    $results += Get-IdleSample 'core-landing' $process $snapshot

    # Applications loads its inventory on first visit; measure that state too, once the report shows it loaded.
    Connect-CleanupUi $process.Id
    Select-CleanupUi 'Applications'
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do { Start-Sleep -Milliseconds 200; $candidate=Read-Report; if($candidate -and $candidate.InventoryCount -gt 0){$snapshot=$candidate;break} } while([DateTime]::UtcNow -lt $deadline)
    if(-not $snapshot -or $snapshot.InventoryCount -le 0){throw 'Applications inventory did not report.'}
    $results += Get-IdleSample 'applications' $process $snapshot
}
$results | ConvertTo-Json | Set-Content (Join-Path $out results.json)
$results | Format-Table
Write-Output 'Warm local launches; window Loaded is not first painted frame or disk-cache-controlled cold start.'
