namespace PubgHighlightTrim.Ui.Core;

public enum DetectionTarget
{
    All,
    SelfDeath,
    OwnKill,
}

public sealed class TargetSelection
{
    public bool All => !SelfDeath && !OwnKill;
    public bool SelfDeath { get; private set; }
    public bool OwnKill { get; private set; }
    public string Target => SelfDeath == OwnKill ? "both" : SelfDeath ? "self-death" : "own-kill";
    public bool SplitMergeByTarget => SelfDeath && OwnKill;

    public void SetSelected(DetectionTarget target, bool selected)
    {
        switch (target)
        {
            case DetectionTarget.All when selected:
                SelfDeath = false;
                OwnKill = false;
                break;
            case DetectionTarget.SelfDeath:
                SelfDeath = selected;
                break;
            case DetectionTarget.OwnKill:
                OwnKill = selected;
                break;
        }
    }
}
