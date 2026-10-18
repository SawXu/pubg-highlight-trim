using System.Windows;
using System.Windows.Controls;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class MainWindowTests
{
    [TestMethod]
    public void TargetControlsMaintainSelectionAndOnlyExposeOpenOutput()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                var app = new App();
                app.InitializeComponent();
                window = new MainWindow();
                var all = (CheckBox)window.FindName("TargetBoth");
                var selfDeath = (CheckBox)window.FindName("TargetSelfDeath");
                var ownKill = (CheckBox)window.FindName("TargetOwnKill");

                Assert.IsTrue(all.IsChecked == true);
                selfDeath.IsChecked = true;
                Assert.IsFalse(all.IsChecked == true);
                ownKill.IsChecked = true;
                Assert.IsTrue(selfDeath.IsChecked == true && ownKill.IsChecked == true);
                all.IsChecked = true;
                Assert.IsFalse(selfDeath.IsChecked == true || ownKill.IsChecked == true);
                all.IsChecked = false;
                Assert.IsTrue(all.IsChecked == true);
                selfDeath.IsChecked = true;
                ownKill.IsChecked = true;
                selfDeath.IsChecked = false;
                Assert.IsFalse(all.IsChecked == true);
                ownKill.IsChecked = false;
                Assert.IsTrue(all.IsChecked == true);

                Assert.IsNull(window.FindName("PlayMergedButton"));
                Assert.IsInstanceOfType<Button>(window.FindName("OpenOutputButton"));
            }
            catch (Exception ex)
            {
                failure = ex;
            }
            finally
            {
                window?.Close();
            }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
        {
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
        }
    }
}
