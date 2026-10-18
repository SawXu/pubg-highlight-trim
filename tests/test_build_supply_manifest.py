from __future__ import annotations

import json
import sys
from pathlib import Path

from scripts.build_supply_manifest import components, main


def test_rust_bundle_manifest_omits_paddle_when_not_present(tmp_path: Path) -> None:
    bundle = tmp_path / "bundle"
    (bundle / "_internal" / "vendor" / "ffmpeg").mkdir(parents=True)
    (bundle / "_internal" / "vendor" / "ffmpeg" / "ffmpeg.exe").write_bytes(b"ffmpeg")
    (bundle / "_internal" / "vendor" / "ffmpeg" / "ffprobe.exe").write_bytes(b"ffprobe")
    (bundle / "pubg-highlight-trim.exe").write_bytes(b"app")
    (bundle / "ocr_engine.exe").write_bytes(b"engine")
    (bundle / "python312.dll").write_bytes(b"python")

    entries = components(bundle)
    names = {entry["name"] for entry in entries}

    assert next(item for item in entries if item["name"] == "CPython")["version"] == sys.version.split()[0]

    assert "OpenCV contrib" not in names
    assert "PaddleOCR / PaddlePaddle / PaddleX" not in names
    assert "DirectML" in names
    assert next(item for item in entries if item["name"] == "pubg-ocr-engine")["version"] == "0.1.0"
    assert "FFmpeg / FFprobe" in names


def test_manifest_excludes_itself_on_consecutive_runs(tmp_path: Path, monkeypatch) -> None:
    bundle = tmp_path / "bundle"
    bundle.mkdir()
    (bundle / "pubg-highlight-trim.exe").write_bytes(b"app")
    output = bundle / "SUPPLY_CHAIN_MANIFEST.json"
    monkeypatch.setattr("sys.argv", ["build_supply_manifest.py", str(bundle)])

    assert main() == 0
    first = output.read_text(encoding="utf-8")
    assert main() == 0
    second = output.read_text(encoding="utf-8")

    assert first == second
    assert all(item["path"] != "SUPPLY_CHAIN_MANIFEST.json" for item in json.loads(second)["files"])


def test_manifest_hashes_files_and_records_bundle(tmp_path: Path, monkeypatch) -> None:
    bundle = tmp_path / "bundle"
    bundle.mkdir()
    (bundle / "pubg-highlight-trim.exe").write_bytes(b"app")
    output = tmp_path / "manifest.json"

    monkeypatch.setattr("sys.argv", ["build_supply_manifest.py", str(bundle), "--output", str(output)])
    assert main() == 0

    manifest = json.loads(output.read_text(encoding="utf-8"))
    assert manifest["bundle"] == "bundle"
    assert {item["name"] for item in manifest["components"]} == {"pubg-highlight-trim", "PyInstaller bootloader", "DirectML"}
    assert manifest["files"] == [
        {"path": "pubg-highlight-trim.exe", "sha256": "a172cedcae47474b615c54d510a5d84a8dea3032e958587430b413538be3f333", "bytes": 3}
    ]
