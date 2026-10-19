namespace PubgHighlightTrim.Ui.Core;

public sealed record CliRunOutcome(
    string Title,
    string Description,
    string Status,
    string Glyph,
    bool Completed)
{
    public static CliRunOutcome From(CliRunResult result, CliRunSummary? summary)
    {
        if (result.Cancelled)
        {
            return new("任务已停止", "CLI 及其子进程已经终止。", "已取消", "■", false);
        }

        if (summary is null || result.ExitCode is not (0 or 2))
        {
            return new("处理失败", $"CLI 退出码：{result.ExitCode}，请展开运行日志查看返回的信息。", "运行失败", "!", false);
        }

        if (summary.UnreadableCount > 0)
        {
            var allUnreadable = summary.SourceCount > 0 && summary.UnreadableCount >= summary.SourceCount;
            return new(
                allUnreadable ? "视频读取失败" : "处理完成，部分视频读取失败",
                $"读取失败 {summary.UnreadableCount} / {summary.SourceCount} 个源文件，保留 {summary.IncludedCount} 个片段。请检查运行日志和 CSV 记录。",
                allUnreadable ? "读取失败，请检查运行记录" : "部分视频读取失败，请检查运行记录",
                "!",
                true);
        }

        return new(
            summary.IncludedCount > 0 ? "处理完成" : "扫描完成，没有匹配事件",
            $"扫描 {summary.SourceCount} 个源文件，保留 {summary.IncludedCount} 个片段",
            summary.DryRun ? "仅扫描结果已写入输出目录" : "输出已生成",
            "✓",
            true);
    }
}
