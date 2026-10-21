using Cleanup.Services;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Foundation;

namespace Cleanup.Controls;

/// <summary>A round gauge. The accent arc covers <see cref="UsedFraction"/> of the circle, clockwise from the top.</summary>
public sealed partial class SpaceRing : UserControl
{
    public static readonly DependencyProperty UsedFractionProperty = DependencyProperty.Register(
        nameof(UsedFraction), typeof(double), typeof(SpaceRing), new PropertyMetadata(0d, OnGeometryChanged));

    public static readonly DependencyProperty RingThicknessProperty = DependencyProperty.Register(
        nameof(RingThickness), typeof(double), typeof(SpaceRing), new PropertyMetadata(16d, OnGeometryChanged));

    public SpaceRing()
    {
        InitializeComponent();
    }

    /// <summary>Sweeps the accent arc to <paramref name="fraction"/>, starting from wherever it is now.</summary>
    public void SweepTo(double fraction, TimeSpan duration)
    {
        if (!Motion.Enabled)
        {
            Snap(fraction);
            return;
        }
        double from = UsedFraction;
        Motion.Tween(this, DispatcherQueue, duration, TimeSpan.Zero, progress => UsedFraction = from + (fraction - from) * progress);
    }

    /// <summary>Stops any sweep in progress and sets the arc to <paramref name="fraction"/> at once.</summary>
    public void Snap(double fraction)
    {
        Motion.Stop(this);
        UsedFraction = fraction;
    }

    public double UsedFraction
    {
        get => (double)GetValue(UsedFractionProperty);
        set => SetValue(UsedFractionProperty, value);
    }

    public double RingThickness
    {
        get => (double)GetValue(RingThicknessProperty);
        set => SetValue(RingThicknessProperty, value);
    }

    private static void OnGeometryChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args) =>
        ((SpaceRing)sender).Redraw();

    private void OnSizeChanged(object sender, SizeChangedEventArgs e) => Redraw();

    private void Redraw()
    {
        double diameter = Math.Min(ActualWidth, ActualHeight);
        if (diameter <= RingThickness)
        {
            TrackPath.Data = null;
            UsedPath.Data = null;
            return;
        }

        var center = new Point(ActualWidth / 2, ActualHeight / 2);
        double radius = (diameter - RingThickness) / 2;
        TrackPath.StrokeThickness = RingThickness;
        UsedPath.StrokeThickness = RingThickness;
        TrackPath.Data = Circle(center, radius);

        double fraction = Math.Clamp(UsedFraction, 0, 1);
        UsedPath.Data = fraction switch
        {
            <= 0 => null,
            >= 0.9999 => Circle(center, radius),
            _ => Arc(center, radius, fraction),
        };
    }

    // An arc cannot start and end at the same point, so a full circle is drawn as an ellipse.
    private static EllipseGeometry Circle(Point center, double radius) =>
        new() { Center = center, RadiusX = radius, RadiusY = radius };

    private static PathGeometry Arc(Point center, double radius, double fraction)
    {
        double angle = fraction * 2 * Math.PI;
        var start = new Point(center.X, center.Y - radius);
        var end = new Point(center.X + radius * Math.Sin(angle), center.Y - radius * Math.Cos(angle));
        var figure = new PathFigure { StartPoint = start };
        figure.Segments.Add(new ArcSegment
        {
            Point = end,
            Size = new Size(radius, radius),
            IsLargeArc = fraction > 0.5,
            SweepDirection = SweepDirection.Clockwise,
        });
        return new PathGeometry { Figures = { figure } };
    }
}
