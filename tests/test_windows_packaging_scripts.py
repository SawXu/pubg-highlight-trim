from pathlib import Path

ROOT = Path(__file__).parents[1]


def test_default_windows_build_uses_opencv_and_excludes_paddle():
    script = (ROOT / "scripts" / "build_windows.ps1").read_text(encoding="utf-8")
    assert ".[ocr,build]" in script
    assert '"--exclude-module", "paddleocr"' in script
    assert '"--exclude-module", "paddle"' in script
    assert '"--exclude-module", "paddlex"' in script


def test_paddle_artifact_and_smoke_contract():
    script = (ROOT / "scripts" / "build_windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    smoke = (ROOT / "scripts" / "smoke_windows_bundle.ps1").read_text(encoding="utf-8")
    assert ". [paddle,build]" not in script
    assert ".[paddle,build]" in script
    assert "pubg-highlight-trim-paddle-windows-x64.zip" in script
    assert "pubg-highlight-trim-paddle-windows-x64.zip" in workflow
    assert "icacls" in smoke
    assert "--scan-only" in smoke
    assert "PUBG_OCR_BACKEND" in smoke
    assert "StandardInputEncoding = New-Object System.Text.UTF8Encoding($false)" in smoke
    assert "StandardInput.WriteLine($requestJson)" in smoke
    assert "$input | & $engine" not in smoke
