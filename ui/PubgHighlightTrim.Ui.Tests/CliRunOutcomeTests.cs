using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class CliRunOutcomeTests
{
    [TestMethod]
    [DataRow(0)]
    [DataRow(2)]
    public void PartialReadFailuresAreVisibleEvenWhenOtherSourcesHaveNoEvents(int included)
    {
        var summary = new CliOutputParser().Parse(
            $"SUMMARY {{\"source_count\":3,\"included_count\":{included},\"skipped_count\":1,\"unreadable_count\":1}}").Summary;

        var outcome = CliRunOutcome.From(new CliRunResult(included > 0 ? 0 : 2, false), summary);

        Assert.AreEqual("处理完成，部分视频读取失败", outcome.Title);
        StringAssert.Contains(outcome.Description, "读取失败 1 / 3");
        StringAssert.Contains(outcome.Description, $"保留 {included} 个片段");
        Assert.AreEqual("!", outcome.Glyph);
    }

    [TestMethod]
    [DataRow(0, 1, false, "处理完成", "输出已生成")]
    [DataRow(2, 0, true, "扫描完成，没有匹配事件", "仅扫描结果已写入输出目录")]
    public void ReadableSourcesKeepExistingSuccessAndNoEventResults(
        int exitCode, int included, bool dryRun, string title, string status)
    {
        // Older summaries can omit unreadable_count.
        var summary = new CliOutputParser().Parse(
            $"SUMMARY {{\"source_count\":1,\"included_count\":{included},\"dry_run\":{dryRun.ToString().ToLowerInvariant()}}}").Summary;

        var outcome = CliRunOutcome.From(new CliRunResult(exitCode, false), summary);

        Assert.AreEqual(title, outcome.Title);
        Assert.AreEqual(status, outcome.Status);
        Assert.AreEqual("✓", outcome.Glyph);
        Assert.IsTrue(outcome.Completed);
    }

    [TestMethod]
    [DataRow(1, false, true, "处理失败")]
    [DataRow(2, false, false, "处理失败")]
    [DataRow(0, false, false, "处理失败")]
    [DataRow(-1, false, true, "处理失败")]
    [DataRow(2, true, true, "任务已停止")]
    public void FatalAndCancelledResultsTakePrecedenceOverSummary(
        int exitCode, bool cancelled, bool hasSummary, string title)
    {
        var summary = hasSummary ? new CliRunSummary { SourceCount = 1, UnreadableCount = 1 } : null;

        var outcome = CliRunOutcome.From(new CliRunResult(exitCode, cancelled), summary);

        Assert.AreEqual(title, outcome.Title);
        Assert.IsFalse(outcome.Completed);
    }

    [TestMethod]
    public void AllUnreadableSourcesReportReadFailureInsteadOfNoEvents()
    {
        var summary = new CliOutputParser().Parse(
            "SUMMARY {\"source_count\":1,\"included_count\":0,\"skipped_count\":1,\"unreadable_count\":1}").Summary;

        var outcome = CliRunOutcome.From(new CliRunResult(2, false), summary);

        Assert.AreEqual("视频读取失败", outcome.Title);
        StringAssert.Contains(outcome.Description, "1 / 1");
        Assert.AreEqual("读取失败，请检查运行记录", outcome.Status);
        Assert.AreEqual("!", outcome.Glyph);
    }
}
