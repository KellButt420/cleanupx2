# Uses fresh namespaced fixtures only. Retains restored files for inspection.
# Run with the installed app open and leave its window alone during verification.
param([switch] $UndoThroughHistory)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ui-automation.ps1')
if (-not ('CleanupVerificationFolders' -as [type])) {
    Add-Type @"
using System; using System.Runtime.InteropServices;
public static class CleanupVerificationFolders {
    [DllImport("shell32.dll")] static extern int SHGetKnownFolderPath(ref Guid id, uint flags, IntPtr token, out IntPtr path);
    public static string Downloads() {
        var id = new Guid("374DE290-123F-4565-9164-39C4925E467B"); IntPtr path;
        int hr = SHGetKnownFolderPath(ref id, 0, IntPtr.Zero, out path);
        if (hr < 0) Marshal.ThrowExceptionForHR(hr);
        try { return Marshal.PtrToStringUni(path); } finally { Marshal.FreeCoTaskMem(path); }
    }
}
"@
}
function Set-Option([string] $Name, [string] $Value) {
    $box = Find-CleanupUi $Name ComboBox | Select-Object -First 1
    if (-not $box) { throw "Missing option: $Name" }
    $box.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 100
    $item = Find-CleanupUi $Value ListItem | Select-Object -First 1
    if (-not $item) { throw "Missing option value: $Value" }
    $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    $box.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
    Start-Sleep -Milliseconds 100
}
function Wait-Preview([string] $ExpectedSource, [string] $ExpectedDestination='') {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $path = Find-CleanupUi '*' Text FolderPath | Select-Object -First 1
        $move = Find-CleanupUi 'Move 5 files' Button ApplyButton | Select-Object -First 1
        $destinationsReady = -not $ExpectedDestination -or @(Find-CleanupUi '*' ListItem | Where-Object {$_.Current.Name.Contains($ExpectedDestination,[StringComparison]::OrdinalIgnoreCase)}).Count -eq 5
        if ($path -and $path.Current.Name -eq $ExpectedSource -and $move -and $move.Current.IsEnabled -and $destinationsReady) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'The expected disposable preview did not load; no move approved.'
}
function Assert-Unchanged {
    foreach ($file in $files.GetEnumerator()) {
        if (-not (Test-Path -LiteralPath (Join-Path $source $file.Key)) -or
            [IO.File]::ReadAllText((Join-Path $source $file.Key)) -ne $file.Value) { throw 'A source fixture changed before approval.' }
    }
    if (@(Get-ChildItem -LiteralPath $destination -File -Recurse).Count -ne 1) { throw 'A cancelled dialog moved files.' }
}
function Wait-Done([string] $Title) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $done = Find-CleanupUi $Title Text DoneTitle | Select-Object -First 1
        if ($done) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Operation did not finish: $Title"
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$source = Join-Path ([CleanupVerificationFolders]::Downloads()) ('CleanupVerification-' + [guid]::NewGuid().ToString('N'))
$destination = Join-Path $repoRoot ('artifacts\ui-fixtures\cross-volume-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $source, (Join-Path $destination Doc) | Out-Null
$files = [ordered]@{'unicode-日本.txt'='Unicode fixture'; 'book.pdf'='Doc fixture'; 'photo.jpeg'='Media fixture'; 'packed.zip'='Binary fixture'; 'misc.abc'='Others fixture'}
foreach ($file in $files.GetEnumerator()) { [IO.File]::WriteAllText((Join-Path $source $file.Key),$file.Value) }
$existing = Join-Path $destination 'Doc\unicode-日本.txt'
[IO.File]::WriteAllText($existing,'Existing file must not be overwritten')
Connect-CleanupUi (Get-Process Cleanup | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1).Id
Select-CleanupUi Organize
Start-Sleep -Milliseconds 300
if (Find-CleanupUi 'Change folder' Button) { Invoke-CleanupUi 'Change folder' }
if (Find-CleanupUi 'Organize another folder' Button) { Invoke-CleanupUi 'Organize another folder' }
Select-CleanupFolder $source
Wait-Preview $source
Invoke-CleanupUi 'Sorting options'
$original = @{}
foreach ($name in @('Sort into','Date folders','When a name is already taken')) {
    $box = Find-CleanupUi $name ComboBox | Select-Object -First 1
    $original[$name] = $box.GetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern).Current.GetSelection()[0].Current.Name
}
Set-Option 'Sort into' 'Doc, Media, Binary, Others'
Set-Option 'Date folders' None
Set-Option 'When a name is already taken' 'Ask me'
$more = Find-CleanupUi 'More options' Button | Select-Object -First 1
$more.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
Set-CleanupUiText 'Put the folders in' $destination
Invoke-CleanupUi Done PrimaryButton
Wait-Preview $source $destination
# First cancel at the name-conflict prompt, then at the exact-path confirmation.
Invoke-CleanupUi 'Move 5 files' ApplyButton
Invoke-CleanupUi Cancel CloseButton
Assert-Unchanged
Invoke-CleanupUi 'Move 5 files' ApplyButton
Invoke-CleanupUi 'Keep both' PrimaryButton
Invoke-CleanupUi Cancel CloseButton
Assert-Unchanged
Invoke-CleanupUi 'Move 5 files' ApplyButton
Invoke-CleanupUi 'Keep both' PrimaryButton
$deadline=[DateTime]::UtcNow.AddSeconds(4)
do {
    $confirmation = (Find-CleanupUi '*' Text | ForEach-Object { $_.Current.Name }) -join "`n"
    if ($confirmation.Contains($source) -and $confirmation.Contains($destination)) { break }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
if (-not $confirmation.Contains($source) -or -not $confirmation.Contains($destination)) { throw 'Exact fixture paths are missing; no move approved.' }
Invoke-CleanupUi 'Move files' PrimaryButton
Wait-Done 'Moved 5 files into 4 folders'
foreach ($file in $files.GetEnumerator()) {
    if (Test-Path -LiteralPath (Join-Path $source $file.Key)) { throw 'A selected source fixture remained.' }
    $match = @(Get-ChildItem -LiteralPath $destination -File -Recurse | Where-Object { [IO.File]::ReadAllText($_.FullName) -eq $file.Value })
    if ($match.Count -ne 1) { throw 'Moved fixture content mismatch.' }
}
foreach ($group in @('Doc','Media','Binary','Others')) {
    if (-not (Test-Path -LiteralPath (Join-Path $destination $group) -PathType Container)) { throw "Missing category: $group" }
}
if ([IO.File]::ReadAllText($existing) -ne 'Existing file must not be overwritten') { throw 'Existing collision target changed.' }
if ($UndoThroughHistory) {
    Select-CleanupUi History
    Start-Sleep -Milliseconds 350
    $sourceName = Split-Path $source -Leaf
    Set-CleanupUiText 'Search history' $sourceName
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    do {
        $row = Find-CleanupUi ("Organized " + $sourceName + '*') ListItem | Select-Object -First 1
        if($row){break};Start-Sleep -Milliseconds 100
    } while([DateTime]::UtcNow -lt $deadline)
    if (-not $row) { throw 'Dedicated fixture history was not found; no undo approved.' }
    $row.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Invoke-CleanupUi 'Undo file moves'
    Invoke-CleanupUi 'Undo moves' PrimaryButton
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        Start-Sleep -Milliseconds 100
        $restored=Find-CleanupUi 'Restored*' Text DetailSummary | Select-Object -First 1
    } while (-not $restored -and [DateTime]::UtcNow -lt $deadline)
    if (-not $restored) { throw 'History did not report the restored fixture.' }
    Invoke-CleanupUi 'Close history details'
    Set-CleanupUiText 'Search history' ''
    Select-CleanupUi Organize
} else {
    Invoke-CleanupUi Undo
    Invoke-CleanupUi 'Undo moves' PrimaryButton
    Wait-Done 'Put back 5 files'
}
Assert-Unchanged
# Restore persisted sorting preferences; source/destination are page-local only.
Invoke-CleanupUi 'Organize another folder'
Select-CleanupFolder $source
Wait-Preview $source
Invoke-CleanupUi 'Sorting options'
foreach ($name in $original.Keys) { Set-Option $name $original[$name] }
$more = Find-CleanupUi 'More options' Button | Select-Object -First 1
$more.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
Set-CleanupUiText 'Put the folders in' ''
Invoke-CleanupUi Done PrimaryButton
Start-Sleep -Milliseconds 500
Invoke-CleanupUi 'Change folder'
[ordered]@{passed=$true; source=$source; destination=$destination; crossVolume=([IO.Path]::GetPathRoot($source) -ne [IO.Path]::GetPathRoot($destination)); checks=@('native picker','four groups','collision prompt cancellation','exact confirmation cancellation','no overwrite','Unicode content','undo')} |
    ConvertTo-Json | Set-Content (Join-Path $repoRoot 'artifacts\ui-fixtures\cross-volume-result.json')
Write-Host 'PASS: native folder picker, four groups, cancellation at both approval dialogs, collision preservation, Unicode move and undo.'
Write-Host "Restored fixtures retained for inspection: $source"
