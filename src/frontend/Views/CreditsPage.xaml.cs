using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;

namespace Cleanup.Views;

public sealed partial class CreditsPage : Page
{
    public CreditsPage()
    {
        InitializeComponent();
        YearText.Text = DateTime.Now.Year.ToString(System.Globalization.CultureInfo.InvariantCulture);
    }

    private void OnBackInvoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        (App.MainWindow as MainWindow)?.OpenSettings();
    }
}
