using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class CliLocatorTests
{
    [TestMethod]
    public void Find_PrefersConfiguredCliPath()
    {
        var tempFile = Path.GetTempFileName();
        var original = Environment.GetEnvironmentVariable("PUBG_HIGHLIGHT_TRIM_CLI");
        try
        {
            Environment.SetEnvironmentVariable("PUBG_HIGHLIGHT_TRIM_CLI", tempFile);
            Assert.AreEqual(tempFile, CliLocator.Find(Path.GetTempPath()));
        }
        finally
        {
            Environment.SetEnvironmentVariable("PUBG_HIGHLIGHT_TRIM_CLI", original);
            File.Delete(tempFile);
        }
    }
}
