using System.Globalization;
using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Windows.UI;
using Windows.UI.ViewManagement;

namespace Cleanup.Services;

/// <summary>
/// Native materials shared by the entire window, including its caption area.
/// WinUI owns activation, theme, transparency, and accessibility transitions.
/// </summary>
internal static class WindowMaterials
{
    public static readonly Color DarkSurface = Color.FromArgb(255, 0x1C, 0x1C, 0x20);
    public static readonly Color LightSurface = Color.FromArgb(255, 0xF3, 0xF3, 0xF5);

    public static SystemBackdrop? Create(BackdropKind kind) => kind switch {
        BackdropKind.Acrylic when DesktopAcrylicController.IsSupported() => new DesktopAcrylicBackdrop(),
        BackdropKind.Mica when MicaController.IsSupported() => new MicaBackdrop(),
        _ => null,
    };
}

/// <summary>
/// Applies the Windows accent color (followed live) or a custom accent, with
/// shades adjusted so accent-filled controls keep readable text in both themes.
/// </summary>
internal static class AccentService
{
    private static readonly UISettings Ui = new();
    private static string? _lastPalette;

    public static Color WindowsAccent => Ui.GetColorValue(UIColorType.Accent);

    /// <summary>Calls <paramref name="changed"/> on the UI thread when Windows colors change.</summary>
    public static void WatchSystemChanges(Microsoft.UI.Dispatching.DispatcherQueue dispatcher, Action changed) =>
        Ui.ColorValuesChanged += (_, _) => dispatcher.TryEnqueue(() => changed());

    public static Color? ParseHex(string? text)
    {
        if (text is { Length: 7 } && text[0] == '#' && uint.TryParse(text.AsSpan(1), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out uint rgb))
        {
            return Color.FromArgb(255, (byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb);
        }
        return null;
    }

    public static string ToHex(Color color) => $"#{color.R:X2}{color.G:X2}{color.B:X2}";

    /// <summary>Writes the accent palette into app resources and re-applies the theme so every brush updates.</summary>
    public static void Apply(FrameworkElement root)
    {
        if (new AccessibilitySettings().HighContrast) { _lastPalette = null; return; }
        AppSettings settings = SettingsService.Current;
        Color? custom = settings.UseCustomAccent ? ParseHex(settings.CustomAccent) : null;
        Color accent = custom ?? WindowsAccent;
        string palette = $"{custom is not null}:{ToHex(accent)}";
        if (_lastPalette == palette) return;
        _lastPalette = palette;

        Color light1, light2, light3, dark1, dark2, dark3;
        if (custom is null)
        {
            light1 = Ui.GetColorValue(UIColorType.AccentLight1);
            light2 = Ui.GetColorValue(UIColorType.AccentLight2);
            light3 = Ui.GetColorValue(UIColorType.AccentLight3);
            dark1 = Ui.GetColorValue(UIColorType.AccentDark1);
            dark2 = Ui.GetColorValue(UIColorType.AccentDark2);
            dark3 = Ui.GetColorValue(UIColorType.AccentDark3);
        }
        else
        {
            light1 = Mix(accent, Colors.White, 0.25);
            light2 = Mix(accent, Colors.White, 0.45);
            light3 = Mix(accent, Colors.White, 0.65);
            dark1 = Mix(accent, Colors.Black, 0.2);
            dark2 = Mix(accent, Colors.Black, 0.4);
            dark3 = Mix(accent, Colors.Black, 0.6);
        }
        // Dark theme draws accent fills with Light2 under black text; light theme
        // uses Dark1 under white text. Push the shades until both are readable.
        light2 = EnsureContrast(light2, Colors.Black, Colors.White);
        light1 = EnsureContrast(light1, Colors.Black, Colors.White, 3.0);
        dark1 = EnsureContrast(dark1, Colors.White, Colors.Black);
        dark2 = EnsureContrast(dark2, Colors.White, Colors.Black, 3.0);

        ResourceDictionary resources = Application.Current.Resources;
        resources["SystemAccentColor"] = accent;
        resources["SystemAccentColorLight1"] = light1;
        resources["SystemAccentColorLight2"] = light2;
        resources["SystemAccentColorLight3"] = light3;
        resources["SystemAccentColorDark1"] = dark1;
        resources["SystemAccentColorDark2"] = dark2;
        resources["SystemAccentColorDark3"] = dark3;
        // Native templates hold StaticResource aliases to these brush objects.
        // Mutate the existing brushes in each theme so switches, selection
        // indicators, progress, and buttons update along with color resources.
        ApplyBrushes(resources, root.RequestedTheme == ElementTheme.Dark);

        void ApplyBrushes(ResourceDictionary dictionary, bool dark)
        {
            Color fill = dark ? light2 : dark1;
            Color text = dark ? light3 : dark2;
            Set("AccentFillColorDefaultBrush", fill);
            Set("AccentFillColorSecondaryBrush", Color.FromArgb(230, fill.R, fill.G, fill.B));
            Set("AccentFillColorTertiaryBrush", Color.FromArgb(204, fill.R, fill.G, fill.B));
            Set("AccentTextFillColorPrimaryBrush", text);
            Set("AccentTextFillColorSecondaryBrush", dark ? light2 : dark3);
            Set("AccentTextFillColorTertiaryBrush", dark ? light1 : dark1);
            foreach (ResourceDictionary merged in dictionary.MergedDictionaries) ApplyBrushes(merged, dark);
            foreach (var pair in dictionary.ThemeDictionaries)
            {
                if (pair.Key is string theme && theme != "HighContrast" && pair.Value is ResourceDictionary themed)
                    ApplyBrushes(themed, theme != "Light");
            }
            void Set(string key, Color color)
            {
                if (dictionary.TryGetValue(key, out object brush) && brush is SolidColorBrush solid) solid.Color = color;
            }
        }

        // Theme resources resolve again only on a theme change.
        ElementTheme theme = root.RequestedTheme;
        root.RequestedTheme = root.ActualTheme == ElementTheme.Dark ? ElementTheme.Light : ElementTheme.Dark;
        root.RequestedTheme = theme;
    }

    private static Color Mix(Color color, Color with, double amount) => Color.FromArgb(
        255,
        (byte)Math.Round(color.R + (with.R - color.R) * amount),
        (byte)Math.Round(color.G + (with.G - color.G) * amount),
        (byte)Math.Round(color.B + (with.B - color.B) * amount));

    private static Color EnsureContrast(Color color, Color text, Color toward, double minimum = 4.5)
    {
        for (int step = 0; step < 20 && ContrastRatio(color, text) < minimum; step++)
        {
            color = Mix(color, toward, 0.12);
        }
        return color;
    }

    public static double ContrastRatio(Color a, Color b)
    {
        double la = Luminance(a), lb = Luminance(b);
        return (Math.Max(la, lb) + 0.05) / (Math.Min(la, lb) + 0.05);
    }

    private static double Luminance(Color c)
    {
        static double Channel(byte v)
        {
            double s = v / 255.0;
            return s <= 0.03928 ? s / 12.92 : Math.Pow((s + 0.055) / 1.055, 2.4);
        }
        return 0.2126 * Channel(c.R) + 0.7152 * Channel(c.G) + 0.0722 * Channel(c.B);
    }

    public static SolidColorBrush Brush(Color color) => new(color);
}
