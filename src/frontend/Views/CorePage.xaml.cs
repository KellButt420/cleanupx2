using Cleanup.Services;
using Cleanup.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace Cleanup.Views;

/// <summary>Home page: connected and network drives, and combined used and free space.</summary>
public sealed partial class CorePage : Page
{
    private static readonly TimeSpan RingSweep = TimeSpan.FromMilliseconds(900);
    private static readonly TimeSpan BarGrow = TimeSpan.FromMilliseconds(700);
    private static readonly TimeSpan CardStagger = TimeSpan.FromMilliseconds(45);

    private bool _refreshing;
    private DriveItemViewModel[] _drives = [];

    public CorePage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
    }

    // Drives are added and removed while the page is hidden, so each visit reads them again. The ring starts
    // empty so that every visit sweeps in from zero.
    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        Ring.Snap(0);
        await RefreshAsync();
    }

    private async void OnRefreshClick(object sender, RoutedEventArgs e) => await RefreshAsync();

    private async Task RefreshAsync()
    {
        if (_refreshing) return;
        _refreshing = true;
        RefreshButton.IsEnabled = false;
        SummaryText.Text = "Checking drives...";
        try
        {
            DriveItemViewModel[] drives = await StorageService.ReadAsync();
            _drives = drives;
            DriveList.ItemsSource = drives;
            ShowTotals(StorageTotals.From(drives));
        }
        catch (Exception error)
        {
            SummaryText.Text = $"Couldn't read drives: {error.Message}";
        }
        finally
        {
            _refreshing = false;
            RefreshButton.IsEnabled = true;
        }
    }

    private void ShowTotals(StorageTotals totals)
    {
        string free = Ui.Size(totals.Free);
        string total = Ui.Size(totals.Total);
        int percent = (int)Math.Round(totals.UsedFraction * 100);
        Ring.SweepTo(totals.UsedFraction, RingSweep);
        // The free space counts up with the sweep. The last step is the exact figure.
        if (Motion.Enabled)
        {
            Motion.Tween(FreeText, FreeText.DispatcherQueue, RingSweep, TimeSpan.Zero,
                progress => FreeText.Text = Ui.Size((ulong)(totals.Free * progress)));
        }
        else
        {
            FreeText.Text = free;
        }
        UsedText.Text = $"Used {Ui.Size(totals.Used)} ({percent}%)";
        FreeLegendText.Text = $"Free {free}";
        TotalText.Text = $"Total {total}";
        AutomationProperties.SetName(Ring, $"{percent}% used. {free} free of {total}.");
        SummaryText.Text = totals.Drives == 0
            ? "No ready drives found."
            : $"{totals.Drives} {(totals.Drives == 1 ? "drive" : "drives")} combined"
                + (totals.NetworkDrives > 0 ? $", including {totals.NetworkDrives} network" : "");
    }

    // Each drive card rises in turn, and its usage bar then grows from empty to its share.
    private void OnDriveElementPrepared(ItemsRepeater sender, ItemsRepeaterElementPreparedEventArgs args)
    {
        if (args.Element is not FrameworkElement card) return;
        TimeSpan delay = TimeSpan.FromMilliseconds(Math.Min(args.Index, 7) * CardStagger.TotalMilliseconds);
        Motion.Enter(card, delay: delay);
        if (args.Index >= _drives.Length || !_drives[args.Index].ShowsBar || card.FindName("UsageBar") is not ProgressBar bar) return;
        double share = _drives[args.Index].UsedPercent;
        if (!Motion.Enabled)
        {
            bar.Value = share;
            return;
        }
        bar.Value = 0;
        Motion.Tween(bar, bar.DispatcherQueue, BarGrow, delay + TimeSpan.FromMilliseconds(120), progress => bar.Value = share * progress);
    }
}
