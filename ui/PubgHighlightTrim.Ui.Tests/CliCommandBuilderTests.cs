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
            false);

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
            true);

        var arguments = CliCommandBuilder.Build(options);

        CollectionAssert.AreEqual(
            new[] { "--files", @"C:\Videos\second.mp4", @"C:\Videos\first.mp4" },
            arguments.Take(3).ToArray());
    }

    [TestMethod]
    public void Preview_QuotesPathsContainingSpaces()
    {
        var preview = CliCommandBuilder.Preview(@"C:\Program Files\PUBG Trim\pubg-highlight-trim.exe", [@"C:\Video Clips", "--merge"]);

        StringAssert.StartsWith(preview, "\"C:\\Program Files\\PUBG Trim\\pubg-highlight-trim.exe\"");
        StringAssert.Contains(preview, "\"C:\\Video Clips\"");
    }
}
