from __future__ import annotations

from pathlib import Path

from scripts.prune_windows_bundle import prune_bundle


def write_file(path: Path, content: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)


def test_prune_removes_duplicate_ffmpeg_dll_and_dev_files(tmp_path: Path) -> None:
    bundle = tmp_path / "pubg-highlight-trim"
    internal = bundle / "_internal"
    write_file(internal / "vendor" / "ffmpeg" / "avcodec-test.dll", b"dll")
    write_file(internal / "avcodec-test.dll", b"dll")
    write_file(internal / "paddle" / "include" / "header.h", b"header")
    write_file(internal / "paddle" / "base" / "libpaddle.lib", b"lib")
    write_file(internal / "paddle" / "tensor.pyi", b"stub")
    write_file(internal / "cv2" / "data" / "haarcascade_test.xml", b"cascade")
    write_file(internal / "vendor" / "ffmpeg" / "ffmpeg.exe", b"exe")

    pruned = prune_bundle(bundle)

    assert sum(item.size for item in pruned) == len(b"dllheaderlibstubcascade")
    assert not (internal / "avcodec-test.dll").exists()
    assert not (internal / "paddle" / "include").exists()
    assert not (internal / "paddle" / "base" / "libpaddle.lib").exists()
    assert not (internal / "paddle" / "tensor.pyi").exists()
    assert not (internal / "cv2" / "data" / "haarcascade_test.xml").exists()
    assert (internal / "vendor" / "ffmpeg" / "avcodec-test.dll").exists()
    assert (internal / "vendor" / "ffmpeg" / "ffmpeg.exe").exists()


def test_prune_keeps_non_identical_top_level_ffmpeg_dll(tmp_path: Path) -> None:
    bundle = tmp_path / "pubg-highlight-trim"
    internal = bundle / "_internal"
    write_file(internal / "vendor" / "ffmpeg" / "avcodec-test.dll", b"vendor")
    write_file(internal / "avcodec-test.dll", b"top-level")

    pruned = prune_bundle(bundle)

    assert pruned == []
    assert (internal / "avcodec-test.dll").exists()
