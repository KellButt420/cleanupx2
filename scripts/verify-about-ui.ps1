# Non-destructive native UI check. No uninstall or filesystem operation is executed.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ui-automation.ps1')
Connect-CleanupUi (Get-Process Cleanup | Select-Object -First 1 -ExpandProperty Id)
Select-CleanupUi Settings
# Wait for the page to finish loading (a cold start can take a moment).
$deadline = [DateTime]::UtcNow.AddSeconds(5)
do {
    Start-Sleep -Milliseconds 200
    $reference = Find-CleanupUi -Name '*' -Id ThemeBox | Select-Object -First 1
} while (-not $reference -and [DateTime]::UtcNow -lt $deadline)
if (-not $reference) { throw 'Settings did not load.' }
foreach ($id in @('CustomAccentToggle','AdvancedToggle')) {
    $toggle = Find-CleanupUi -Name '*' -Id $id | Select-Object -First 1
    if (-not $toggle -or [math]::Abs($toggle.Current.BoundingRectangle.Width-40) -gt 1 -or
        # ComboBox UIA bounds include four extra pixels for its focus visual.
        [math]::Abs($toggle.Current.BoundingRectangle.Right-$reference.Current.BoundingRectangle.Right) -gt 5) {
        throw "Switch is not compact and aligned to the right: $id"
    }
}
Invoke-CleanupUi 'luh geek'
Start-Sleep -Milliseconds 600  # let the fade-in finish
foreach ($name in @('forthescene','a (wired-in) project',[DateTime]::Now.Year.ToString())) {
    if (-not (Find-CleanupUi -Name $name -Type Text)) { throw "Missing credits text: $name" }
}
$navigation = Find-CleanupUi -Name Applications -Type ListItem
if ($navigation -and -not $navigation[0].Current.IsOffscreen) { throw 'Sidebar should be hidden.' }
$logo = Find-CleanupUi -Name 'forthescene logo' -Type Image | Select-Object -First 1
$window = $script:UiWindow.Current.BoundingRectangle
if (-not $logo -or [math]::Abs(($logo.Current.BoundingRectangle.Left+$logo.Current.BoundingRectangle.Width/2)-($window.Left+$window.Width/2)) -gt 2) {
    throw 'Credits logo should be centered in the full window.'
}
# Back sits in the title-bar row, where the app icon is on other pages.
$back = Find-CleanupUi -Name 'Back to Settings' -Type Button | Select-Object -First 1
if (-not $back) { throw 'Missing title-bar Back button.' }
$backCenterY = $back.Current.BoundingRectangle.Top + $back.Current.BoundingRectangle.Height / 2
$dpi = (Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -ErrorAction SilentlyContinue).AppliedDPI
$rowHeight = 48 * ($(if ($dpi) { $dpi } else { 96 }) / 96)
if ($backCenterY -gt $window.Top + $rowHeight) { throw "Back button is below the title bar (center y=$backCenterY, title bar ends at $($window.Top + $rowHeight))." }
if ($back.Current.BoundingRectangle.Left - $window.Left -gt 40 * $rowHeight / 48) { throw 'Back button is not at the left edge where the app icon sits.' }
Invoke-CleanupUi 'Back to Settings'
Start-Sleep -Milliseconds 600
if (-not (Find-CleanupUi -Name '*' -Id CustomAccentToggle)) { throw 'Back must return to Settings.' }
if (Find-CleanupUi -Name 'About Cleanup' -Type Button) { throw 'The old About Cleanup button should be gone.' }
Select-CleanupUi Applications
Set-CleanupUiText 'Search apps' ''
$deadline=[DateTime]::UtcNow.AddSeconds(5)
do {
    $rows=@(Find-CleanupUi -Name '*' -Type ListItem | Where-Object {$_.Current.Name -notin @('Applications','Organize','History','Settings')})
    if ($rows.Count -ge 9) { break }
    Start-Sleep -Milliseconds 100
} while ([DateTime]::UtcNow -lt $deadline)
if ($rows.Count -lt 9) { throw 'Need nine visible inventory rows.' }
$rows[0].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
for ($i=1; $i -lt 9; $i++) {
    $rows[$i].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).AddToSelection()
    if ($i -ge 6) {
        $overflow=Find-CleanupUi -Name '*' -Id SelectionOverflow | Select-Object -First 1
        if (-not $overflow -or $overflow.Current.Name -ne (($i-5).ToString()+' more selected applications')) { throw 'Wrong additional-selection count.' }
    }
}
Invoke-CleanupUi 'Clear application selection'
Write-Output 'Passed: compact right-aligned switches; luh geek opens the full-window credits; centered logo/text; Back in the title-bar row returns to Settings; +1/+2/+3 selection counts.'
