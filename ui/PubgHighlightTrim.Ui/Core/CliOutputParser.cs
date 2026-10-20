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
                var phase = root.TryGetProperty("phase", out var phaseEl) ? phaseEl.GetString() : null;
                var phaseEvent = root.TryGetProperty("event", out var eventEl) ? eventEl.GetString() : null;
                var hasTotal = root.TryGetProperty("total", out var totalEl);
                var hasCurrent = root.TryGetProperty("current", out var currentEl);
                return new CliOutputEvent(
                    line,
                    hasCurrent ? currentEl.GetInt32() : null,
                    hasTotal ? totalEl.GetInt32() : null,
                    false,
                    false,
                    null,
                    phase,
                    root.TryGetProperty("workers", out var workers) ? workers.GetInt32() : null,
                    phaseEvent);
            }
            catch (Exception exception) when (exception is JsonException or KeyNotFoundException or InvalidOperationException)
            {
                return new CliOutputEvent(line, null, null, false, false, null, null, null, null);
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
                null,
                null);
        }

        if (line.StartsWith("SUMMARY ", StringComparison.Ordinal))
        {
            try
            {
                var summary = JsonSerializer.Deserialize<CliRunSummary>(line[8..]);
                return new CliOutputEvent(line, null, null, false, false, summary, null, null, null);
            }
            catch (JsonException)
            {
                return new CliOutputEvent(line, null, null, false, false, null, null, null, null);
            }
        }

        return new CliOutputEvent(line, null, null, false, false, null, null, null, null);
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
    int? Workers,
    string? PhaseEvent);
