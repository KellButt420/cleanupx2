using Microsoft.UI.Composition;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace Cleanup.Services;

/// <summary>One window, one native controller. XAML owns its policy configuration;
/// this class owns and disconnects the controller, including during material switches.</summary>
internal sealed class TintedAcrylicBackdrop : SystemBackdrop
{
    private DesktopAcrylicController? _controller;
    private Color _tint;
    private Color _fallback;
    private float _strength;
    private float _luminosity;

    public void Update(AppSettings settings, bool dark)
    {
        Color surface = dark ? WindowMaterials.DarkSurface : WindowMaterials.LightSurface;
        Color chosen = settings.AcrylicTintPreset == "accent" ? AccentService.WindowsAccent : AccentService.ParseHex(settings.AcrylicTintColor) ?? surface;
        // Keep arbitrary custom colors readable in the selected light/dark theme.
        _tint = Blend(chosen, surface, dark ? 0.5 : 0.82);
        _fallback = Blend(_tint, surface, 0.5);
        _strength = (float)(0.4 + Clamp(settings.AcrylicTintStrength, 65) * 0.006);
        _luminosity = (float)(1 - Clamp(settings.AcrylicBackgroundVisibility, 40) * 0.0055);
        Apply();
    }

    private static double Clamp(double value, double fallback) => double.IsFinite(value) ? Math.Clamp(value, 0, 100) : fallback;
    private static Color Blend(Color color, Color surface, double amount) => Color.FromArgb(255,
        (byte)Math.Round(color.R + (surface.R - color.R) * amount),
        (byte)Math.Round(color.G + (surface.G - color.G) * amount),
        (byte)Math.Round(color.B + (surface.B - color.B) * amount));

    protected override void OnTargetConnected(ICompositionSupportsSystemBackdrop target, XamlRoot root)
    {
        base.OnTargetConnected(target, root);
        if (_controller is not null) throw new InvalidOperationException("A tinted backdrop belongs to one window.");
        _controller = new DesktopAcrylicController();
        _controller.SetSystemBackdropConfiguration(GetDefaultSystemBackdropConfiguration(target, root));
        Apply();
        _controller.AddSystemBackdropTarget(target);
    }

    protected override void OnTargetDisconnected(ICompositionSupportsSystemBackdrop target)
    {
        if (_controller is { } controller)
        {
            _controller = null;
            controller.RemoveSystemBackdropTarget(target);
            controller.Dispose();
        }
        base.OnTargetDisconnected(target);
    }

    protected override void OnDefaultSystemBackdropConfigurationChanged(ICompositionSupportsSystemBackdrop target, XamlRoot root)
    {
        // Theme changes can arrive without a target. The base implementation
        // looks that target up and throws. The controller already holds XAML's
        // live configuration; only our explicit colors need updating here.
        if (_controller is not null && root?.Content is FrameworkElement content)
            Update(SettingsService.Current, content.ActualTheme == ElementTheme.Dark);
    }

    private void Apply()
    {
        if (_controller is not { } controller) return;
        controller.TintColor = _tint;
        controller.FallbackColor = _fallback;
        controller.TintOpacity = _strength;
        controller.LuminosityOpacity = _luminosity;
    }
}
