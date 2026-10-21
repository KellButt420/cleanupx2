param([string] $Label='baseline',[int] $Cycles=10,[switch] $CreditsFrames)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'ui-automation.ps1')
Connect-CleanupUi (Get-Process Cleanup | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1).Id
$out=Join-Path (Resolve-Path (Join-Path $PSScriptRoot '../artifacts')).Path ('performance/'+$Label)
New-Item -ItemType Directory -Path $out -Force | Out-Null
$script:Samples=@()
function Sample([string] $Stage) {
    $script:UiProcess.Refresh()
    $script:Samples += [pscustomobject]@{stage=$Stage;workingSetMiB=[math]::Round($script:UiProcess.WorkingSet64/1MB,2);privateMiB=[math]::Round($script:UiProcess.PrivateMemorySize64/1MB,2);handles=$script:UiProcess.HandleCount;threads=$script:UiProcess.Threads.Count;cpuSeconds=$script:UiProcess.CPU}
}
Sample initial
for($i=1;$i -le $Cycles;$i++) {
    foreach($page in @('Settings','History','Organize','Photos','Videos','Applications','Core')) { Select-CleanupUi $page; Start-Sleep -Milliseconds 70 }
    Sample "cycle-$i"
}
$script:Samples | ConvertTo-Json | Set-Content (Join-Path $out navigation.json)
$script:Samples | Format-Table
if($CreditsFrames) {
    Select-CleanupUi Settings
    Invoke-CleanupUi 'luh geek'
    Start-Sleep -Milliseconds 700
    $logo=(Find-CleanupUi 'forthescene logo' Image | Select-Object -First 1).Current.BoundingRectangle
    $win=$script:UiWindow.Current.BoundingRectangle
    [ordered]@{x=$logo.Left-$win.Left;y=$logo.Top-$win.Top;width=$logo.Width;height=$logo.Height} | ConvertTo-Json | Set-Content (Join-Path $out logo-roi.json)
    Invoke-CleanupUi 'Back to Settings'
    Start-Sleep -Milliseconds 600
    [CleanupUiNative]::SetForegroundWindow($script:UiProcess.MainWindowHandle) | Out-Null
    $button=Find-CleanupUi 'luh geek' Button | Select-Object -First 1
    $rect=[CleanupUiNative+Rect]::new()
    [CleanupUiNative]::GetWindowRect($script:UiProcess.MainWindowHandle,[ref]$rect) | Out-Null
    $timing=[Diagnostics.Stopwatch]::StartNew()
    $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $frames=@()
    foreach($target in @(0,50,120,240,400,700)) {
        while($timing.ElapsedMilliseconds -lt $target) { Start-Sleep -Milliseconds 1 }
        if([CleanupUiNative]::GetForegroundWindow() -ne $script:UiProcess.MainWindowHandle){throw 'Only the foreground cleanup window may be captured.'}
        $bitmap=[Drawing.Bitmap]::new($rect.Right-$rect.Left,$rect.Bottom-$rect.Top)
        $graphics=[Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($rect.Left,$rect.Top,0,0,$bitmap.Size)
            $stamp=$timing.ElapsedMilliseconds
            $bitmap.Save((Join-Path $out "fade-$target.png"))
            $frames += [pscustomobject]@{target=$target;actual=$stamp}
        } finally {$graphics.Dispose();$bitmap.Dispose()}
    }
    $frames | ConvertTo-Json | Set-Content (Join-Path $out fade-times.json)
    Invoke-CleanupUi 'Back to Settings'
}
