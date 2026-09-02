from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).parents[1]


def test_ui_manifest_declares_per_monitor_v2_and_legacy_fallback():
    manifest = ROOT / "ui" / "PubgHighlightTrim.Ui" / "app.manifest"
    root = ET.parse(manifest).getroot()
    settings = next(element for element in root.iter() if element.tag.endswith("windowsSettings"))
    values = {element.tag.rsplit("}", 1)[-1]: (element.text or "").strip() for element in settings}
    assert values["dpiAwareness"] == "PerMonitorV2"
    assert values["dpiAware"] == "true/pm"


def test_ui_build_verifies_the_final_archive():
    build_script = (ROOT / "scripts" / "build_ui_windows.ps1").read_text(encoding="utf-8")
    verify_script = (ROOT / "scripts" / "verify_windows_ui_bundle.ps1").read_text(encoding="utf-8")
    assert "verify_windows_ui_bundle.ps1" in build_script
    assert "-Package $zipPath" in build_script
    assert "Expand-Archive" in verify_script
    assert "cli\\pubg-highlight-trim.exe" in verify_script
