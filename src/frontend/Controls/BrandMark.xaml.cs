using Cleanup.Services;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Windows.UI;
using Windows.UI.ViewManagement;

namespace Cleanup.Controls;

/// <summary>The window and About logos share the chosen tint and follow theme and Windows color changes.</summary>
public sealed partial class BrandMark : UserControl
{
    private readonly UISettings _uiSettings = new();
    private readonly AccessibilitySettings _accessibility = new();

    public BrandMark()
    {
        InitializeComponent();
        Loaded += (_, _) =>
        {
            SettingsService.Changed += ApplyColors;
            _uiSettings.ColorValuesChanged += OnSystemColorsChanged;
            ApplyColors();
        };
        Unloaded += (_, _) =>
        {
            SettingsService.Changed -= ApplyColors;
            _uiSettings.ColorValuesChanged -= OnSystemColorsChanged;
        };
        ActualThemeChanged += (_, _) => ApplyColors();
    }

    private void OnSystemColorsChanged(UISettings sender, object args) => DispatcherQueue.TryEnqueue(ApplyColors);

    protected override AutomationPeer OnCreateAutomationPeer() => new BrandMarkAutomationPeer(this);

    private sealed class BrandMarkAutomationPeer(BrandMark owner) : FrameworkElementAutomationPeer(owner)
    {
        protected override AutomationControlType GetAutomationControlTypeCore() => AutomationControlType.Image;
        protected override string GetClassNameCore() => nameof(BrandMark);
    }

    private void ApplyColors()
    {
        AppSettings settings = SettingsService.Current;
        bool dark = ActualTheme == ElementTheme.Dark;
        Color surface = dark ? WindowMaterials.DarkSurface : WindowMaterials.LightSurface;
        Color tint = settings.AcrylicTintPreset is "system" or "accent"
            ? AccentService.WindowsAccent
            : AccentService.ParseHex(settings.AcrylicTintColor) ?? AccentService.WindowsAccent;
        Color toward = dark ? Colors.White : Colors.Black;
        Border[] tiles = [TopLeft, TopRight, BottomLeft, BottomRight];
        double[] shades = dark ? [0.75, 0.58, 0.48, 0.30] : [0.25, 0.15, 0.08, 0.0];
        for (int i = 0; i < tiles.Length; i++)
        {
            Color color = Mix(tint, toward, shades[i]);
            for (int step = 0; step < 20 && AccentService.ContrastRatio(color, surface) < 3; step++)
                color = Mix(color, toward, 0.12);
            if (_accessibility.HighContrast) color = _uiSettings.GetColorValue(UIColorType.Foreground);
            if (tiles[i].Background is Microsoft.UI.Xaml.Media.SolidColorBrush brush) brush.Color = color;
            else tiles[i].Background = AccentService.Brush(color);
        }
    }

    private static Color Mix(Color color, Color toward, double amount) => Color.FromArgb(255,
        (byte)Math.Round(color.R + (toward.R - color.R) * amount),
        (byte)Math.Round(color.G + (toward.G - color.G) * amount),
        (byte)Math.Round(color.B + (toward.B - color.B) * amount));
}
