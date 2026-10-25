using Cleanup.Interop;
using Cleanup.Services;
using Cleanup.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Cleanup.Views;

public sealed partial class HistoryPage : Page
{
    private bool _busy;
    private bool _initialized;
    private HistoryItemViewModel[] _entries = [];
    public HistoryPage()
    {
        InitializeComponent();
        _initialized = true;
        Loaded += OnLoaded;
        Unloaded += (_, _) => SettingsService.Changed -= ApplyMode;
    }
    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        ApplyMode();
        SettingsService.Changed += ApplyMode;
        await RefreshAsync();
    }
    private void ApplyMode() => AuditExpander.Visibility = SettingsService.Current.AdvancedMode ? Visibility.Visible : Visibility.Collapsed;
    private async void OnRefreshClick(object sender, RoutedEventArgs e) => await RefreshAsync();
    private async void OnOlderClick(object sender, RoutedEventArgs e)
    {
        if (_entries.Length > 0) await RefreshAsync(before: _entries[^1].Entry.BatchId);
    }
    private async Task RefreshAsync(string? revealBatch = null, string? before = null)
    {
        if (_busy) return;
        string? selected = revealBatch ?? (HistoryList.SelectedItem as HistoryItemViewModel)?.Entry.BatchId;
        bool reopen = revealBatch is not null || DetailsView.IsPaneOpen;
        try
        {
            SetBusy(true);
            _entries = (await Features.GetHistoryAsync(before)).Select(entry => new HistoryItemViewModel(entry)).ToArray();
            ApplyFilter();
            if (reopen && _entries.FirstOrDefault(i => i.Entry.BatchId == selected) is { } item)
            {
                HistoryList.SelectedItem = item;
                ShowDetails(item);
            }
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't load history", error.Message); }
        finally { SetBusy(false); }
    }
    private void OnSearchChanged(object sender, TextChangedEventArgs e) { if (_initialized) ApplyFilter(); }
    private void OnFilterChanged(object sender, SelectionChangedEventArgs e) { if (_initialized) ApplyFilter(); }
    private void ApplyFilter()
    {
        string search = SearchBox.Text.Trim();
        IEnumerable<HistoryItemViewModel> view = _entries.Where(i => i.Matches(search));
        view = TypeBox.SelectedIndex switch { 1 => view.Where(i => i.Entry.Kind == "organize"), 2 => view.Where(i => i.Entry.Kind == "uninstall"), 3 => view.Where(i => i.Entry.Kind == "leftovers"), _ => view };
        view = StateBox.SelectedIndex switch { 1 => view.Where(i => i.NeedsAttention), 2 => view.Where(i => i.Entry.CanUndo), 3 => view.Where(i => i.Entry.Status is "undone" or "partiallyUndone"), _ => view };
        HistoryItemViewModel[] visible = view.ToArray();
        HistoryList.ItemsSource = visible;
        DetailsView.IsPaneOpen = false;
        DetailItems.ItemsSource = null;
        EmptyState.Visibility = visible.Length == 0 ? Visibility.Visible : Visibility.Collapsed;
        EmptyTitle.Text = _entries.Length == 0 ? "Your changes will appear here" : "No matching changes";
        EmptyDescription.Text = _entries.Length == 0 ? "Remove an app or organize a folder to get started." : "Try another search or filter.";
        HistoryCount.Text = $"{visible.Length} changes on this page. Search and filters apply to this page.";
    }
    private void OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (HistoryList.SelectedItem is HistoryItemViewModel item) ShowDetails(item);
    }
    private void OnItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is HistoryItemViewModel item) ShowDetails(item);
    }
    private void ShowDetails(HistoryItemViewModel item)
    {
        DetailTitle.Text = item.Title;
        DetailDate.Text = item.Date;
        DetailSummary.Text = $"{item.Status} · {item.Summary}";
        GuidanceText.Text = item.Guidance;
        MessageText.Text = item.Entry.Message ?? "";
        MessageText.Visibility = string.IsNullOrWhiteSpace(item.Entry.Message) ? Visibility.Collapsed : Visibility.Visible;
        AuditText.Text = item.Audit;
        DetailItems.ItemsSource = item.Entry.Items;
        UndoButton.Visibility = item.Entry.Kind == "organize" && item.Entry.CanUndo ? Visibility.Visible : Visibility.Collapsed;
        UndoButton.IsEnabled = !_busy && item.Entry.CanUndo;
        DetailsView.IsPaneOpen = true;
    }
    private void OnCloseDetailsClick(object sender, RoutedEventArgs e)
    {
        DetailsView.IsPaneOpen = false;
        DetailItems.ItemsSource = null;
        HistoryList.Focus(FocusState.Programmatic);
    }
    private async void OnUndoClick(object sender, RoutedEventArgs e)
    {
        if (_busy || HistoryList.SelectedItem is not HistoryItemViewModel item || !item.Entry.CanUndo) return;
        var confirm = new ContentDialog { Title = "Undo these file moves?", Content = new ScrollViewer { MaxHeight = 320, Content = new TextBlock { Text = item.Details, TextWrapping = TextWrapping.Wrap } }, XamlRoot = XamlRoot, RequestedTheme = ActualTheme,
            PrimaryButtonText = "Undo moves", CloseButtonText = "Cancel", DefaultButton = ContentDialogButton.Close };
        if (await confirm.ShowAsync() != ContentDialogResult.Primary) return;
        SetBusy(true);
        try
        {
            var result = await Features.UndoAsync(item.Entry.BatchId);
            OperationUpdates.PublishUndo(item.Entry.BatchId, result);
            Show(result.Conflicts.Length > 0 ? InfoBarSeverity.Warning : InfoBarSeverity.Success, $"Restored {result.Restored} files", string.Join("\n", result.Conflicts.Select(i => $"{i.FileName}: {i.Reason}")));
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't finish undo", error.Message); }
        finally { SetBusy(false); await RefreshAsync(item.Entry.BatchId); }
    }
    private void SetBusy(bool busy)
    {
        _busy = busy;
        HistoryLoading.IsActive = busy;
        RefreshButton.IsEnabled = HistoryList.IsEnabled = SearchBox.IsEnabled = TypeBox.IsEnabled = StateBox.IsEnabled = !busy;
        OlderButton.IsEnabled = !busy && _entries.Length == 20;
        UndoButton.IsEnabled = !busy && (HistoryList.SelectedItem as HistoryItemViewModel)?.Entry.CanUndo == true;
    }
    private void Show(InfoBarSeverity severity, string title, string message) { Status.Title = title; Status.Message = message; Status.Severity = severity; Status.IsOpen = true; }
}
