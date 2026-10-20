using System.Reflection;
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

    // Shared STA helper: creates App only if not already present (singleton per process).
    private static MainWindow CreateWindowOnCurrentStaThread()
    {
        if (Application.Current is null)
        {
            var app = new App();
            app.InitializeComponent();
        }
        return new MainWindow();
    }

    private static void Feed(MainWindow window, string line, bool isError = false)
    {
        var method = typeof(MainWindow).GetMethod("HandleCliLine", BindingFlags.NonPublic | BindingFlags.Instance)!;
        method.Invoke(window, [line, isError]);
    }

    [TestMethod]
    public void TrimPhaseActivityIsNotOverwrittenBySubsequentIncludeLine()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"event\":\"start\",\"total\":5}");
                var titleAfterTrimStart = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                Feed(window, "[01/05] INCLUDE own-kill 42.000s 37.000-43.000 ocr lang=en | sample.mp4");
                var titleAfterInclude = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                Assert.AreEqual(titleAfterTrimStart, titleAfterInclude, "INCLUDE must not overwrite trim phase activity");
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void TrimProgressUpdatesActivityWithClipCount()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"current\":3,\"total\":7}");
                var progress = ((TextBlock)window.FindName("ActivityProgressText")).Text;
                StringAssert.Contains(progress, "3");
                StringAssert.Contains(progress, "7");
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void UnknownPhaseProgressDoesNotOverwriteActivityAndDoesNotThrow()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"current\":2,\"total\":3}");
                var titleAfterTrim = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                // An unknown future phase (e.g. "archive") must not crash or corrupt activity.
                Feed(window, "PROGRESS {\"phase\":\"archive\",\"event\":\"start\",\"total\":1}");
                // INCLUDE after unknown phase must still be suppressed (structured-phase latch is held).
                Feed(window, "[01/03] INCLUDE own-kill 42.000s 37.000-43.000 ocr lang=en | sample.mp4");
                var titleAfterUnknown = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                Assert.AreEqual(titleAfterTrim, titleAfterUnknown, "unknown phase must not corrupt activity");
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void MergePhaseActivityIsDistinctFromTrimPhase()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"event\":\"start\",\"total\":2}");
                var titleAfterTrim = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                Feed(window, "PROGRESS {\"phase\":\"merge\",\"event\":\"start\",\"total\":1}");
                var titleAfterMerge = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                Assert.AreNotEqual(titleAfterTrim, titleAfterMerge, "merge phase must use distinct activity title");
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void StderrProgressAndSummaryLinesHaveNoStateImpact()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                // stderr is-error=true; should not update any state
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"current\":3,\"total\":7}", isError: true);
                Feed(window, "SUMMARY {\"source_count\":5,\"included_count\":5,\"skipped_count\":0}", isError: true);
                var progress = ((TextBlock)window.FindName("ActivityProgressText")).Text;
                var included = ((TextBlock)window.FindName("IncludedMetricText")).Text;
                // Progress text must NOT contain 3/7 from the stderr PROGRESS line
                Assert.IsFalse(progress.Contains("3") && progress.Contains("7"),
                    "stderr PROGRESS must not update activity progress");
                // Included metric must NOT be 5 from the stderr SUMMARY
                Assert.AreNotEqual("5", included, "stderr SUMMARY must not update included metric");
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void LegacyIncludeSkipUpdatesProgressBarWhenNoStructuredPhaseIsActive()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                // Before any structured phase: INCLUDE line must update progress bar
                Feed(window, "[03/12] INCLUDE own-kill 42.000s 37.000-43.000 ocr lang=en | sample.mp4");
                var bar = (System.Windows.Controls.ProgressBar)window.FindName("RunProgressBar");
                var metric = ((TextBlock)window.FindName("ProgressMetricText")).Text;
                Assert.AreEqual(12.0, bar.Maximum, "progress bar maximum must be total");
                Assert.AreEqual(3.0, bar.Value, "progress bar value must be current");
                StringAssert.Contains(metric, "3", "progress metric must contain current");
                StringAssert.Contains(metric, "12", "progress metric must contain total");
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void RerunResetsLatchAndMetrics()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                // First "run": enter trim phase (sets latch), see some included
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"event\":\"start\",\"total\":5}");
                Feed(window, "[01/05] INCLUDE own-kill 42.000s 37.000-43.000 ocr lang=en | a.mp4");
                Feed(window, "[02/05] SKIP self-death ocr lang=en | skipped.mp4");
                Feed(window, "SUMMARY {\"source_count\":5,\"included_count\":4,\"skipped_count\":1,\"total_seconds\":123}");
                var finish = typeof(MainWindow).GetMethod("FinishRun", BindingFlags.NonPublic | BindingFlags.Instance)!;
                finish.Invoke(window, [new Core.CliRunResult(1, true)]);
                var resetMethod = typeof(MainWindow).GetMethod(
                    "ResetRunState", BindingFlags.NonPublic | BindingFlags.Instance);
                Assert.IsNotNull(resetMethod, "production run reset must exist");
                resetMethod.Invoke(window, []);
                Assert.AreEqual("0", ((TextBlock)window.FindName("IncludedMetricText")).Text);
                Assert.AreEqual("0", ((TextBlock)window.FindName("SkippedMetricText")).Text);
                Assert.AreEqual("0 / 0", ((TextBlock)window.FindName("ProgressMetricText")).Text);
                Assert.IsNull(typeof(MainWindow).GetField("_summary", BindingFlags.NonPublic | BindingFlags.Instance)!.GetValue(window));
                Assert.IsFalse(((Button)window.FindName("OpenOutputButton")).IsEnabled);
                Assert.AreEqual(0.0, ((ProgressBar)window.FindName("RunProgressBar")).Value);
                // Second "run": INCLUDE line must now update activity (latch cleared)
                Feed(window, "[01/03] INCLUDE own-kill 42.000s 37.000-43.000 ocr lang=en | b.mp4");
                var activityTitle = ((TextBlock)window.FindName("ActivityTitleText")).Text;
                // Without latch, legacy INCLUDE updates activity to "正在整理输出"
                Assert.AreEqual("正在整理输出", activityTitle,
                    "after latch reset, INCLUDE must update activity");
                Assert.AreEqual("1", ((TextBlock)window.FindName("IncludedMetricText")).Text);
                finish.Invoke(window, [new Core.CliRunResult(-1, false)]);
                resetMethod.Invoke(window, []);
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"event\":\"start\",\"current\":0,\"total\":2,\"workers\":1}");
                StringAssert.Contains(((TextBlock)window.FindName("RunSubtitleText")).Text, "裁剪");
                StringAssert.Contains(((TextBlock)window.FindName("ActivityProgressText")).Text, "1 个");
                Feed(window, "PROGRESS {\"phase\":\"trim\",\"event\":\"done\",\"current\":2,\"total\":2,\"workers\":1}");
                Assert.AreEqual("✓", ((TextBlock)window.FindName("ActivityGlyphText")).Text);
                Feed(window, "PROGRESS {\"phase\":\"merge\",\"event\":\"skipped\",\"current\":0,\"total\":0,\"workers\":0,\"reason\":\"dry-run\"}");
                StringAssert.Contains(((TextBlock)window.FindName("ActivityTitleText")).Text, "跳过");
                StringAssert.Contains(((TextBlock)window.FindName("RunSubtitleText")).Text, "0 个");
                Assert.IsFalse(((ProgressBar)window.FindName("RunProgressBar")).IsIndeterminate);
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }

    [TestMethod]
    public void OldScanProgressWithoutEventFieldIsCompatible()
    {
        Exception? failure = null;
        var thread = new Thread(() =>
        {
            MainWindow? window = null;
            try
            {
                window = CreateWindowOnCurrentStaThread();
                // Old-style scan PROGRESS without "event" field
                Feed(window, "PROGRESS {\"phase\":\"scan\",\"current\":2,\"total\":5,\"workers\":1}");
                var bar = (System.Windows.Controls.ProgressBar)window.FindName("RunProgressBar");
                Assert.AreEqual(5.0, bar.Maximum);
                Assert.AreEqual(2.0, bar.Value);
            }
            catch (Exception ex) { failure = ex; }
            finally { window?.Close(); }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (failure is not null)
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }
}
