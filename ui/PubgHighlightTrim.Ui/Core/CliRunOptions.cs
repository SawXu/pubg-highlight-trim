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
    bool BrightnessGate)
{
    public string PrimaryInputPath => InputPaths.FirstOrDefault() ?? string.Empty;
}
