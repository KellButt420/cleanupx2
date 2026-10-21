using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;
using Cleanup.Interop;
using Cleanup.Services;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;
using WinRT.Interop;

namespace Cleanup;

public sealed partial class FeasibilityWindow : Window
{
    private readonly SelfTestOptions _options;
    private readonly ObservableCollection<AppRecord> _apps = [];
    private CancellationTokenSource? _cancellation;
    private double _startupToLoadedMs;

    internal FeasibilityWindow(SelfTestOptions options)
    {
        _options = options;
        InitializeComponent();
        AppWindow.Resize(new SizeInt32(1100, 720));
        AppList.ItemsSource = _apps;
        StatusText.Text = AppEnvironment.IsPackaged
            ? $"Packaged: {AppEnvironment.PackageFullName}\nApp data: {AppEnvironment.LocalStateDirectory}"
            : $"Not packaged (development run). App data: {AppEnvironment.LocalStateDirectory}";
    }

    private async void OnRootLoaded(object sender, RoutedEventArgs e)
    {
        _startupToLoadedMs = (DateTime.Now - Process.GetCurrentProcess().StartTime).TotalMilliseconds;
        Log($"Window loaded {_startupToLoadedMs:F0} ms after process start.");
        try
        {
            NativeCore.EnsureCompatible();
            Log($"Native core loaded (ABI {NativeCore.AbiVersion}).");
        }
        catch (Exception ex)
        {
            Log($"Native core unavailable: {ex.Message}");
            ScanButton.IsEnabled = ProbeButton.IsEnabled = false;
        }

        if (_options.Enabled)
        {
            await RunSelfTestAsync();
        }
    }

    private async Task RunSelfTestAsync()
    {
        // Disable input without starting the progress animation, so the idle
        // measurement reflects the resting UI.
        ScanButton.IsEnabled = ProbeButton.IsEnabled = RemoveButton.IsEnabled = false;
        var runner = new FeasibilityRunner(_options, WindowNative.GetWindowHandle(this), Log);
        JsonObject report;
        try
        {
            report = await runner.RunAsync(_startupToLoadedMs);
        }
        catch (Exception ex)
        {
            report = new JsonObject { ["fatal"] = ex.ToString() };
        }

        if (_options.ReportPath is { } path)
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
            await File.WriteAllTextAsync(path, report.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        }
        Application.Current.Exit();
    }

    private async void OnScanClick(object sender, RoutedEventArgs e)
    {
        _cancellation = new CancellationTokenSource();
        SetBusy(true, cancellable: true);
        _apps.Clear();
        var stopwatch = Stopwatch.StartNew();
        try
        {
            InventoryScanResult result = await NativeCore.ScanInventoryAsync(
                batch => DispatcherQueue.TryEnqueue(() =>
                {
                    foreach (AppRecord app in batch)
                    {
                        _apps.Add(app);
                    }
                }),
                _cancellation.Token);
            Log($"Scan finished: {result.Records.Count} apps, {result.Batches} batches, {stopwatch.ElapsedMilliseconds} ms.");
            Log(result.SummaryJson);
        }
        catch (Exception ex)
        {
            Log($"Scan failed: {ex.Message}");
        }
        finally
        {
            SetBusy(false, cancellable: false);
        }
    }

    private async void OnProbeClick(object sender, RoutedEventArgs e)
    {
        SetBusy(true, cancellable: false);
        try
        {
            string stateDirectory = AppEnvironment.LocalStateDirectory;
            await Task.Run(() =>
            {
                Log(NativeCore.ProbeEnvironment());
                Log(NativeCore.ProbePersistence(stateDirectory));
                string moves = Path.Combine(stateDirectory, "probe-file-moves");
                Directory.CreateDirectory(moves);
                Log(NativeCore.ProbeFileMoves(moves));
            });
        }
        catch (Exception ex)
        {
            Log($"Probe failed: {ex.Message}");
        }
        finally
        {
            SetBusy(false, cancellable: false);
        }
    }

    private async void OnRemoveClick(object sender, RoutedEventArgs e)
    {
        if (AppList.SelectedItem is not AppRecord app || !FeasibilityRunner.IsFixture(app))
        {
            return;
        }

        var dialog = new ContentDialog
        {
            Title = $"Remove {app.DisplayName}?",
            Content = "This runs the application's own uninstaller. Windows may ask for administrator approval.",
            PrimaryButtonText = "Remove",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Root.XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }

        _cancellation = new CancellationTokenSource();
        SetBusy(true, cancellable: true);
        try
        {
            string report = await NativeCore.UninstallAsync(
                app.Id,
                allowElevation: true,
                WindowNative.GetWindowHandle(this),
                stage => Log($"{app.DisplayName}: {stage}"),
                _cancellation.Token);
            Log(report);
        }
        catch (Exception ex)
        {
            Log($"Uninstall failed: {ex.Message}");
        }
        finally
        {
            SetBusy(false, cancellable: false);
        }
    }

    private void OnCancelClick(object sender, RoutedEventArgs e) => _cancellation?.Cancel();

    private void OnAppSelectionChanged(object sender, SelectionChangedEventArgs e) =>
        RemoveButton.IsEnabled = !Busy.IsActive && AppList.SelectedItem is AppRecord app && FeasibilityRunner.IsFixture(app);

    private void SetBusy(bool busy, bool cancellable)
    {
        Busy.IsActive = busy;
        ScanButton.IsEnabled = ProbeButton.IsEnabled = !busy;
        RemoveButton.IsEnabled = !busy && AppList.SelectedItem is AppRecord app && FeasibilityRunner.IsFixture(app);
        CancelButton.IsEnabled = busy && cancellable;
    }

    private void Log(string message)
    {
        if (!DispatcherQueue.HasThreadAccess)
        {
            DispatcherQueue.TryEnqueue(() => Log(message));
            return;
        }
        LogBox.Text += $"[{DateTime.Now:HH:mm:ss}] {message}{Environment.NewLine}";
        LogBox.SelectionStart = LogBox.Text.Length;
    }
}
