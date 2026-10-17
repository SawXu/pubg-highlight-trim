using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class CliRunnerTests
{
    [TestMethod]
    public async Task RunAsync_CapturesStdoutAndExitCode()
    {
        var lines = new List<string>();
        var runner = new CliRunner();

        var result = await runner.RunAsync(
            Path.Combine(Environment.SystemDirectory, "cmd.exe"),
            ["/d", "/c", "echo [01/01] INCLUDE own-kill"],
            (line, _) => lines.Add(line),
            CancellationToken.None);

        Assert.AreEqual(0, result.ExitCode);
        Assert.IsFalse(result.Cancelled);
        CollectionAssert.Contains(lines, "[01/01] INCLUDE own-kill");
    }

    [TestMethod]
    public async Task RunAsync_ForcesUtf8ForPythonCliOutput()
    {
        var lines = new List<string>();
        var runner = new CliRunner();
        var powershell = Path.Combine(Environment.SystemDirectory, "WindowsPowerShell", "v1.0", "powershell.exe");

        var result = await runner.RunAsync(
            powershell,
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); Write-Output ($env:PYTHONIOENCODING + '|' + $env:PYTHONUTF8 + '|' + $env:PUBG_HIGHLIGHT_TRIM_OUTPUT_ENCODING + '|中文日志')",
            ],
            (line, _) => lines.Add(line),
            CancellationToken.None);

        Assert.AreEqual(0, result.ExitCode);
        CollectionAssert.Contains(lines, "utf-8|1|utf-8|中文日志");
    }

    [TestMethod]
    public async Task RunAsync_UsesBundledRustBackendWhenCliBundleContainsRustEngine()
    {
        var root = Path.Combine(Path.GetTempPath(), "pubg-ui-rust-test-" + Guid.NewGuid().ToString("N"));
        var cliDirectory = Path.Combine(root, "cli");
        var modelDirectory = Path.Combine(cliDirectory, "ocr_models");
        Directory.CreateDirectory(modelDirectory);
        File.Copy(
            Path.Combine(Environment.SystemDirectory, "WindowsPowerShell", "v1.0", "powershell.exe"),
            Path.Combine(cliDirectory, "pubg-highlight-trim.exe"));
        File.WriteAllText(Path.Combine(cliDirectory, "ocr_engine.exe"), string.Empty);
        foreach (var file in new[] { "pp_ocrv6_medium.json", "det.onnx", "rec.onnx", "dict.txt" })
        {
            File.WriteAllText(Path.Combine(modelDirectory, file), string.Empty);
        }

        try
        {
            var lines = new List<string>();
            var runner = new CliRunner();
            var cli = Path.Combine(cliDirectory, "pubg-highlight-trim.exe");
            var result = await runner.RunAsync(
                cli,
                [
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "Write-Output ($env:PUBG_OCR_BACKEND + '|' + $env:PUBG_OCR_ENGINE + '|' + $env:PUBG_OCR_MODEL_DIR)",
                ],
                (line, _) => lines.Add(line),
                CancellationToken.None);

            Assert.AreEqual(0, result.ExitCode);
            CollectionAssert.Contains(
                lines,
                $"rust|{Path.Combine(cliDirectory, "ocr_engine.exe")}|{modelDirectory}");
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }
}
