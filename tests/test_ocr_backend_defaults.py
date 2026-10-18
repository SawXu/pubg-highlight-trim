"""Default backend selection must never silently enable the Paddle runtime."""
import sys
from types import ModuleType
from unittest.mock import Mock, patch

import pytest

from pubg_highlight_trim.cache import ocr_engine_descriptor
from pubg_highlight_trim.ocr import OcrUnavailable, load_backend
from pubg_highlight_trim.runtime import ocr_backend_name


@pytest.fixture(autouse=True)
def clean_backend_environment(monkeypatch):
    monkeypatch.delenv("PUBG_OCR_BACKEND", raising=False)
    monkeypatch.delenv("PUBG_OCR_PROVIDER", raising=False)


def test_default_loads_rust_without_importing_paddle():
    backend = Mock(provider="cpu")
    with patch.dict(sys.modules, {"cv2": ModuleType("cv2"), "paddleocr": None}), patch(
        "pubg_highlight_trim.rust_ocr.rust_backend_from_environment", return_value=backend
    ) as factory:
        assert load_backend()[1] is backend
        factory.assert_called_once_with()
    assert ocr_backend_name() == "rust"
    assert ocr_engine_descriptor() == "rust-onnx:reading-order-v2:cpu"


def test_explicit_rust_and_default_share_cache_descriptor(monkeypatch):
    default = ocr_engine_descriptor()
    monkeypatch.setenv("PUBG_OCR_BACKEND", " RUST ")
    assert ocr_engine_descriptor() == default
    monkeypatch.setenv("PUBG_OCR_BACKEND", "paddle")
    assert ocr_engine_descriptor() == "paddle"
    assert ocr_engine_descriptor() != default


@pytest.mark.parametrize("value", ["", "automatic", "padle"])
def test_invalid_backend_does_not_silently_load_paddle(monkeypatch, value):
    monkeypatch.setenv("PUBG_OCR_BACKEND", value)
    with pytest.raises(ValueError, match="unknown OCR backend"):
        load_backend()
    with pytest.raises(ValueError, match="unknown OCR backend"):
        ocr_engine_descriptor()


def test_rust_failure_does_not_fall_back_to_paddle():
    with patch.dict(sys.modules, {"cv2": ModuleType("cv2"), "paddleocr": None}), patch(
        "pubg_highlight_trim.rust_ocr.rust_backend_from_environment",
        side_effect=RuntimeError("engine missing"),
    ):
        with pytest.raises(RuntimeError, match="engine missing"):
            load_backend()


def test_explicit_paddle_reports_compatibility_install(monkeypatch):
    monkeypatch.setenv("PUBG_OCR_BACKEND", "paddle")
    with patch.dict(sys.modules, {"cv2": ModuleType("cv2"), "paddleocr": None}), patch(
        "pubg_highlight_trim.ocr.configure_paddlex_cache"
    ):
        with pytest.raises(OcrUnavailable, match=r"\.\[paddle\]"):
            load_backend()
