using System.Diagnostics;
using System.IO;

namespace PubgHighlightTrim.Ui.Core;

public sealed class CliRunner
{
    private Process? _process;

    public async Task<CliRunResult> RunAsync(
        string executablePath,
        IReadOnlyList<string> arguments,
        Action<string, bool> onLine,
        CancellationToken cancellationToken)
    {
        var startInfo = new ProcessStartInfo(executablePath)
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = System.Text.Encoding.UTF8,
            StandardErrorEncoding = System.Text.Encoding.UTF8,
            WorkingDirectory = Path.GetDirectoryName(executablePath) ?? AppContext.BaseDirectory,
        };

        foreach (var argument in arguments)
        {
            startInfo.ArgumentList.Add(argument);
        }

        // Python otherwise uses the active Windows code page when stdout is redirected.
        startInfo.Environment["PYTHONIOENCODING"] = "utf-8";
        startInfo.Environment["PYTHONUTF8"] = "1";
        startInfo.Environment["PUBG_HIGHLIGHT_TRIM_OUTPUT_ENCODING"] = "utf-8";
        ConfigureBundledRustBackend(startInfo, executablePath);

        using var process = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
        _process = process;
        if (!process.Start())
        {
            throw new InvalidOperationException("The CLI process could not be started.");
        }

        using var registration = cancellationToken.Register(() => StopProcess(process));
        var stdout = ReadLinesAsync(process.StandardOutput, line => onLine(line, false));
        var stderr = ReadLinesAsync(process.StandardError, line => onLine(line, true));

        try
        {
            await process.WaitForExitAsync(cancellationToken);
            await Task.WhenAll(stdout, stderr);
            return new CliRunResult(process.ExitCode, cancellationToken.IsCancellationRequested);
        }
        catch (OperationCanceledException)
        {
            StopProcess(process);
            await Task.WhenAll(stdout, stderr);
            return new CliRunResult(-1, true);
        }
        finally
        {
            _process = null;
        }
    }

    public void Cancel() => StopProcess(_process);

    private static void ConfigureBundledRustBackend(ProcessStartInfo startInfo, string executablePath)
    {
        var cliDirectory = Path.GetDirectoryName(executablePath);
        if (string.IsNullOrWhiteSpace(cliDirectory))
        {
            return;
        }

        var modelDirectory = Path.Combine(cliDirectory, "ocr_models");
        var hasRustBundle =
            File.Exists(Path.Combine(cliDirectory, "ocr_engine.exe"))
            && File.Exists(Path.Combine(modelDirectory, "pp_ocrv6_medium.json"))
            && File.Exists(Path.Combine(modelDirectory, "det.onnx"))
            && File.Exists(Path.Combine(modelDirectory, "rec.onnx"))
            && File.Exists(Path.Combine(modelDirectory, "dict.txt"));
        if (!hasRustBundle)
        {
            return;
        }

        // A Rust-backed CLI bundle is self-contained and should not fall back to
        // PaddleOCR, which is intentionally absent from the Rust GUI package.
        startInfo.Environment["PUBG_OCR_BACKEND"] = "rust";
        startInfo.Environment["PUBG_OCR_ENGINE"] = Path.Combine(cliDirectory, "ocr_engine.exe");
        startInfo.Environment["PUBG_OCR_MODEL_DIR"] = modelDirectory;
    }

    private static async Task ReadLinesAsync(StreamReader reader, Action<string> onLine)
    {
        while (await reader.ReadLineAsync() is { } line)
        {
            onLine(line);
        }
    }

    private static void StopProcess(Process? process)
    {
        try
        {
            if (process is { HasExited: false })
            {
                process.Kill(entireProcessTree: true);
            }
        }
        catch (InvalidOperationException)
        {
        }
    }
}

public sealed record CliRunResult(int ExitCode, bool Cancelled);
