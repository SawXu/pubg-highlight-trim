namespace PubgHighlightTrim.Ui.Core;

public sealed record CliRunOptions(
    IReadOnlyList<string> InputPaths,
    string? OutputDirectory,
    string Target,
    string GameLanguage,
    double SecondsBefore,
    double SecondsAfter,
    double MinimumEventSeconds,
    double MolotovSecondsBefore,
    string ScanMode,
    string Jobs,
    bool Recursive,
    bool Merge,
    bool ScanOnly,
    bool Overwrite,
    bool BrightnessGate,
    bool MoveOriginals,
    bool SplitMergeByTarget = false,
    string TrimJobs = "auto")
{
    public string PrimaryInputPath => InputPaths.FirstOrDefault() ?? string.Empty;
}
