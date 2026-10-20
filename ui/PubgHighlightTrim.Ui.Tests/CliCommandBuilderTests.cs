using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class CliCommandBuilderTests
{
    [TestMethod]
    public void Build_MapsUiOptionsToCliArguments()
    {
        var options = new CliRunOptions(
            [@"C:\Videos\PUBG Highlights"],
            @"D:\Output Clips",
            "own-kill",
            "zh-Hant",
            7.5,
            1.25,
            0,
            12,
            "full",
            "2",
            true,
            false,
            true,
            true,
            false,
            true);

        var arguments = CliCommandBuilder.Build(options);

        CollectionAssert.Contains(arguments.ToList(), @"C:\Videos\PUBG Highlights");
        CollectionAssert.Contains(arguments.ToList(), "own-kill");
        CollectionAssert.Contains(arguments.ToList(), "zh-Hant");
        CollectionAssert.Contains(arguments.ToList(), "7.5");
        CollectionAssert.Contains(arguments.ToList(), @"D:\Output Clips");
        CollectionAssert.Contains(arguments.ToList(), "--recursive");
        CollectionAssert.Contains(arguments.ToList(), "--no-merge");
        CollectionAssert.Contains(arguments.ToList(), "--scan-only");
        CollectionAssert.Contains(arguments.ToList(), "--overwrite");
        CollectionAssert.Contains(arguments.ToList(), "--no-brightness-gate");
        CollectionAssert.Contains(arguments.ToList(), "--move-originals");
    }

    [TestMethod]
    public void Build_MultipleFilesUsesFilesOptionInSelectionOrder()
    {
        var options = new CliRunOptions(
            [@"C:\Videos\second.mp4", @"C:\Videos\first.mp4"],
            null,
            "both",
            "auto",
            5,
            1,
            2,
            10,
            "auto",
            "auto",
            false,
            true,
            false,
            false,
            true,
            false);

        var arguments = CliCommandBuilder.Build(options);

        CollectionAssert.AreEqual(
            new[] { "--files", @"C:\Videos\second.mp4", @"C:\Videos\first.mp4" },
            arguments.Take(3).ToArray());
    }

    [TestMethod]
    public void Build_FastScanEnablesFastPath()
    {
        var options = new CliRunOptions(
            [@"C:\Videos\highlights"],
            null,
            "both",
            "auto",
            5,
            1,
            2,
            10,
            "fast",
            "auto",
            false,
            true,
            false,
            false,
            true,
            false);

        var arguments = CliCommandBuilder.Build(options);

        CollectionAssert.Contains(arguments.ToList(), "--fast-path");
    }

    [TestMethod]
    [DataRow("both", true, true, true)]
    [DataRow("both", true, false, false)]
    [DataRow("both", false, true, false)]
    [DataRow("both", false, false, false)]
    [DataRow("self-death", true, false, false)]
    [DataRow("self-death", false, false, false)]
    [DataRow("own-kill", true, false, false)]
    [DataRow("own-kill", false, false, false)]
    [DataRow("self-death", true, true, false)]
    [DataRow("own-kill", true, true, false)]
    public void Build_SplitsOnlyWhenMergingBothSpecificTargets(string target, bool merge, bool split, bool expected)
    {
        var options = new CliRunOptions(
            [@"C:\Videos\highlights"], null, target, "auto", 5, 1, 2, 10, "auto", "auto",
            false, merge, false, false, true, false, split);

        var arguments = CliCommandBuilder.Build(options).ToList();

        Assert.AreEqual(expected, arguments.Contains("--split-merge-by-target"));
        Assert.AreEqual(target, arguments[arguments.IndexOf("--target") + 1]);
        CollectionAssert.Contains(arguments, merge ? "--merge" : "--no-merge");
    }

    [TestMethod]
    public void Build_DefaultTrimJobsIsAutoIndependentOfOcrJobs()
    {
        var options = new CliRunOptions(
            [@"C:\Videos\highlights"], null, "both", "auto", 5, 1, 2, 10, "auto", "4",
            false, true, false, false, true, false);

        var arguments = CliCommandBuilder.Build(options).ToList();

        Assert.AreEqual("4", arguments[arguments.IndexOf("--jobs") + 1]);
        CollectionAssert.Contains(arguments, "--trim-jobs");
        Assert.AreEqual("auto", arguments[arguments.IndexOf("--trim-jobs") + 1]);
    }

    [TestMethod]
    public void Preview_QuotesPathsContainingSpaces()
    {
        var preview = CliCommandBuilder.Preview(@"C:\Program Files\PUBG Trim\pubg-highlight-trim.exe", [@"C:\Video Clips", "--merge"]);

        StringAssert.StartsWith(preview, "\"C:\\Program Files\\PUBG Trim\\pubg-highlight-trim.exe\"");
        StringAssert.Contains(preview, "\"C:\\Video Clips\"");
    }
}
