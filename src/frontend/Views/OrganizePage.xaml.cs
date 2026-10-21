using System.Diagnostics;
using Cleanup.Interop;
using Cleanup.Services;
using Cleanup.ViewModels;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Cleanup.Views;

/// <summary>One destination folder in the preview, with how many files go there.</summary>
public sealed record GroupSummary(string Name, string Detail, string Glyph);

/// <summary>
/// Choose a folder → review the moves → done. Sorting choices live behind a
/// single Options button, and the page only asks a question when a decision is
/// actually needed (for example, when a file name is already taken).
/// </summary>
public sealed partial class OrganizePage : Page
{
    private static readonly string[] Modes = ["simple", "type", "extension", "date"];
    private static readonly string[] ModeNames = ["Doc, Media, Binary, Others", "Detailed file types", "File extension", "Date modified"];
    private static readonly string[] DateKinds = ["none", "year", "month", "yearMonth"];
    private static readonly string[] SimpleGroups = ["Doc", "Media", "Binary", "Others"];
    private static readonly string[] DetailedGroups = ["Images", "Videos", "Audio", "Documents", "Archives", "Installers", "Other"];

    private OrganizePlan? _plan;
    private KnownLocations? _locations;
    private List<OrganizationRule> _rules = [];
    private string? _source;
    private string? _lastBatchId;
    private string? _lastFolder;
    private bool _busy;
    private bool _choosingFolder;
    private CancellationTokenSource? _cancellation;

    // Options (mode, date folders, and name handling are remembered).
    private string _mode = Modes.Contains(SettingsService.Current.OrganizeMode) ? SettingsService.Current.OrganizeMode : "simple";
    private string _dateFolders = DateKinds.Contains(SettingsService.Current.OrganizeDateFolders) ? SettingsService.Current.OrganizeDateFolders : "none";
    private string _collisions = SettingsService.Current.OrganizeCollisions;
    private string _destination = "";
    private string? _onlyCategory;
    private string _onlyExtension = "";

    public OrganizePage()
    {
        InitializeComponent();
        foreach (UIElement step in new UIElement[] { StartPanel, PreviewPanel, DonePanel }) Motion.ShowHide(step, rise: 14);
        UpdateStartDescription();
        Loaded += OnLoaded;
        Unloaded += (_, _) => OperationUpdates.UndoCompleted -= OnBatchUndone;
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        // The event is static, so a page that stayed subscribed after it left would never be released.
        OperationUpdates.UndoCompleted -= OnBatchUndone;
        OperationUpdates.UndoCompleted += OnBatchUndone;
        try { _locations ??= await Features.GetLocationsAsync(); _rules = await RulesService.LoadAsync(); }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't load folders or rules", error.Message); }
    }

    private void UpdateStartDescription() => StartDescription.Text = _mode switch
    {
        "simple" => "Sort files into Doc, Media, Binary, and Others. Preview every change before moving anything.",
        "type" => "Files are sorted into Images, Videos, Audio, Documents, Archives, Installers, and Other. You'll see every move before anything happens.",
        "extension" => "Files are sorted into one folder per extension, such as PDF or ZIP. You'll see every move before anything happens.",
        _ => "Files are sorted into folders by when they were last changed. You'll see every move before anything happens.",
    };

    // ---- Steps ---------------------------------------------------------------

    private void ShowStep(UIElement step)
    {
        foreach (UIElement panel in new UIElement[] { StartPanel, PreviewPanel, DonePanel })
            panel.Visibility = panel == step ? Visibility.Visible : Visibility.Collapsed;
    }

    private void OnStartOverClick(object sender, RoutedEventArgs e)
    {
        if (_busy) return;
        _plan = null; _source = null; _lastBatchId = null; _lastFolder = null; Status.IsOpen = false;
        ShowStep(StartPanel);
    }

    private void OnChangeFolderClick(object sender, RoutedEventArgs e) => OnStartOverClick(sender, e);

    // ---- Choosing a folder ---------------------------------------------------

    private async void OnDownloadsClick(object sender, RoutedEventArgs e) { if (_locations?.Downloads is { } path) await OpenFolderAsync(path); }

    private async void OnDesktopClick(object sender, RoutedEventArgs e) { if (_locations?.Desktop is { } path) await OpenFolderAsync(path); }

    private async void OnDocumentsClick(object sender, RoutedEventArgs e) { if (_locations?.Documents is { } path) await OpenFolderAsync(path); }

    private async void OnChooseFolderClick(object sender, RoutedEventArgs e)
    {
        if (_busy || _choosingFolder) return;
        _choosingFolder = true;
        try
        {
            var picker = new FolderPicker(); picker.FileTypeFilter.Add("*");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(App.MainWindow!));
            if (await picker.PickSingleFolderAsync() is { } folder) await OpenFolderAsync(folder.Path);
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't open the folder picker", error.Message); }
        finally { _choosingFolder = false; }
    }

    private void OnFolderDragOver(object sender, DragEventArgs e)
    {
        if (_busy || !e.DataView.Contains(StandardDataFormats.StorageItems)) return;
        e.AcceptedOperation = DataPackageOperation.Link;
        e.DragUIOverride.Caption = "Organize this folder";
    }

    private async void OnFolderDrop(object sender, DragEventArgs e)
    {
        if (_busy || !e.DataView.Contains(StandardDataFormats.StorageItems)) return;
        try
        {
        IReadOnlyList<IStorageItem> items = await e.DataView.GetStorageItemsAsync();
        if (items.OfType<StorageFolder>().FirstOrDefault() is { } folder) await OpenFolderAsync(folder.Path);
        else Show(InfoBarSeverity.Informational, "Drop a folder, not a file", "cleanup organizes the files inside a folder.");
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't read the dropped folder", error.Message); }
    }

    private async Task OpenFolderAsync(string path)
    {
        if (_busy) return;
        _source = path;
        SettingsService.Remember(s => s.LastOrganizeFolder = path);
        await PreviewAsync();
    }

    // ---- Preview -------------------------------------------------------------

    private async Task PreviewAsync()
    {
        if (_busy || _source is null) return;
        SetBusy(true);
        Status.IsOpen = false;
        try
        {
            _plan = await Features.PlanAsync(_source, _mode, _dateFolders,
                string.IsNullOrWhiteSpace(_destination) ? null : _destination,
                string.IsNullOrWhiteSpace(_onlyExtension) ? null : _onlyExtension.Trim().TrimStart('.'),
                _onlyCategory, skipCollisions: _collisions == "skip", cancellation: _cancellation!.Token);
            FolderName.Text = Path.GetFileName(_plan.Source.TrimEnd('\\')) is { Length: > 0 } name ? name : _plan.Source;
            FolderPath.Text = _plan.Source;
            PreviewList.ItemsSource = _plan.Operations;
            PreviewList.SelectAll();
            GroupTiles.ItemsSource = _plan.Operations
                .GroupBy(o => o.DestinationFolder.Split('\\')[0], StringComparer.OrdinalIgnoreCase)
                .OrderByDescending(g => g.Count())
                .Select(g => new GroupSummary(g.Key, $"{g.Count()} {(g.Count() == 1 ? "file" : "files")} · {AppItemViewModel.FormatSize((ulong)g.Sum(o => (decimal)o.SizeBytes)) ?? "0 B"}", GlyphFor(g.Key)))
                .ToArray();
            bool tidy = _plan.Operations.Length == 0;
            TidyState.Visibility = tidy ? Visibility.Visible : Visibility.Collapsed;
            PreviewList.Visibility = tidy ? Visibility.Collapsed : Visibility.Visible;
            TidyDetail.Text = _plan.Skipped.Length > 0 ? "Some files are staying where they are; see why below." : "There are no loose files to sort. Subfolders are left as they are.";
            SkippedButton.Content = $"{_plan.Skipped.Length} staying in place";
            SkippedButton.Visibility = _plan.Skipped.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
            ShowStep(PreviewPanel);
            UpdateSummary();
        }
        catch (Exception error) when (error is OperationCanceledException || error is NativeCoreException { Code: "cancelled" })
        {
            _plan = null;
            ShowStep(StartPanel);
            Show(InfoBarSeverity.Informational, "Preview cancelled", "No files were moved.");
        }
        catch (Exception error)
        {
            Show(InfoBarSeverity.Error, "Couldn't open this folder", error.Message);
            ShowStep(StartPanel);
        }
        finally { SetBusy(false); }
    }

    private static string GlyphFor(string folder) => folder.ToLowerInvariant() switch
    {
        "doc" or "documents" => "",
        "media" or "images" => "",
        "videos" => "",
        "audio" => "",
        "binary" or "installers" => "",
        "archives" => "",
        _ => "",
    };

    private void OnPreviewSelectionChanged(object sender, SelectionChangedEventArgs e) => UpdateSummary();

    private void UpdateSummary()
    {
        PlannedOperation[] selected = PreviewList.SelectedItems.Cast<PlannedOperation>().ToArray();
        ApplyButton.IsEnabled = !_busy && selected.Length > 0;
        ApplyButton.Content = selected.Length > 0 ? $"Move {selected.Length} {(selected.Length == 1 ? "file" : "files")}" : "Move files";
        if (_plan is null || _plan.Operations.Length == 0) { SummaryText.Text = ""; return; }
        int folders = selected.Select(o => o.DestinationFolder).Distinct(StringComparer.OrdinalIgnoreCase).Count();
        SummaryText.Text = $"{selected.Length} of {_plan.Operations.Length} selected · {AppItemViewModel.FormatSize((ulong)selected.Sum(i => (decimal)i.SizeBytes)) ?? "0 B"} into {folders} {(folders == 1 ? "folder" : "folders")}";
    }

    private async void OnSkippedClick(object sender, RoutedEventArgs e)
    {
        if (_plan is { } plan)
            await Dialog("Staying in place", string.Join("\n\n", plan.Skipped.Select(i => $"{i.FileName}\n{i.Reason}")), null, "Close").ShowAsync();
    }

    // ---- Moving --------------------------------------------------------------

    private async void OnApplyClick(object sender, RoutedEventArgs e)
    {
        if (_plan is not { } plan || _busy) return;
        PlannedOperation[] selected = PreviewList.SelectedItems.Cast<PlannedOperation>().ToArray();

        // Only ask about name clashes when there are some.
        PlannedOperation[] clashes = selected.Where(o => o.Renamed).ToArray();
        string handling = _collisions;
        if (clashes.Length > 0 && handling == "ask")
        {
            var ask = Dialog(
                $"{clashes.Length} {(clashes.Length == 1 ? "name is" : "names are")} already taken",
                $"{string.Join(", ", clashes.Take(5).Select(c => c.FileName))}{(clashes.Length > 5 ? ", …" : "")} already exist{(clashes.Length == 1 ? "s" : "")} in the destination. Nothing is ever overwritten.",
                "Keep both", "Cancel", secondary: "Skip these files");
            ContentDialogResult answer = await ask.ShowAsync();
            if (answer == ContentDialogResult.None) return;
            handling = answer == ContentDialogResult.Primary ? "keepBoth" : "skip";
        }
        if (handling == "skip") selected = selected.Where(o => !o.Renamed).ToArray();
        if (selected.Length == 0) return;

        var confirm = Dialog($"Move {selected.Length} {(selected.Length == 1 ? "file" : "files")}?",
            "Only the selected files will move. Existing files will not be overwritten.\n\n" +
            string.Join("\n\n", selected.Select(o => $"{o.Source}\n→ {o.Destination}")), "Move files", "Cancel");
        confirm.DefaultButton = ContentDialogButton.Close;
        if (await confirm.ShowAsync() != ContentDialogResult.Primary) return;

        SetBusy(true);
        try
        {
            string folderName = Path.GetFileName(plan.Source.TrimEnd('\\'));
            OrganizeApplyResult result = await Features.ApplyAsync(plan, selected, $"Organized {folderName}", _cancellation!.Token);
            _lastBatchId = result.Moved > 0 ? result.BatchId : null;
            _lastFolder = plan.Destination;
            int folders = selected.Select(o => o.DestinationFolder.Split('\\')[0]).Distinct(StringComparer.OrdinalIgnoreCase).Count();
            DoneIcon.Glyph = result.Failed > 0 ? "" : "";
            bool stopped = _cancellation.IsCancellationRequested;
            DoneTitle.Text = stopped ? $"Stopped after moving {result.Moved} files" : result.Failed > 0
                ? $"Moved {result.Moved} of {selected.Length} files"
                : $"Moved {result.Moved} {(result.Moved == 1 ? "file" : "files")} into {folders} {(folders == 1 ? "folder" : "folders")}";
            DoneDetail.Text = result.Failed > 0
                ? string.Join("\n", result.Failures.Select(i => $"{i.FileName}: {i.Reason}"))
                : stopped ? "Completed moves are in History and can be undone where eligible. Remaining files were left in place." : "You can undo this now or later from History.";
            _plan = null;
            UndoButton.Visibility = _lastBatchId is null ? Visibility.Collapsed : Visibility.Visible;
            ShowStep(DonePanel);
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't finish moving files", error.Message + " Check History before retrying; some moves may have completed."); }
        finally { SetBusy(false); }
    }

    private async void OnUndoClick(object sender, RoutedEventArgs e)
    {
        if (_lastBatchId is not { } batch || _busy) return;
        var confirm = Dialog("Undo these file moves?", "Only unchanged files with available original names can be restored. Conflicts will stay in place.", "Undo moves", "Cancel");
        confirm.DefaultButton = ContentDialogButton.Close;
        if (await confirm.ShowAsync() != ContentDialogResult.Primary) return;
        SetBusy(true, canCancel: false);
        try
        {
            UndoResult result = await Features.UndoAsync(batch);
            OperationUpdates.PublishUndo(batch, result);
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't undo", error.Message); }
        finally { SetBusy(false); }
    }

    private void OnBatchUndone(string batch, UndoResult result)
    {
        if (_lastBatchId != batch) return;
        _lastBatchId = result.Conflicts.Length == 0 ? null : batch;
        _lastFolder = _source ?? _lastFolder;
        UndoButton.Visibility = _lastBatchId is null ? Visibility.Collapsed : Visibility.Visible;
        DoneIcon.Glyph = result.Conflicts.Length > 0 ? "" : "";
        DoneTitle.Text = $"Put back {result.Restored} {(result.Restored == 1 ? "file" : "files")}";
        DoneDetail.Text = result.Conflicts.Length > 0
            ? string.Join("\n", result.Conflicts.Select(i => $"{i.FileName}: {i.Reason}"))
            : "All recorded moves in this batch have been restored.";
    }

    private void OnOpenFolderClick(object sender, RoutedEventArgs e)
    {
        if (_busy) return;
        if (_lastFolder is { } folder && Directory.Exists(folder))
            Process.Start(new ProcessStartInfo("explorer.exe", $"\"{folder}\"") { UseShellExecute = true });
    }

    // ---- Options ---------------------------------------------------------------

    private async void OnOptionsClick(object sender, RoutedEventArgs e)
    {
        if (_busy) return;
        SetBusy(true, canCancel: false);
        try
        {
            string? ruleSource = _source;
            var feedback = new InfoBar { IsOpen = false, IsClosable = true };
            void Report(string message) { feedback.Message = message; feedback.Severity = InfoBarSeverity.Warning; feedback.IsOpen = true; }
            var mode = new ComboBox { Header = "Sort into", ItemsSource = ModeNames, SelectedIndex = Array.IndexOf(Modes, _mode), HorizontalAlignment = HorizontalAlignment.Stretch };
            var dates = new ComboBox { Header = "Date folders", ItemsSource = new[] { "None", "Year", "Month", "Year / Month" }, SelectedIndex = Array.IndexOf(DateKinds, _dateFolders), HorizontalAlignment = HorizontalAlignment.Stretch };
            var names = new ComboBox { Header = "When a name is already taken", ItemsSource = new[] { "Ask me", "Keep both (rename the moved file)", "Skip the file" }, SelectedIndex = _collisions switch { "keepBoth" => 1, "skip" => 2, _ => 0 }, HorizontalAlignment = HorizontalAlignment.Stretch };
            var destination = new TextBox { Header = "Put the folders in", PlaceholderText = "The same folder", Text = _destination };
            var category = new ComboBox { Header = "Only move", HorizontalAlignment = HorizontalAlignment.Stretch };
            var extension = new TextBox { Header = "Only this extension", PlaceholderText = ".pdf", Text = _onlyExtension };
            void FillCategories()
            {
                bool simple = Modes[Math.Max(0, mode.SelectedIndex)] == "simple";
                string[] groups = simple ? SimpleGroups : DetailedGroups;
                category.ItemsSource = new[] { "All files" }.Concat(groups).ToArray();
                category.IsEnabled = Modes[Math.Max(0, mode.SelectedIndex)] is "simple" or "type";
                category.SelectedIndex = _onlyCategory is not null && Array.IndexOf(groups, _onlyCategory) is int i and >= 0 ? i + 1 : 0;
            }
            FillCategories();
            mode.SelectionChanged += (_, _) => FillCategories();

            var ruleName = new TextBox { PlaceholderText = "Rule name" };
            var saveRule = new Button { Content = "Save as rule" };
            var savedRules = new ComboBox { PlaceholderText = "Saved rules", ItemsSource = _rules.ToArray(), HorizontalAlignment = HorizontalAlignment.Stretch };
            var deleteRule = new Button { Content = "Delete" };
            var rulesRow = new Grid { ColumnSpacing = 8 };
            rulesRow.ColumnDefinitions.Add(new ColumnDefinition()); rulesRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            rulesRow.Children.Add(ruleName); Grid.SetColumn(saveRule, 1); rulesRow.Children.Add(saveRule);
            var savedRow = new Grid { ColumnSpacing = 8 };
            savedRow.ColumnDefinitions.Add(new ColumnDefinition()); savedRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            savedRow.Children.Add(savedRules); Grid.SetColumn(deleteRule, 1); savedRow.Children.Add(deleteRule);

            savedRules.SelectionChanged += (_, _) =>
            {
                if (savedRules.SelectedItem is not OrganizationRule r) return;
                mode.SelectedIndex = Math.Max(0, Array.IndexOf(Modes, r.EffectiveMode));
                dates.SelectedIndex = Math.Clamp(r.DateFolders, 0, 3);
                // Rules saved before "Ask me" existed used 0 = rename, 1 = skip.
                names.SelectedIndex = r.SchemaVersion < 2 ? (r.Collision == 1 ? 2 : 1) : Math.Clamp(r.Collision, 0, 2);
                ruleSource = string.IsNullOrWhiteSpace(r.Source) ? _source : r.Source;
                destination.Text = r.Destination; extension.Text = r.Extension; ruleName.Text = r.Name;
                FillCategories();
                category.SelectedIndex = Math.Clamp(r.Category, 0, ((string[])category.ItemsSource).Length - 1);
            };
            saveRule.Click += async (_, _) =>
            {
                if (string.IsNullOrWhiteSpace(ruleName.Text)) { ruleName.Focus(FocusState.Programmatic); return; }
                if (ruleSource is null) { Report("Choose a source folder before saving a rule."); return; }
                string ruleExtension = extension.Text.Trim().TrimStart('.');
                if (!ValidExtension(ruleExtension) || ruleName.Text.Trim().Length > 100) { Report("Use a short rule name and a single extension such as .pdf."); return; }
                saveRule.IsEnabled = false;
                try
                {
                    string grouping = Modes[Math.Max(0, mode.SelectedIndex)];
                    string? match = category.IsEnabled && category.SelectedIndex > 0 ? (string)category.SelectedItem : null;
                    // A dry-run validates the source, destination and matching rules; nothing is moved.
                    await Features.PlanAsync(ruleSource, grouping, DateKinds[Math.Max(0, dates.SelectedIndex)],
                        string.IsNullOrWhiteSpace(destination.Text) ? null : destination.Text.Trim(),
                        ruleExtension.Length == 0 ? null : ruleExtension, match, names.SelectedIndex == 2);
                    var rule = new OrganizationRule(ruleName.Text.Trim(), ruleSource, destination.Text.Trim(), 0, dates.SelectedIndex, category.SelectedIndex, ruleExtension, names.SelectedIndex, grouping, SchemaVersion: 2);
                    var updated = _rules.Where(i => !i.Name.Equals(rule.Name, StringComparison.OrdinalIgnoreCase)).Append(rule).ToList();
                    await RulesService.SaveAsync(updated);
                    _rules = updated;
                    savedRules.ItemsSource = _rules.ToArray();
                    savedRules.SelectedItem = rule;
                    feedback.IsOpen = false;
                }
                catch (Exception error) { Report($"Couldn't save this rule: {error.Message}"); }
                finally { saveRule.IsEnabled = true; }
            };
            deleteRule.Click += async (_, _) =>
            {
                if (savedRules.SelectedItem is not OrganizationRule r) return;
                try
                {
                    var updated = _rules.Where(i => i != r).ToList();
                    await RulesService.SaveAsync(updated);
                    _rules = updated;
                    savedRules.ItemsSource = _rules.ToArray();
                }
                catch (Exception error) { Report($"Couldn't delete this rule: {error.Message}"); }
            };

            var more = new Expander { Header = "More options", HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Stretch,
                Content = new StackPanel { Spacing = 10, Children = { destination, category, extension, new TextBlock { Text = "Saved rules", Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], Margin = new Thickness(0, 6, 0, 0) }, savedRow, rulesRow } } };
            var content = new ScrollViewer { MaxHeight = 400, Content = new StackPanel { Spacing = 12, MinWidth = 340, Children = { feedback, mode, dates, names, more } } };
            ContentDialog dialog = Dialog("Options", null, "Done", "Cancel");
            dialog.Content = content;
            if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;

            string ext = extension.Text.Trim().TrimStart('.');
            if (!ValidExtension(ext))
            {
                Show(InfoBarSeverity.Warning, "Use one extension, such as .pdf", "Options were not changed.");
                return;
            }
            _mode = Modes[Math.Max(0, mode.SelectedIndex)];
            _dateFolders = DateKinds[Math.Max(0, dates.SelectedIndex)];
            _collisions = names.SelectedIndex switch { 1 => "keepBoth", 2 => "skip", _ => "ask" };
            _destination = destination.Text.Trim();
            _onlyCategory = category.IsEnabled && category.SelectedIndex > 0 ? (string)category.SelectedItem : null;
            _onlyExtension = ext;
            _source = ruleSource;
            SettingsService.Remember(s => { s.OrganizeMode = _mode; s.OrganizeDateFolders = _dateFolders; s.OrganizeCollisions = _collisions; });
            UpdateStartDescription();
            SetBusy(false);
            if (_source is not null) await PreviewAsync();
        }
        catch (Exception error) { Show(InfoBarSeverity.Error, "Couldn't update sorting options", error.Message); }
        finally { if (_busy) SetBusy(false); }
    }

    // ---- Helpers ---------------------------------------------------------------

    private ContentDialog Dialog(string title, string? message, string? primary, string close, string? secondary = null) => new()
    {
        Title = title,
        XamlRoot = XamlRoot,
        RequestedTheme = ActualTheme,
        PrimaryButtonText = primary ?? "",
        SecondaryButtonText = secondary ?? "",
        CloseButtonText = close,
        DefaultButton = primary is null ? ContentDialogButton.Close : ContentDialogButton.Primary,
        Content = message is null ? null : new ScrollViewer { MaxHeight = 360, Content = new TextBlock { TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true, Text = message } },
    };

    private void Show(InfoBarSeverity severity, string title, string message)
    {
        Status.Severity = severity; Status.Title = title; Status.Message = message; Status.IsOpen = true;
    }

    private void SetBusy(bool busy, bool canCancel = true)
    {
        _busy = busy;
        if (busy) { _cancellation = new CancellationTokenSource(); CancelButton.IsEnabled = true; }
        else { _cancellation?.Dispose(); _cancellation = null; }
        CancelButton.Visibility = busy && canCancel ? Visibility.Visible : Visibility.Collapsed;
        StartBusy.IsActive = busy && StartPanel.Visibility == Visibility.Visible;
        PreviewBusy.IsActive = busy && PreviewPanel.Visibility == Visibility.Visible;
        DropZone.Opacity = StartBusy.IsActive ? 0.4 : 1;
        DropZone.IsHitTestVisible = PreviewList.IsEnabled = UndoButton.IsEnabled = OpenFolderButton.IsEnabled = !busy;
        UpdateSummary();
    }

    private void OnCancelClick(object sender, RoutedEventArgs e)
    {
        _cancellation?.Cancel();
        CancelButton.IsEnabled = false;
    }

    private static bool ValidExtension(string value) => value.Length <= 30 && value.All(c => char.IsLetterOrDigit(c) || c is '-' or '_');
}
