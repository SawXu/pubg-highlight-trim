using PubgHighlightTrim.Ui.Core;

namespace PubgHighlightTrim.Ui.Tests;

[TestClass]
public sealed class TargetSelectionTests
{
    [TestMethod]
    public void DefaultAndEmptySelectionUseAll()
    {
        var selection = new TargetSelection();
        Assert.IsTrue(selection.All);
        Assert.AreEqual("both", selection.Target);
        Assert.IsFalse(selection.SplitMergeByTarget);

        selection.SetSelected(DetectionTarget.All, false);
        Assert.IsTrue(selection.All);
        selection.SetSelected(DetectionTarget.SelfDeath, true);
        selection.SetSelected(DetectionTarget.SelfDeath, false);
        Assert.IsTrue(selection.All);
        selection.SetSelected(DetectionTarget.OwnKill, true);
        selection.SetSelected(DetectionTarget.OwnKill, false);
        Assert.IsTrue(selection.All);
    }

    [TestMethod]
    public void SpecificTargetsCanBeCombinedAndAllClearsThem()
    {
        var selection = new TargetSelection();
        selection.SetSelected(DetectionTarget.SelfDeath, true);
        Assert.IsFalse(selection.All);
        Assert.AreEqual("self-death", selection.Target);
        Assert.IsFalse(selection.SplitMergeByTarget);

        selection.SetSelected(DetectionTarget.OwnKill, true);
        Assert.IsTrue(selection.SelfDeath);
        Assert.IsTrue(selection.OwnKill);
        Assert.IsFalse(selection.All);
        Assert.AreEqual("both", selection.Target);
        Assert.IsTrue(selection.SplitMergeByTarget);

        selection.SetSelected(DetectionTarget.SelfDeath, false);
        Assert.AreEqual("own-kill", selection.Target);
        Assert.IsFalse(selection.SplitMergeByTarget);

        selection.SetSelected(DetectionTarget.SelfDeath, true);
        selection.SetSelected(DetectionTarget.All, true);
        Assert.IsTrue(selection.All);
        Assert.IsFalse(selection.SelfDeath);
        Assert.IsFalse(selection.OwnKill);
        Assert.IsFalse(selection.SplitMergeByTarget);
    }
}
