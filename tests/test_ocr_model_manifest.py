import hashlib
import json
import subprocess
import sys
from pathlib import Path
from scripts.download_ocr_models import download


ROOT = Path(__file__).parents[1]
MANIFEST = ROOT / "rust" / "ocr_engine" / "models" / "pp_ocrv6_medium.json"
VERIFY = ROOT / "scripts" / "verify_ocr_models.py"


def test_manifest_declares_three_languages_and_offline_sources():
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    assert manifest["contract"]["languages"] == ["zh-Hans", "zh-Hant", "en"]
    assert all(manifest[role]["source"].startswith("https://") for role in ("detector", "recognizer"))
    assert all(len(manifest[role]["sha256"]) == 64 for role in ("detector", "recognizer", "dictionary"))


def test_verifier_rejects_unpinned_or_missing_assets(tmp_path):
    (tmp_path / "det.onnx").write_bytes(b"fixture")
    (tmp_path / "rec.onnx").write_bytes(b"fixture")
    (tmp_path / "dict.txt").write_bytes(b"fixture")
    result = subprocess.run(
        [sys.executable, str(VERIFY), "--model-dir", str(tmp_path)],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1
    assert "sha256 mismatch" in result.stdout


def test_fixture_includes_required_language_event_samples():
    fixture = ROOT / "rust" / "ocr_engine" / "fixtures" / "pubg_ocr_baseline.jsonl"
    rows = [json.loads(line) for line in fixture.read_text(encoding="utf-8").splitlines()]
    assert {row["language"] for row in rows} == {"zh-Hans", "zh-Hant", "en"}
    assert {row["action"] for row in rows} >= {"knock", "eliminate", "ignored"}


def test_downloader_rejects_hash_mismatch_without_leaving_partial_file(tmp_path):
    from unittest.mock import patch

    class Response:
        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

        def read(self, _size):
            if hasattr(self, "done"):
                return b""
            self.done = True
            return b"fixture"

    destination = tmp_path / "det.onnx"
    with patch("scripts.download_ocr_models.urlopen", return_value=Response()):
        try:
            download("https://example.invalid/model", destination, "0" * 64)
        except RuntimeError:
            pass
        else:
            raise AssertionError("expected hash mismatch")
    assert not destination.exists()
    assert not destination.with_suffix(".onnx.part").exists()
