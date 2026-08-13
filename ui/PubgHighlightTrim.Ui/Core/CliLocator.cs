using System.IO;

namespace PubgHighlightTrim.Ui.Core;

public static class CliLocator
{
    public static string? Find(string baseDirectory)
    {
        var configured = Environment.GetEnvironmentVariable("PUBG_HIGHLIGHT_TRIM_CLI");
        var candidates = new[]
        {
            configured,
            Path.Combine(baseDirectory, "cli", "pubg-highlight-trim.exe"),
            Path.Combine(baseDirectory, "pubg-highlight-trim.exe"),
        };

        return candidates.FirstOrDefault(path => !string.IsNullOrWhiteSpace(path) && File.Exists(path));
    }
}
