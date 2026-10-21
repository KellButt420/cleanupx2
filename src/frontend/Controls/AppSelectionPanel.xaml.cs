using System.ComponentModel;
using Cleanup.Services;
using Cleanup.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Cleanup.Controls;

/// <summary>Compact selection presentation in the shell, sharing the page's confirmed actions.</summary>
public sealed partial class AppSelectionPanel : UserControl
{
    // Icons already on screen, so only newly selected apps animate in.
    private readonly HashSet<string> _animatedIds = [];

    public AppSelectionPanel()
    {
        InitializeComponent();

        // The panel fades in when a selection starts, and each part
        // cross-fades between the one-app and several-apps layouts. (No layout
        // glide: the bottom-anchored panel would briefly overlap Settings.)
        Motion.ShowHide(PanelRoot, rise: 0);
        foreach (UIElement part in new UIElement[] { SingleHeader, MultipleHeader, SingleDetails, IconStrip, SingleActions, MultipleRemove })
        {
            Motion.ShowHide(part, rise: 0);
        }

        Loaded += (_, _) =>
        {
            ViewModel.PropertyChanged += OnViewModelChanged;
        };
        Unloaded += (_, _) =>
        {
            ViewModel.PropertyChanged -= OnViewModelChanged;
        };
    }

    public ApplicationsViewModel ViewModel => ApplicationsViewModel.Shared;
    public event EventHandler? RemoveRequested;
    public event EventHandler? ClearRequested;

    private void OnViewModelChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(ApplicationsViewModel.SelectionIcons):
                _animatedIds.IntersectWith(ViewModel.SelectionIcons.Select(i => i.Record.Id));
                break;
            case nameof(ApplicationsViewModel.SelectionCount) when ViewModel.ShowMultiple:
                Motion.Bump(MultipleTitleText);
                break;
        }
    }

    private int _staggered;

    private void OnIconPrepared(ItemsRepeater sender, ItemsRepeaterElementPreparedEventArgs args)
    {
        if (args.Index >= ViewModel.SelectionIcons.Count || args.Element is not FrameworkElement element) return;
        if (!_animatedIds.Add(ViewModel.SelectionIcons[args.Index].Record.Id)) return;
        // Stagger icons that appear together (for example, Select all).
        Motion.PopIn(element, TimeSpan.FromMilliseconds(35 * (_staggered++ % 6)));
        DispatcherQueue.TryEnqueue(() => _staggered = 0);
    }

    private void OnClearClick(object sender, RoutedEventArgs e) => ClearRequested?.Invoke(this, EventArgs.Empty);
    private void OnRemoveClick(object sender, RoutedEventArgs e) => RemoveRequested?.Invoke(this, EventArgs.Empty);
    private async void OnOpenClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.Selected is { } item && await AppLauncher.OpenAsync(item) is { } problem)
        {
            ViewModel.ShowMessage(InfoBarSeverity.Warning, $"Couldn't open {item.Name}", problem);
        }
    }

    private void OnLocationClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.Selected is { } item) ApplicationsViewModel.OpenLocation(item);
    }

    private void OnWindowsSettingsClick(object sender, RoutedEventArgs e) => ApplicationsViewModel.OpenWindowsSettings();
}
