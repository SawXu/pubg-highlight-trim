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
}
