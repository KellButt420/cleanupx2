using Cleanup.Interop;
using Cleanup.Services;
using Cleanup.ViewModels;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Windows.System;
using WinRT.Interop;

namespace Cleanup.Views;

public sealed partial class ApplicationsPage : Page
{
    private bool _restoringSelection;
    private int _itemsRevision;

    public ApplicationsPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += (_, _) =>
        {
            _itemsRevision++;
            _restoringSelection = false;
            ViewModel.PropertyChanging -= OnViewModelChanging;
            ViewModel.PropertyChanged -= OnViewModelChanged;
        };
    }

    public ApplicationsViewModel ViewModel => ApplicationsViewModel.Shared;

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        ViewModel.PropertyChanging += OnViewModelChanging;
        ViewModel.PropertyChanged += OnViewModelChanged;
        RestoreSelection();
        await ViewModel.EnsureLoadedAsync();
    }

    private void OnViewModelChanging(object? sender, PropertyChangingEventArgs e)
    {
        if (e.PropertyName == nameof(ApplicationsViewModel.Items)) _restoringSelection = true;
    }

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(ApplicationsViewModel.Items)) return;
        int revision = ++_itemsRevision;
        // Let the list finish replacing its source before restoring the shared selection.
        // SelectionChanged can otherwise arrive after the property notification returns.
        DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, () =>
        {
            if (revision != _itemsRevision) return;
            try { RestoreSelection(); }
            finally { _restoringSelection = false; }
        });
    }

    private void OnTableSizeChanged(object sender, SizeChangedEventArgs e) => TableContent.Width = Math.Max(820, e.NewSize.Width);

    private void OnColumnSortClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: string column } && int.TryParse(column, out int index))
            ViewModel.SortByColumn(index);
    }

    // Each visit builds a new page with an empty list, but the selection lives in the shared view model.
    private void RestoreSelection()
    {
        IReadOnlyList<AppItemViewModel> selected = ViewModel.SelectedItems;
        if (selected.Count == 0)
        {
            return;
        }
        HashSet<string> ids = selected.Select(item => item.Record.Id).ToHashSet();
        AppItemViewModel[] restored = ViewModel.Items.Where(item => ids.Contains(item.Record.Id)).ToArray();
        _restoringSelection = true;
        try
        {
            foreach (AppItemViewModel item in restored)
            {
                AppList.SelectedItems.Add(item);
            }
        }
        finally
        {
            _restoringSelection = false;
        }
        // The shared selection already holds these apps. Publishing it again re-raises every panel property,
        // so it is only corrected when a selected app is no longer listed.
        if (restored.Length != selected.Count)
        {
            ViewModel.UpdateSelection(restored);
        }
    }

    private void OnContainerContentChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (!args.InRecycleQueue && args.Item is AppItemViewModel item)
        {
            item.EnsureIcon();
        }
    }

    private async void OnRemoveMenuClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is AppItemViewModel item)
        {
            await ConfirmAndRemoveAsync([item]);
        }
    }

    // Right-clicking a row opens its menu at the pointer, rather than relying on the list to pass the gesture on.
    private void OnRowRightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (sender is FrameworkElement row && row.ContextFlyout is { } menu)
        {
            menu.ShowAt(row, new FlyoutShowOptions { Position = e.GetPosition(row) });
            e.Handled = true;
        }
    }

    private async void OnOpenMenuClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is AppItemViewModel item && await AppLauncher.OpenAsync(item) is { } problem)
        {
            ViewModel.ShowMessage(InfoBarSeverity.Warning, $"Couldn't open {item.Name}", problem);
        }
    }

    private void OnSearchMenuClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is AppItemViewModel item && ApplicationsViewModel.SearchOnline(item) is { } problem)
        {
            ViewModel.ShowMessage(InfoBarSeverity.Warning, $"Couldn't search for {item.Name}", problem);
        }
    }

    private void OnOpenLocationMenuClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is AppItemViewModel item)
        {
            ApplicationsViewModel.OpenLocation(item);
        }
    }

    private async void OnTechnicalDetailsMenuClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is not AppItemViewModel item) return;

        var details = new StackPanel { Spacing = 16 };
        details.Children.Add(new TextBlock { Text = item.Name, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, TextWrapping = TextWrapping.Wrap });
        AddDetail("Identifier", item.IdentifierText);
        AddDetail("Uninstall method", item.UninstallDetail);
        AddDetail("Registry key", item.Record.RegistryKey);
        AddDetail("Package", item.Record.PackageFullName);

        var dialog = new ContentDialog
        {
            Title = "Technical details",
            Content = new ScrollViewer
            {
                Content = details,
                MaxHeight = 360,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
            },
            CloseButtonText = "Close",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = XamlRoot,
            RequestedTheme = ActualTheme,
            Style = (Style)Application.Current.Resources["DefaultContentDialogStyle"],
        };
        await dialog.ShowAsync();

        void AddDetail(string label, string? value)
        {
            var field = new StackPanel { Spacing = 4 };
            field.Children.Add(new TextBlock
            {
                Text = label,
                Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"],
                Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
            });
            field.Children.Add(new TextBlock
            {
                Text = string.IsNullOrWhiteSpace(value) ? "Not reported" : value,
                TextWrapping = TextWrapping.Wrap,
                IsTextSelectionEnabled = true,
            });
            details.Children.Add(field);
        }
    }

    private async void OnListKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Delete && ViewModel.SelectedItems.Count > 0)
        {
            e.Handled = true;
            await ConfirmAndRemoveAsync(ViewModel.SelectedItems.ToArray());
        }
        if (e.Key == VirtualKey.Escape) AppList.SelectedItems.Clear();
    }

    private void OnFindInvoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        SearchBox.Focus(FocusState.Keyboard);
    }

    private void OnRefreshInvoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        _ = ViewModel.RefreshAsync();
    }

    private void OnSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!_restoringSelection) ViewModel.UpdateSelection(AppList.SelectedItems.Cast<AppItemViewModel>());
    }
    public void ClearSelection() => AppList.SelectedItems.Clear();
    public Task RemoveSelectionAsync() => ConfirmAndRemoveAsync(ViewModel.SelectedItems.ToArray());
    private void OnStopQueueClick(object sender, RoutedEventArgs e) => ViewModel.StopQueue();

    private async void OnReviewLeftoversClick(object sender, RoutedEventArgs e)
    {
        try { await LeftoverReview.ShowAsync(this, ViewModel); }
        catch (Exception error) { ViewModel.ShowMessage(InfoBarSeverity.Error, "Couldn't review leftovers", error.Message); }
    }

    private async Task ConfirmAndRemoveAsync(IReadOnlyList<AppItemViewModel> items)
    {
        var removable = items.Where(item => item.CanRemove).ToArray();
        if (removable.Length == 0 || ViewModel.IsRemoving)
        {
            return;
        }
        var dialog = new ContentDialog
        {
            Title = removable.Length == 1 ? $"Remove {removable[0].Name}?" : $"Remove {removable.Length} applications?",
            Content = new ScrollViewer { MaxHeight = 360, Content = new TextBlock {
                Text = "Each app's official uninstaller runs one at a time. Windows may request administrator approval. Leftovers require a separate review.\n\n"
                    + string.Join("\n", removable.Select(item => item.Name)), TextWrapping = TextWrapping.Wrap } },
            PrimaryButtonText = "Remove",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = XamlRoot,
            RequestedTheme = ActualTheme,
            Style = (Style)Application.Current.Resources["DefaultContentDialogStyle"],
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        nint window = App.MainWindow is { } main ? WindowNative.GetWindowHandle(main) : 0;
        await ViewModel.RemoveAsync(removable, window);
    }
}
