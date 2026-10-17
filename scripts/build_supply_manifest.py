"""Generate SUPPLY_CHAIN_MANIFEST.json for a built bundle.

Records every shipped file (path, sha256, size) plus component metadata
(version, source, license) so the release is auditable offline.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tomllib
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
MODEL_MANIFEST = REPO_ROOT / "rust" / "ocr_engine" / "models" / "pp_ocrv6_medium.json"
PYPROJECT = REPO_ROOT / "pyproject.toml"

CRT_DLLS = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll"]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def package_version(name: str) -> str:
    try:
        return version(name)
    except PackageNotFoundError:
        return "unknown"


def ffmpeg_version(ffmpeg: Path) -> str:
    try:
        out = subprocess.run(
            [str(ffmpeg), "-version"], capture_output=True, text=True, timeout=10
        )
        first = out.stdout.splitlines()[0] if out.stdout else ""
        match = re.search(r"ffmpeg version (\S+)", first)
        return match.group(1) if match else first or "unknown"
    except (OSError, subprocess.SubprocessError, IndexError):
        return "unknown"


def project_version() -> str:
    return tomllib.loads(PYPROJECT.read_text(encoding="utf-8"))["project"]["version"]


def components(bundle: Path) -> list[dict]:
    model_manifest = json.loads(MODEL_MANIFEST.read_text(encoding="utf-8"))
    ffmpeg = bundle / "_internal" / "vendor" / "ffmpeg" / "ffmpeg.exe"
    entries = [
        {
            "name": "pubg-highlight-trim",
            "kind": "application",
            "version": project_version(),
            "source": "this repository (PyInstaller frozen)",
            "license": "GPL-3.0-or-later",
            "files": ["pubg-highlight-trim.exe"],
        },
        {
            "name": "pubg-ocr-engine",
            "kind": "application",
            "version": project_version(),
            "source": "this repository, rust/ocr_engine (cargo --features cli,directml)",
            "license": "GPL-3.0-or-later",
            "files": ["ocr_engine.exe"],
        },
        {
            "name": "ONNX Runtime",
            "kind": "static library inside ocr_engine.exe",
            "version": "1.20.0",
            "source": "https://parcel.pyke.io ortrs_static-v1.20.0-x86_64-pc-windows-msvc.tgz"
            " (via ort-sys 2.0.0-rc.9, sha256 BB02D856F2747E4863C344BE38AF2A645E660E9055C871EE9984907497BE590A)",
            "license": "MIT",
            "files": ["ocr_engine.exe"],
        },
        {
            "name": "ppocr-rs",
            "kind": "rust crate, vendored",
            "version": "0.7.3",
            "source": "https://github.com/dariofinardi/PaddleOCR-OCR-rs vendored at vendor/ppocr-rs",
            "license": "Apache-2.0",
            "files": ["ocr_engine.exe"],
        },
        {
            "name": "ort",
            "kind": "rust crate",
            "version": "2.0.0-rc.9",
            "source": "https://crates.io/crates/ort",
            "license": "MIT OR Apache-2.0",
            "files": ["ocr_engine.exe"],
        },
        {
            "name": "DirectML",
            "kind": "Windows inbox component, delay-loaded",
            "source": "provided by Windows 10 1903+ (directml.dll, d3d12.dll, dxgi.dll, dxcore.dll)",
            "license": "proprietary, Microsoft Windows",
            "notes": "optional execution provider; CPU provider requires no DirectML",
            "files": [],
        },
        {
            "name": "Visual C++ runtime",
            "kind": "redistributable DLLs",
            "source": "Microsoft Visual C++ 2015-2022 Redistributable (x64), copied next to ocr_engine.exe",
            "license": "Microsoft software license terms (redistributable)",
            "files": CRT_DLLS,
        },
        {
            "name": "PP-OCRv6 medium ONNX models",
            "kind": "model assets",
            "version": model_manifest["model_version"],
            "source": "; ".join(
                f"{role}: {model_manifest[role]['source']}"
                for role in ("detector", "recognizer", "dictionary")
            ),
            "license": model_manifest["license"],
            "files": [
                f"ocr_models/{model_manifest[role]['file']}"
                for role in ("detector", "recognizer", "dictionary")
            ]
            + ["ocr_models/pp_ocrv6_medium.json"],
        },
        {
            "name": "FFmpeg / FFprobe",
            "kind": "binary",
            "version": ffmpeg_version(ffmpeg),
            "source": "https://github.com/BtbN/FFmpeg-Builds ffmpeg-master-latest-win64-gpl-shared.zip",
            "license": "GPL-3.0-or-later",
            "files": [
                "_internal/vendor/ffmpeg/ffmpeg.exe",
                "_internal/vendor/ffmpeg/ffprobe.exe",
            ],
        },
        {
            "name": "PyInstaller bootloader",
            "kind": "freezer",
            "version": package_version("pyinstaller"),
            "source": "https://github.com/pyinstaller/pyinstaller",
            "license": "GPL-2.0-or-later with bootloader exception",
            "files": ["pubg-highlight-trim.exe"],
        },
        {
            "name": "PaddleOCR / PaddlePaddle / PaddleX",
            "kind": "python packages in _internal",
            "version": f"paddleocr {package_version('paddleocr')}, "
            f"paddlepaddle {package_version('paddlepaddle')}, "
            f"paddlex {package_version('paddlex')}",
            "source": "https://pypi.org",
            "license": "Apache-2.0",
            "notes": "legacy OCR backend; not used when PUBG_OCR_BACKEND=rust",
            "files": [],
        },
        {
            "name": "OpenCV contrib",
            "kind": "python package in _internal",
            "version": package_version("opencv-contrib-python"),
            "source": "https://pypi.org",
            "license": "Apache-2.0",
            "files": ["_internal/cv2"],
        },
    ]
    return entries


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    bundle = args.bundle
    if not bundle.is_dir():
        raise FileNotFoundError(f"bundle not found: {bundle}")

    files = []
    for path in sorted(bundle.rglob("*")):
        if path.is_file():
            files.append(
                {
                    "path": path.relative_to(bundle).as_posix(),
                    "sha256": sha256(path),
                    "bytes": path.stat().st_size,
                }
            )

    manifest = {
        "schema": "pubg-highlight-trim-supply-chain/v1",
        "bundle": bundle.name,
        "generated_by": "scripts/build_supply_manifest.py",
        "policy": "release runtime must not download models, binaries, or caches",
        "components": components(bundle),
        "files": files,
    }

    output = args.output or bundle / "SUPPLY_CHAIN_MANIFEST.json"
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {output} ({len(files)} files hashed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
