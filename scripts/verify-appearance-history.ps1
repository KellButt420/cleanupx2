# Read-only history and reversible appearance checks. No file operations or uninstallers run.
param([int] $Repetitions=3)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'ui-automation.ps1')
Connect-CleanupUi (Get-Process Cleanup | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1).Id
function Get-Box([string] $Id) {
    $deadline=[DateTime]::UtcNow.AddSeconds(4)
    do {
        $box=Find-CleanupUi '*' ComboBox $Id | Select-Object -First 1
        if($box){return $box}
        Start-Sleep -Milliseconds 100
    } while([DateTime]::UtcNow -lt $deadline)
    throw "Control did not become ready: $Id"
}
function Choose([string] $Id,[string] $Value) {
    Add-Content (Join-Path $PSScriptRoot '../artifacts/appearance-steps.log') ('{0:o} {1}={2}' -f [DateTime]::Now,$Id,$Value)
    $box=Get-Box $Id
    $box.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Expand()
    Start-Sleep -Milliseconds 220
    $item=Find-CleanupUi $Value ListItem | Select-Object -Last 1
    if(-not $item){throw "Missing value: $Value"}
    $scroll=$null
    if($item.TryGetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern,[ref]$scroll)){$scroll.ScrollIntoView()}
    $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 120
    $box=Get-Box $Id
    $box.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern).Collapse()
    Start-Sleep -Milliseconds 220
    if((CurrentChoice $Id) -ne $Value){throw "Selection did not apply: $Id = $Value"}
}
function CurrentChoice([string] $Id) {
    (Get-Box $Id).GetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern).Current.GetSelection()[0].Current.Name
}
if($script:UiWindow.Current.Name -ne 'cleanup'){throw 'Window title must be lowercase cleanup.'}
Select-CleanupUi Settings
Start-Sleep -Milliseconds 300
$original=@{theme=(CurrentChoice ThemeBox);backdrop=(CurrentChoice BackdropBox);tint=(CurrentChoice TintPresetBox)}
$initialSettings=Get-Content (Join-Path $env:LOCALAPPDATA 'Packages/Cleanup.Dev_kfjrmgg4yenmj/LocalState/settings.json') -Raw | ConvertFrom-Json
Choose BackdropBox Acrylic
for($repeat=0;$repeat -lt $Repetitions;$repeat++) {
foreach($preset in @('Windows default','Graphite','Violet','Ocean','Forest','Rose','Windows accent','Custom color')){
    Choose TintPresetBox $preset
    if($preset -ne 'Windows default' -and -not (Find-CleanupUi '*' Slider TintStrengthSlider)){throw 'Missing tint strength slider.'}
}
}
$probeColor=if($initialSettings.acrylicTintColor -eq '#284660'){'#5B304D'}else{'#284660'}
$probeStrength=if($initialSettings.acrylicTintStrength -eq 72){53}else{72}
Set-CleanupUiText 'Acrylic tint color' $probeColor
Invoke-CleanupUi 'Apply tint'
$slider=Find-CleanupUi '*' Slider TintStrengthSlider | Select-Object -First 1
$slider.GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue($probeStrength)
Start-Sleep -Milliseconds 300
$saved=Get-Content (Join-Path $env:LOCALAPPDATA 'Packages/Cleanup.Dev_kfjrmgg4yenmj/LocalState/settings.json') -Raw | ConvertFrom-Json
if($saved.acrylicTintColor -ne $probeColor -or $saved.acrylicTintStrength -ne $probeStrength){throw 'Custom tint settings were not persisted.'}
for($repeat=0;$repeat -lt $Repetitions;$repeat++) {
foreach($theme in @('Light','Dark')) {
    Choose ThemeBox $theme
    foreach($material in @('Mica','Solid','Acrylic')) {Choose BackdropBox $material}
}
}
Choose TintPresetBox 'Windows default'
Choose TintPresetBox 'Custom color'
Invoke-CleanupUi 'Choose color'
$deadline=[DateTime]::UtcNow.AddSeconds(5)
do {
    $spectrum=Find-CleanupUi '*' | Where-Object {$_.Current.ClassName -like '*ColorSpectrum*'}
    if($spectrum){break};Start-Sleep -Milliseconds 100
} while([DateTime]::UtcNow -lt $deadline)
if(-not $spectrum){throw 'Native color picker did not open.'}
Invoke-CleanupUi 'Apply color' PrimaryButton
Invoke-CleanupUi 'Choose color'
Invoke-CleanupUi Cancel CloseButton
Set-CleanupUiText 'Acrylic tint color' $initialSettings.acrylicTintColor
Invoke-CleanupUi 'Apply tint'
$slider=Find-CleanupUi '*' Slider TintStrengthSlider | Select-Object -First 1
$slider.GetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern).SetValue($initialSettings.acrylicTintStrength)
Choose TintPresetBox $original.tint
Choose ThemeBox $original.theme
Choose BackdropBox $original.backdrop

Select-CleanupUi History
Start-Sleep -Milliseconds 350
Set-CleanupUiText 'Search history' ('no-fixture-matches-'+[guid]::NewGuid().ToString('N'))
if(-not (Find-CleanupUi 'No matching changes' Text)){throw 'History search did not show its empty state.'}
Set-CleanupUiText 'Search history' ''
Choose TypeBox 'File moves'
Choose StateBox Restored
$list=Find-CleanupUi '*' List HistoryList | Select-Object -First 1
$rows=$list.FindAll([System.Windows.Automation.TreeScope]::Children,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::ListItem))
if($rows.Count -eq 0){throw 'Expected restored disposable organizer history.'}
$rows[0].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
Start-Sleep -Milliseconds 150
if(-not (Find-CleanupUi 'Close history details' Button)){throw 'History detail drawer did not open.'}
if(Find-CleanupUi 'Undo file moves' Button | Where-Object {-not $_.Current.IsOffscreen -and $_.Current.IsEnabled}){throw 'An already restored batch must not offer undo.'}
Invoke-CleanupUi 'Close history details'
Choose TypeBox 'All changes'
Choose StateBox 'Any status'
Write-Output 'PASS: lowercase title, eight tint choices, custom tint persistence, slider persistence, theme/material switches, history search/filter/detail/undo eligibility.'
