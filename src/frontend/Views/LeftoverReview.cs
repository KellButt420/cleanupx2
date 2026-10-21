using Cleanup.Interop;
using Cleanup.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Markup;
using WinRT.Interop;

namespace Cleanup.Views;

internal static class LeftoverReview
{
    private sealed record Choice(LeftoverAppInfo App, LeftoverCandidate Candidate)
    {
        public override string ToString() => $"{App.DisplayName} · {Candidate.Kind} · {Candidate.Confidence} confidence\n{Candidate.Path}\n{string.Join("; ", Candidate.Reasons)}";
    }

    public static async Task ShowAsync(Page page, ApplicationsViewModel viewModel)
    {
        viewModel.CanReviewLeftovers = false;
        var choices = new List<Choice>();
        try
        {
            foreach (AppRecord app in viewModel.RemovedApps.Where(app => app.Technology != "msix"))
            {
                LeftoverAppInfo context = viewModel.LeftoverContext(app);
                LeftoverScanResult scan = await Features.ScanLeftoversAsync(context);
                choices.AddRange(scan.Candidates.Select(candidate => new Choice(context, candidate)));
            }
            var list = new ListView { ItemsSource = choices, SelectionMode = ListViewSelectionMode.Multiple,
                MaxHeight = 340, ItemTemplate = (DataTemplate)XamlReader.Load("<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><TextBlock Text='{Binding}' TextWrapping='Wrap' Margin='0,6' /></DataTemplate>") };
            var content = new StackPanel { Spacing = 12, MaxWidth = 600 };
            content.Children.Add(new TextBlock { Text = choices.Count == 0 ? "No clearly associated leftovers were found. Some data may remain."
                : "Select only items you recognize. Confidence is an association estimate, not a guarantee. Files go to the Recycle Bin where supported; registry removal is permanent after a backup is saved.", TextWrapping = TextWrapping.Wrap });
            content.Children.Add(list);
            var review = new ContentDialog { Title = "Review leftovers", Content = content, XamlRoot = page.XamlRoot,
                RequestedTheme = page.ActualTheme, PrimaryButtonText = "Review selected", CloseButtonText = "Keep everything",
                DefaultButton = ContentDialogButton.Close, IsPrimaryButtonEnabled = false };
            list.SelectionChanged += (_, _) => review.IsPrimaryButtonEnabled = list.SelectedItems.Count > 0;
            if (await review.ShowAsync() != ContentDialogResult.Primary) return;
            Choice[] selected = list.SelectedItems.Cast<Choice>().ToArray();
            var confirm = new ContentDialog { Title = $"Remove {selected.Length} selected leftovers?", XamlRoot = page.XamlRoot,
                RequestedTheme = page.ActualTheme, DefaultButton = ContentDialogButton.Close, PrimaryButtonText = "Remove selected", CloseButtonText = "Cancel",
                Content = new ScrollViewer { MaxHeight = 350, Content = new TextBlock { TextWrapping = TextWrapping.Wrap,
                    Text = "These exact targets will be rechecked before removal. Registry deletion cannot be undone in cleanup. Windows may ask separately if a file cannot be recycled.\n\n" + string.Join("\n\n", selected.Select(choice => choice.Candidate.Path)) } } };
            if (await confirm.ShowAsync() != ContentDialogResult.Primary) return;
            int removed = 0, failed = 0;
            var details = new List<string>();
            nint owner = App.MainWindow is { } window ? WindowNative.GetWindowHandle(window) : 0;
            foreach (var group in selected.GroupBy(choice => choice.App))
            {
                LeftoverRemoveResult result = await Features.RemoveLeftoversAsync(group.Key, group.Select(choice => choice.Candidate.Id).ToArray(), owner);
                removed += result.Removed; failed += result.Failed;
                details.AddRange(result.Results.Where(item => item.Detail is not null).Select(item => $"{item.Path}: {item.Detail}"));
            }
            viewModel.ShowMessage(failed > 0 ? InfoBarSeverity.Warning : InfoBarSeverity.Success,
                $"Removed {removed} leftover items" + (failed > 0 ? $"; {failed} left unchanged" : ""), string.Join("\n", details));
        }
        finally { viewModel.CanReviewLeftovers = viewModel.RemovedApps.Any(app => app.Technology != "msix"); }
    }
}
