using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class CliOutputParserTests
{
    private readonly CliOutputParser _parser = new();

    [TestMethod]
    public void Parse_IncludeLineReturnsProgressAndStatus()
    {
        var result = _parser.Parse("[03/12] INCLUDE own-kill 42.000s 37.000-43.000 ocr lang=en | sample.mp4");

        Assert.AreEqual(3, result.Current);
        Assert.AreEqual(12, result.Total);
        Assert.IsTrue(result.Included);
        Assert.IsFalse(result.Skipped);
    }

    [TestMethod]
    public void Parse_ScanProgressReturnsPhaseWorkersAndCounts()
    {
        var result = _parser.Parse("PROGRESS {\"phase\":\"scan\",\"current\":3,\"total\":12,\"workers\":2}");

        Assert.AreEqual("scan", result.Phase);
        Assert.AreEqual(3, result.Current);
        Assert.AreEqual(12, result.Total);
        Assert.AreEqual(2, result.Workers);
    }

    [TestMethod]
    public void Parse_IncompleteScanProgressDoesNotCrash()
    {
        var result = _parser.Parse("PROGRESS {\"phase\":\"scan\"}");

        Assert.IsNull(result.Current);
        Assert.IsNull(result.Total);
    }

    [TestMethod]
    public void Parse_SummaryLineDeserializesContract()
    {
        const string line = "SUMMARY {\"source_count\":12,\"included_count\":5,\"skipped_count\":7,\"dry_run\":false,\"output_dir\":\"C:\\\\output\",\"merge_output\":\"C:\\\\output.mp4\",\"merge_duration_sec\":31.2,\"merge_size_mb\":90.4,\"profile_total_sec\":45.6}";

        var result = _parser.Parse(line);

        Assert.IsNotNull(result.Summary);
        Assert.AreEqual(12, result.Summary.SourceCount);
        Assert.AreEqual(5, result.Summary.IncludedCount);
        Assert.AreEqual(@"C:\output.mp4", result.Summary.MergeOutput);
        Assert.AreEqual(45.6, result.Summary.TotalSeconds);
    }

    [TestMethod]
    public void Parse_MalformedSummaryDoesNotCrash()
    {
        var result = _parser.Parse("SUMMARY not-json");

        Assert.IsNull(result.Summary);
    }
}
