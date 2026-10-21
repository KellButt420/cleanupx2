# Dot-source for local verification of Cleanup's native window. Scoped to one
# process/window and its owned OS folder picker; no unrelated UI is inspected.
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
if (-not ('CleanupUiNative' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CleanupUiNative {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent, IntPtr child);
    [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr control);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, System.Text.StringBuilder name, int count);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam, string text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
}
'@
}
$script:UiElement = [System.Windows.Automation.AutomationElement]
$script:UiScope = [System.Windows.Automation.TreeScope]::Descendants
function Connect-CleanupUi([int] $ProcessId) {
    $script:UiProcess = Get-Process -Id $ProcessId -ErrorAction Stop
    if ($script:UiProcess.ProcessName -ne 'Cleanup') { throw 'Only Cleanup windows may be inspected.' }
    $script:UiWindow = $script:UiElement::FromHandle($script:UiProcess.MainWindowHandle)
}
function Find-CleanupUi([string] $Name, [string] $Type = '', [string] $Id = '') {
    # WinUI rebuilds the provider tree while a dialog/dropdown transitions.
    for ($attempt=0; ; $attempt++) {
        try { $all = $script:UiWindow.FindAll($script:UiScope, [System.Windows.Automation.Condition]::TrueCondition); break }
        catch { if ($attempt -ge 4) { throw }; Start-Sleep -Milliseconds 100 }
    }
    @($all | Where-Object { ($Name -eq '' -or $_.Current.Name -like $Name) -and ($Type -eq '' -or $_.Current.ControlType.ProgrammaticName -eq "ControlType.$Type") -and ($Id -eq '' -or $_.Current.AutomationId -eq $Id) })
}
function Invoke-CleanupUi([string] $Name, [string] $Id = '') {
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        $element = Find-CleanupUi $Name 'Button' $Id | Where-Object { $_.Current.IsEnabled } | Select-Object -First 1
        if ($element) { break }; Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $element) { throw "Button not found: $Name [$Id]" }
    $pattern=$null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) { $pattern.Invoke() }
    elseif ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$pattern)) { $pattern.Toggle() }
    elseif ($element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern,[ref]$pattern)) { $pattern.Expand() }
    else { throw "No supported button action: $Name" }
}
function Set-CleanupUiText([string] $Name, [string] $Value, [string] $Id = '') {
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        $element = Find-CleanupUi $Name 'Edit' $Id | Select-Object -First 1
        if ($element) { break }; Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $element) { throw "Text field not found: $Name [$Id]" }
    $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
}
function Select-CleanupUi([string] $Name) {
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        $element = Find-CleanupUi $Name 'ListItem' | Select-Object -First 1
        if ($element) { break }; Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $element) { throw "Item not found: $Name" }
    $element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
}

# The OS folder picker is an owned modal window, possibly hosted in another
# process. Some Windows builds expose its native Edit/Button as UIA Panes with
# no patterns. Use their validated Win32 controls, scoped strictly to this picker.
function Select-CleanupFolder([string] $Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw 'Fixture source folder does not exist.' }
    $owner = $script:UiProcess.MainWindowHandle
    $popup = [CleanupUiNative]::GetWindow($owner, 6)
    if ($popup -eq $owner -or $popup -eq [IntPtr]::Zero) { Invoke-CleanupUi 'Choose folder' }
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        $popup = [CleanupUiNative]::GetWindow($owner, 6)
        if ($popup -ne $owner -and $popup -ne [IntPtr]::Zero -and [CleanupUiNative]::GetWindow($popup, 4) -eq $owner) {
            $candidate = $script:UiElement::FromHandle($popup)
            if ($candidate.Current.Name -eq 'Select Folder') { break }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($popup -eq [IntPtr]::Zero -or $popup -eq $owner -or [CleanupUiNative]::GetWindow($popup, 4) -ne $owner) { throw 'No folder picker owned by Cleanup.' }
    $dialog = $script:UiElement::FromHandle($popup)
    if ($dialog.Current.Name -ne 'Select Folder') { throw 'Unexpected owned dialog; no interaction performed.' }
    $controls = $dialog.FindAll($script:UiScope, [System.Windows.Automation.Condition]::TrueCondition)
    $edit = $controls | Where-Object { $_.Current.AutomationId -eq '1152' -and $_.Current.ClassName -eq 'Edit' } | Select-Object -First 1
    $button = $controls | Where-Object { $_.Current.AutomationId -eq '1' -and $_.Current.ClassName -eq 'Button' } | Select-Object -First 1
    if (-not $edit -or -not $button) { throw 'Native folder controls are unavailable.' }
    foreach ($control in @($edit,$button)) {
        if (-not [CleanupUiNative]::IsChild($popup, [IntPtr]$control.Current.NativeWindowHandle)) { throw 'Control is outside the folder picker.' }
    }
    $result = [IntPtr]::Zero
    if ([CleanupUiNative]::SendMessageTimeout([IntPtr]$edit.Current.NativeWindowHandle, 12, [IntPtr]::Zero, $Path, 2, 3000, [ref]$result) -eq [IntPtr]::Zero) { throw 'Could not enter the fixture path.' }
    if (-not [CleanupUiNative]::PostMessage([IntPtr]$button.Current.NativeWindowHandle, 245, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Could not confirm folder selection.' }
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $active = [CleanupUiNative]::GetWindow($owner, 6)
    } while ($active -ne $owner -and $active -ne [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline)
    if ($active -ne $owner -and $active -ne [IntPtr]::Zero) { throw 'Folder picker did not close.' }
}
function Save-CleanupUiScreenshot([string] $Path, [switch] $Screen) {
    $script:UiProcess.Refresh()
    $rect = [CleanupUiNative+Rect]::new()
    [CleanupUiNative]::GetWindowRect($script:UiProcess.MainWindowHandle, [ref]$rect) | Out-Null
    $bitmap = [System.Drawing.Bitmap]::new($rect.Right-$rect.Left, $rect.Bottom-$rect.Top)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        if ($Screen) {
            $focusTarget = Find-CleanupUi -Name '*' -Type 'Edit' | Where-Object { $_.Current.IsKeyboardFocusable -and -not $_.Current.IsOffscreen } | Select-Object -First 1
            if ($focusTarget) { $focusTarget.SetFocus() }
            [CleanupUiNative]::SetForegroundWindow($script:UiProcess.MainWindowHandle) | Out-Null
            Start-Sleep -Milliseconds 500
            if ([CleanupUiNative]::GetForegroundWindow() -ne $script:UiProcess.MainWindowHandle) { throw 'Cleanup is not foreground; screen capture refused.' }
            $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        } else {
            $dc = $graphics.GetHdc()
            try { $ok = [CleanupUiNative]::PrintWindow($script:UiProcess.MainWindowHandle, $dc, 2) }
            finally { $graphics.ReleaseHdc($dc) }
            if (-not $ok) { throw 'PrintWindow failed.' }
        }
        $bitmap.Save($Path)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
