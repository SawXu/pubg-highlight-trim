using System.Text.Json;
using System.Text.RegularExpressions;

namespace PubgHighlightTrim.Ui.Core;

public sealed partial class CliOutputParser
{
    public CliOutputEvent Parse(string line)
    {
        if (line.StartsWith("PROGRESS ", StringComparison.Ordinal))
        {
            try
            {
                using var document = JsonDocument.Parse(line[9..]);
                var root = document.RootElement;
                return new CliOutputEvent(
                    line,
                    root.GetProperty("current").GetInt32(),
                    root.GetProperty("total").GetInt32(),
                    false,
                    false,
                    null,
                    root.GetProperty("phase").GetString(),
                    root.TryGetProperty("workers", out var workers) ? workers.GetInt32() : null);
            }
            catch (Exception exception) when (exception is JsonException or KeyNotFoundException or InvalidOperationException)
            {
                return new CliOutputEvent(line, null, null, false, false, null, null, null);
            }
        }

        var progressMatch = ProgressLine().Match(line);
        if (progressMatch.Success)
        {
            var status = progressMatch.Groups["status"].Value;
            return new CliOutputEvent(
                line,
                int.Parse(progressMatch.Groups["current"].Value),
                int.Parse(progressMatch.Groups["total"].Value),
                status.Equals("INCLUDE", StringComparison.OrdinalIgnoreCase),
                status.Equals("SKIP", StringComparison.OrdinalIgnoreCase),
                null,
                "process",
                null);
        }

        if (line.StartsWith("SUMMARY ", StringComparison.Ordinal))
        {
            try
            {
                var summary = JsonSerializer.Deserialize<CliRunSummary>(line[8..]);
                return new CliOutputEvent(line, null, null, false, false, summary, null, null);
            }
            catch (JsonException)
            {
                return new CliOutputEvent(line, null, null, false, false, null, null, null);
            }
        }

        return new CliOutputEvent(line, null, null, false, false, null, null, null);
    }

    [GeneratedRegex(@"^\[(?<current>\d+)\/(?<total>\d+)\]\s+(?<status>INCLUDE|SKIP|full_scan=\S+)", RegexOptions.IgnoreCase)]
    private static partial Regex ProgressLine();
}

public sealed record CliOutputEvent(
    string RawLine,
    int? Current,
    int? Total,
    bool Included,
    bool Skipped,
    CliRunSummary? Summary,
    string? Phase,
    int? Workers);
