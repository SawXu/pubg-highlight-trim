using System.Globalization;

namespace PubgHighlightTrim.Ui.Core;

public static class CliCommandBuilder
{
    public static IReadOnlyList<string> Build(CliRunOptions options)
    {
        if (options.InputPaths.Count == 0 || options.InputPaths.Any(string.IsNullOrWhiteSpace))
        {
            throw new ArgumentException("At least one input path is required.", nameof(options));
        }

        var arguments = new List<string>();
        if (options.InputPaths.Count == 1)
        {
            arguments.Add(options.InputPaths[0]);
        }
        else
        {
            arguments.Add("--files");
            arguments.AddRange(options.InputPaths);
        }

        arguments.AddRange([
            "--target", options.Target,
            "--game-lang", options.GameLanguage,
            "--before", Format(options.SecondsBefore),
            "--after", Format(options.SecondsAfter),
            "--min-event-sec", Format(options.MinimumEventSeconds),
            "--molotov-elim-before", Format(options.MolotovSecondsBefore),
            "--scan-mode", options.ScanMode,
            "--jobs", options.Jobs,
        ]);

        if (!string.IsNullOrWhiteSpace(options.OutputDirectory))
        {
            arguments.Add("--output-dir");
            arguments.Add(options.OutputDirectory);
        }

        arguments.Add(options.Merge ? "--merge" : "--no-merge");
        AddFlag(arguments, options.Recursive, "--recursive");
        AddFlag(arguments, options.ScanOnly, "--scan-only");
        AddFlag(arguments, options.Overwrite, "--overwrite");
        AddFlag(arguments, !options.BrightnessGate, "--no-brightness-gate");
        return arguments;
    }

    public static string Preview(string executablePath, IReadOnlyList<string> arguments)
    {
        return string.Join(" ", new[] { Quote(executablePath) }.Concat(arguments.Select(Quote)));
    }

    private static void AddFlag(ICollection<string> arguments, bool enabled, string flag)
    {
        if (enabled)
        {
            arguments.Add(flag);
        }
    }

    private static string Format(double value) => value.ToString("0.###", CultureInfo.InvariantCulture);

    private static string Quote(string value)
    {
        if (value.Length > 0 && !value.Any(char.IsWhiteSpace) && !value.Contains('"'))
        {
            return value;
        }

        return $"\"{value.Replace("\"", "\\\"")}\"";
    }
}
