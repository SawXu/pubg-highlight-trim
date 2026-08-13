using System.Text.Json.Serialization;

namespace PubgHighlightTrim.Ui.Core;

public sealed class CliRunSummary
{
    [JsonPropertyName("source_count")]
    public int SourceCount { get; init; }

    [JsonPropertyName("included_count")]
    public int IncludedCount { get; init; }

    [JsonPropertyName("skipped_count")]
    public int SkippedCount { get; init; }

    [JsonPropertyName("dry_run")]
    public bool DryRun { get; init; }

    [JsonPropertyName("output_dir")]
    public string OutputDirectory { get; init; } = string.Empty;

    [JsonPropertyName("merge_output")]
    public string MergeOutput { get; init; } = string.Empty;

    [JsonPropertyName("merge_duration_sec")]
    public double MergeDurationSeconds { get; init; }

    [JsonPropertyName("merge_size_mb")]
    public double MergeSizeMegabytes { get; init; }

    [JsonPropertyName("profile_total_sec")]
    public double TotalSeconds { get; init; }
}
