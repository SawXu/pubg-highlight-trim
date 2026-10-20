using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
using Microsoft.Win32;
using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui;

public partial class MainWindow : Window
{
    private readonly ObservableCollection<string> _logLines = [];
    private readonly CliOutputParser _parser = new();
    private readonly TargetSelection _targetSelection = new();
    private bool _updatingTargetSelection;
    private readonly CliRunner _runner = new();
    private readonly Stopwatch _runStopwatch = new();
    private readonly DispatcherTimer _elapsedTimer = new() { Interval = TimeSpan.FromSeconds(1) };
    private CancellationTokenSource? _runCancellation;
    private string? _cliPath;
    private CliRunSummary? _summary;
    private IReadOnlyList<string> _selectedFiles = [];
    private bool _updatingInputDisplay;
    private int _included;
    private int _skipped;
    // Set when a structured phase (scan/dedupe/trim/merge/unknown) is active;
    // prevents legacy INCLUDE/SKIP lines from overwriting the activity panel.
    private string? _activeStructuredPhase;

    public MainWindow()
    {
        InitializeComponent();
        LogListBox.ItemsSource = _logLines;
        _elapsedTimer.Tick += (_, _) => DurationMetricText.Text = FormatDuration(_runStopwatch.Elapsed.TotalSeconds);
        WireCommandPreviewUpdates();
        Loaded += MainWindow_Loaded;
    }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        _cliPath = CliLocator.Find(AppContext.BaseDirectory);
        if (_cliPath is null)
        {
            SetCliStatus(false, "未找到 CLI");
            FooterStatusText.Text = "请将 CLI bundle 放入应用目录的 cli 文件夹";
            AppendLog("ERROR  未找到 cli\\pubg-highlight-trim.exe");
            UpdateRunAvailability();
            return;
        }

        try
        {
            var version = await ReadCliVersionAsync(_cliPath);
            SetCliStatus(true, version);
            FooterStatusText.Text = "CLI 已连接，后端就绪";
        }
        catch (Exception ex)
        {
            SetCliStatus(false, "CLI 不可用");
            AppendLog($"ERROR  {ex.Message}");
        }

        UpdateRunAvailability();
        UpdateCommandPreview();
    }

    private static async Task<string> ReadCliVersionAsync(string cliPath)
    {
        var startInfo = new ProcessStartInfo(cliPath, "--version")
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
        };
        using var process = Process.Start(startInfo) ?? throw new InvalidOperationException("CLI 启动失败");
        var output = await process.StandardOutput.ReadToEndAsync();
        await process.WaitForExitAsync();
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException($"CLI 版本检查失败，退出码 {process.ExitCode}");
        }

        return output.Trim();
    }

    private void SetCliStatus(bool ready, string text)
    {
        CliStatusDot.Fill = new SolidColorBrush((Color)ColorConverter.ConvertFromString(ready ? "#37C47A" : "#E16A5D"));
        CliStatusText.Text = text;
    }

    private void BrowseFolder_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFolderDialog { Title = "选择 PUBG Highlight 文件夹" };
        if (dialog.ShowDialog(this) == true)
        {
            SetInputDisplay(dialog.FolderName, []);
        }
    }

    private void BrowseFile_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFileDialog
        {
            Title = "选择 PUBG Highlight 视频",
            Filter = "MP4 视频 (*.mp4)|*.mp4",
            Multiselect = true,
        };
        if (dialog.ShowDialog(this) == true)
        {
            var files = dialog.FileNames;
            SetInputDisplay(files.Length == 1 ? files[0] : $"已选择 {files.Length} 个 MP4 文件", files);
        }
    }

    private void BrowseOutput_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFolderDialog { Title = "选择输出目录" };
        if (dialog.ShowDialog(this) == true)
        {
            OutputPathTextBox.Text = dialog.FolderName;
        }
    }

    private void InputPathTextBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (!_updatingInputDisplay)
        {
            _selectedFiles = [];
        }
        UpdateRunAvailability();
        UpdateCommandPreview();
    }

    private void AdvancedExpander_Expanded(object sender, RoutedEventArgs e) => UpdateCommandPreview();

    private async void RunButton_Click(object sender, RoutedEventArgs e)
    {
        if (_cliPath is null)
        {
            MessageBox.Show(this, "CLI 不可用", "无法开始", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }
        if (!TryBuildOptions(out var options, out var error))
        {
            MessageBox.Show(this, error, "无法开始", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        var arguments = CliCommandBuilder.Build(options!);
        CommandPreviewText.Text = CliCommandBuilder.Preview(_cliPath, arguments);
        _runCancellation = new CancellationTokenSource();
        ResetRunState();
        _runStopwatch.Restart();
        _elapsedTimer.Start();
        SetRunningState(true);
        RunTitleText.Text = "正在分析精彩时刻";
        RunSubtitleText.Text = options!.InputPaths.Count > 1
            ? $"已选择 {options.InputPaths.Count} 个 MP4 文件"
            : Path.GetFileName(options.PrimaryInputPath.TrimEnd(Path.DirectorySeparatorChar)) ?? options.PrimaryInputPath;
        SetActivity("OCR 正在工作", "正在加载模型并扫描源视频，完成一个视频后会更新进度。", $"0 / {options.InputPaths.Count}", "◌");
        AppendLog($"START  {DateTime.Now:HH:mm:ss}");

        CliRunResult result;
        try
        {
            result = await _runner.RunAsync(
                _cliPath,
                arguments,
                (line, isError) => Dispatcher.Invoke(() => HandleCliLine(line, isError)),
                _runCancellation.Token);
        }
        catch (Exception ex)
        {
            AppendLog($"ERROR  {ex.Message}");
            result = new CliRunResult(-1, false);
        }

        SetRunningState(false);
        FinishRun(result);
        _runCancellation.Dispose();
        _runCancellation = null;
    }

    private void ResetRunState()
    {
        _summary = null;
        _included = 0;
        _skipped = 0;
        _activeStructuredPhase = null;
        _logLines.Clear();
        IncludedMetricText.Text = "0";
        SkippedMetricText.Text = "0";
        DurationMetricText.Text = FormatDuration(0);
        ProgressMetricText.Text = "0 / 0";
        RunProgressBar.IsIndeterminate = false;
        RunProgressBar.Maximum = 1;
        RunProgressBar.Value = 0;
        OpenOutputButton.IsEnabled = false;
        RunTitleText.Text = "准备开始";
        RunSubtitleText.Text = string.Empty;
        FooterStatusText.Text = "准备开始";
        SetActivity("准备开始", "等待阶段进度。", "0 / 0", "◌");
    }

    private void HandleCliLine(string line, bool isError)
    {
        if (!IsNoisyDiagnostic(line))
        {
            AppendLog(isError ? $"ERR    {line}" : line);
        }
        // Stderr lines are not machine-readable PROGRESS/SUMMARY; do not parse.
        if (isError)
        {
            return;
        }
        var output = _parser.Parse(line);
        HandleStructuredPhase(output);

        if (output.Included)
        {
            IncludedMetricText.Text = (++_included).ToString(CultureInfo.InvariantCulture);
        }
        if (output.Skipped)
        {
            SkippedMetricText.Text = (++_skipped).ToString(CultureInfo.InvariantCulture);
        }
        if (output.Summary is not null)
        {
            _summary = output.Summary;
            IncludedMetricText.Text = _summary.IncludedCount.ToString(CultureInfo.InvariantCulture);
            SkippedMetricText.Text = _summary.SkippedCount.ToString(CultureInfo.InvariantCulture);
            DurationMetricText.Text = FormatDuration(_summary.TotalSeconds);
        }
    }

    private void HandleStructuredPhase(CliOutputEvent output)
    {
        var phase = output.Phase;

        // Legacy INCLUDE/SKIP lines (phase="process") must not overwrite activity
        // once a structured phase has latched. Counters still update above.
        if (phase == "process")
        {
            if (_activeStructuredPhase is null && output.Current is { } cur && output.Total is { } tot)
            {
                // No structured phase yet: restore full legacy progress update.
                RunProgressBar.IsIndeterminate = false;
                RunProgressBar.Maximum = Math.Max(1, tot);
                RunProgressBar.Value = cur;
                ProgressMetricText.Text = $"{cur} / {tot}";
                FooterStatusText.Text = $"正在整理第 {cur} 个，共 {tot} 个";
                SetActivity("正在整理输出", "正在生成裁剪片段和合并视频。", $"{cur} / {tot}", "◌");
            }
            return;
        }

        if (phase == "scan")
        {
            _activeStructuredPhase = "scan";
            if (output.Current is { } current && output.Total is { } total)
            {
                RunProgressBar.IsIndeterminate = current == 0;
                RunProgressBar.Maximum = Math.Max(1, total);
                RunProgressBar.Value = current;
                ProgressMetricText.Text = $"{current} / {total}";
                var workerText = output.Workers is { } workers ? $"，{workers} 个 worker" : string.Empty;
                RunSubtitleText.Text = $"OCR 扫描完成 {current} / {total}{workerText}";
                FooterStatusText.Text = current == 0 ? "正在初始化 OCR 并开始扫描" : $"已完成 {current} / {total} 个视频的 OCR 扫描";
                SetActivity(
                    current == total ? "OCR 扫描完成" : "OCR 正在扫描",
                    current == 0 ? "正在初始化 OCR 模型；首个视频可能需要更长时间。" : $"已完成 {current} / {total} 个视频的扫描。",
                    $"{current} / {total}{workerText}",
                    current == total ? "✓" : "◌");
            }
            return;
        }

        if (phase is "dedupe" or "trim" or "merge")
        {
            _activeStructuredPhase = phase;
            var name = phase switch { "dedupe" => "去重", "trim" => "裁剪", _ => "合并" };
            var current = output.Current ?? 0;
            var total = output.Total ?? 0;
            var skipped = output.PhaseEvent == "skipped";
            var done = output.PhaseEvent == "done";
            var workerText = output.Workers is { } workers ? $"，{workers} 个 worker" : string.Empty;
            var status = skipped ? $"已跳过{name}" : done ? $"{name}完成" : $"正在{name}";
            RunProgressBar.IsIndeterminate = false;
            RunProgressBar.Maximum = Math.Max(1, total);
            RunProgressBar.Value = current;
            ProgressMetricText.Text = $"{current} / {total}";
            RunSubtitleText.Text = $"{status} {current} / {total}{workerText}";
            FooterStatusText.Text = status;
            SetActivity(status,
                skipped ? "此阶段没有执行工作。" : $"已完成 {current} / {total}，单位：{(phase == "dedupe" ? "候选" : phase == "trim" ? "片段" : "合并目标")}。",
                $"{current} / {total}{workerText}", skipped ? "–" : done ? "✓" : "◌");
            return;
        }

        // Unknown future phase: latch to prevent INCLUDE/SKIP overwrite, but do not
        // update activity (future phases are safely ignored per protocol contract).
        if (phase is not null)
        {
            _activeStructuredPhase = phase;
        }
    }

    private void FinishRun(CliRunResult result)
    {
        _elapsedTimer.Stop();
        _runStopwatch.Stop();
        RunProgressBar.IsIndeterminate = false;
        var outcome = CliRunOutcome.From(result, _summary);
        RunTitleText.Text = outcome.Title;
        RunSubtitleText.Text = outcome.Description;
        FooterStatusText.Text = outcome.Status;
        SetActivity(outcome.Title, outcome.Description, outcome.Status, outcome.Glyph);
        OpenOutputButton.IsEnabled = _summary is not null && Directory.Exists(_summary.OutputDirectory);
        if (outcome.Completed)
        {
            RunProgressBar.Value = RunProgressBar.Maximum;
        }

        AppendLog($"EXIT   code={result.ExitCode}");
    }

    private void CancelButton_Click(object sender, RoutedEventArgs e)
    {
        CancelButton.IsEnabled = false;
        FooterStatusText.Text = "正在停止任务";
        _runCancellation?.Cancel();
        _runner.Cancel();
    }

    private void OpenOutputButton_Click(object sender, RoutedEventArgs e)
    {
        var summary = _summary;
        if (summary is not null && Directory.Exists(summary.OutputDirectory))
        {
            Process.Start(new ProcessStartInfo("explorer.exe", summary.OutputDirectory) { UseShellExecute = true });
        }
    }

    private void ClearLog_Click(object sender, RoutedEventArgs e) => _logLines.Clear();

    private void AppendLog(string line)
    {
        _logLines.Add(line);
        if (_logLines.Count > 2000)
        {
            _logLines.RemoveAt(0);
        }
        LogListBox.ScrollIntoView(_logLines.LastOrDefault());
    }

    private void SetActivity(string title, string description, string progress, string glyph)
    {
        ActivityTitleText.Text = title;
        ActivityDescriptionText.Text = description;
        ActivityProgressText.Text = progress;
        ActivityGlyphText.Text = glyph;
    }

    private static bool IsNoisyDiagnostic(string line) =>
        line.Equals("ReduceMeanCheckIfOneDNNSupport", StringComparison.Ordinal);

    private bool TryBuildOptions(out CliRunOptions? options, out string? error)
    {
        options = null;
        error = null;
        var inputPaths = CurrentInputPaths();
        if (inputPaths.Count == 0 || inputPaths.Any(path => !File.Exists(path) && !Directory.Exists(path)))
        {
            error = "请选择存在的 MP4 文件或文件夹。";
            return false;
        }
        if (inputPaths.Count > 1 && inputPaths.Any(path => !File.Exists(path) || !path.EndsWith(".mp4", StringComparison.OrdinalIgnoreCase)))
        {
            error = "多选输入只能包含 MP4 文件。";
            return false;
        }

        if (!TryNonNegative(BeforeTextBox.Text, "事件前秒数", out var before, out error)
            || !TryNonNegative(AfterTextBox.Text, "事件后秒数", out var after, out error)
            || !TryNonNegative(MinEventTextBox.Text, "忽略开场事件秒数", out var minimum, out error)
            || !TryNonNegative(MolotovTextBox.Text, "燃烧瓶前置秒数", out var molotov, out error))
        {
            return false;
        }

        var jobs = JobsTextBox.Text.Trim();
        if (!jobs.Equals("auto", StringComparison.OrdinalIgnoreCase)
            && (!int.TryParse(jobs, out var jobCount) || jobCount < 1))
        {
            error = "并行任务必须是 auto 或正整数。";
            return false;
        }

        options = new CliRunOptions(
            inputPaths,
            NullIfEmpty(OutputPathTextBox.Text),
            _targetSelection.Target,
            SelectedTag(LanguageComboBox),
            before,
            after,
            minimum,
            molotov,
            SelectedTag(ScanModeComboBox),
            jobs.ToLowerInvariant(),
            RecursiveCheckBox.IsChecked == true,
            MergeCheckBox.IsChecked == true,
            ScanOnlyCheckBox.IsChecked == true,
            OverwriteCheckBox.IsChecked == true,
            BrightnessGateCheckBox.IsChecked == true,
            MoveOriginalsCheckBox.IsChecked == true,
            _targetSelection.SplitMergeByTarget);
        return true;
    }

    private static bool TryNonNegative(string text, string label, out double value, out string? error)
    {
        if ((double.TryParse(text, NumberStyles.Float, CultureInfo.CurrentCulture, out value)
             || double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out value))
            && value >= 0)
        {
            error = null;
            return true;
        }

        error = $"{label}必须是大于或等于 0 的数字。";
        return false;
    }

    private static string SelectedTag(ComboBox comboBox) =>
        (comboBox.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? string.Empty;

    private static string? NullIfEmpty(string value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static string FormatDuration(double seconds)
    {
        var duration = TimeSpan.FromSeconds(seconds);
        return duration.TotalHours >= 1 ? duration.ToString(@"h\:mm\:ss") : duration.ToString(@"m\:ss");
    }

    private void SetRunningState(bool running)
    {
        if (running)
        {
            RunProgressBar.Value = 0;
            RunProgressBar.IsIndeterminate = true;
        }
        RunButton.IsEnabled = !running && _cliPath is not null && HasInputSelection();
        CancelButton.IsEnabled = running;
        OpenOutputButton.IsEnabled = !running && _summary is not null && Directory.Exists(_summary.OutputDirectory);
        InputPathTextBox.IsEnabled = !running;
        OutputPathTextBox.IsEnabled = !running;
        BrowseFolderButton.IsEnabled = !running;
        BrowseFileButton.IsEnabled = !running;
        BrowseOutputButton.IsEnabled = !running;
        TargetBoth.IsEnabled = !running;
        TargetSelfDeath.IsEnabled = !running;
        TargetOwnKill.IsEnabled = !running;
        LanguageComboBox.IsEnabled = !running;
        ScanModeComboBox.IsEnabled = !running;
        BeforeTextBox.IsEnabled = !running;
        AfterTextBox.IsEnabled = !running;
        MinEventTextBox.IsEnabled = !running;
        MolotovTextBox.IsEnabled = !running;
        JobsTextBox.IsEnabled = !running;
        MergeCheckBox.IsEnabled = !running;
        RecursiveCheckBox.IsEnabled = !running;
        ScanOnlyCheckBox.IsEnabled = !running;
        OverwriteCheckBox.IsEnabled = !running;
        MoveOriginalsCheckBox.IsEnabled = !running;
        BrightnessGateCheckBox.IsEnabled = !running;
    }

    private void UpdateRunAvailability()
    {
        if (RunButton is not null)
        {
            RunButton.IsEnabled = _runCancellation is null && _cliPath is not null && HasInputSelection();
        }
    }

    private void UpdateCommandPreview()
    {
        if (CommandPreviewText is null || _cliPath is null || !HasInputSelection())
        {
            return;
        }

        if (TryBuildOptions(out var options, out _))
        {
            CommandPreviewText.Text = CliCommandBuilder.Preview(_cliPath, CliCommandBuilder.Build(options!));
        }
    }

    private IReadOnlyList<string> CurrentInputPaths() =>
        _selectedFiles.Count > 0 ? _selectedFiles : NullIfEmpty(InputPathTextBox.Text) is { } path ? [path] : [];

    private bool HasInputSelection() => CurrentInputPaths().Count > 0;

    private void SetInputDisplay(string display, IReadOnlyList<string> selectedFiles)
    {
        _selectedFiles = selectedFiles;
        _updatingInputDisplay = true;
        InputPathTextBox.Text = display;
        _updatingInputDisplay = false;
        UpdateRunAvailability();
        UpdateCommandPreview();
    }

    private void WireCommandPreviewUpdates()
    {
        foreach (var textBox in new[] { OutputPathTextBox, BeforeTextBox, AfterTextBox, MinEventTextBox, MolotovTextBox, JobsTextBox })
        {
            textBox.TextChanged += (_, _) => UpdateCommandPreview();
        }
        foreach (var comboBox in new[] { LanguageComboBox, ScanModeComboBox })
        {
            comboBox.SelectionChanged += (_, _) => UpdateCommandPreview();
        }
        foreach (var checkBox in new[] { MergeCheckBox, RecursiveCheckBox, ScanOnlyCheckBox, OverwriteCheckBox, MoveOriginalsCheckBox, BrightnessGateCheckBox })
        {
            checkBox.Checked += (_, _) => UpdateCommandPreview();
            checkBox.Unchecked += (_, _) => UpdateCommandPreview();
        }
        foreach (var checkBox in new[] { TargetBoth, TargetSelfDeath, TargetOwnKill })
        {
            checkBox.Checked += TargetSelection_Changed;
            checkBox.Unchecked += TargetSelection_Changed;
        }
    }

    private void TargetSelection_Changed(object sender, RoutedEventArgs e)
    {
        if (_updatingTargetSelection)
        {
            return;
        }

        var checkBox = (CheckBox)sender;
        var target = checkBox == TargetBoth ? DetectionTarget.All
            : checkBox == TargetSelfDeath ? DetectionTarget.SelfDeath : DetectionTarget.OwnKill;
        _targetSelection.SetSelected(target, checkBox.IsChecked == true);
        _updatingTargetSelection = true;
        try
        {
            TargetBoth.IsChecked = _targetSelection.All;
            TargetSelfDeath.IsChecked = _targetSelection.SelfDeath;
            TargetOwnKill.IsChecked = _targetSelection.OwnKill;
        }
        finally
        {
            _updatingTargetSelection = false;
        }
        UpdateCommandPreview();
    }

    private void Window_Closing(object sender, System.ComponentModel.CancelEventArgs e)
    {
        if (_runCancellation is null)
        {
            return;
        }

        var result = MessageBox.Show(this, "任务仍在运行。停止任务并退出？", "确认退出", MessageBoxButton.YesNo, MessageBoxImage.Warning);
        if (result != MessageBoxResult.Yes)
        {
            e.Cancel = true;
            return;
        }

        _runCancellation.Cancel();
        _runner.Cancel();
    }
}
