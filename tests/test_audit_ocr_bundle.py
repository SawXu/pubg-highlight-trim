import importlib.util
from pathlib import Path
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "audit_ocr_bundle", Path(__file__).resolve().parents[1] / "scripts/audit_ocr_bundle.py"
)
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


def test_detects_packages_models_metadata_and_zipped_modules(tmp_path):
    for name in ["_internal/paddleocr/__init__.py", "_internal/paddlepaddle-3.dist-info/METADATA", "_internal/vendor/paddlex_cache/model.json"]:
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.touch()
    with zipfile.ZipFile(tmp_path / "base_library.zip", "w") as archive:
        archive.writestr("paddlex/__init__.pyc", b"")
    residue = module.paddle_residue(tmp_path)
    assert "_internal/paddleocr/__init__.py" in residue
    assert "_internal/paddlepaddle-3.dist-info/METADATA" in residue
    assert "_internal/vendor/paddlex_cache/model.json" in residue
    assert "base_library.zip!paddlex/__init__.pyc" in residue


def test_rust_models_python_orchestration_and_notices_are_allowed(tmp_path):
    for name in ["ocr_engine.exe", "ocr_models/rec.onnx", "_internal/python312.dll", "THIRD_PARTY_NOTICES.md"]:
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.touch()
    assert module.paddle_residue(tmp_path) == []
