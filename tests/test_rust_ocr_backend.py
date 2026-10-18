import pytest

from pubg_highlight_trim.rust_ocr import (
    _describe_exit,
    _response_to_paddle_result,
    normalize_provider,
)


def test_reading_order_fix_invalidates_old_rust_detection_cache():
    from unittest.mock import patch

    from pubg_highlight_trim.cache import ocr_engine_descriptor

    with patch.dict("os.environ", {"PUBG_OCR_BACKEND": "rust", "PUBG_OCR_PROVIDER": "cpu"}):
        assert ocr_engine_descriptor() != "rust-onnx:cpu"
    with patch.dict("os.environ", {"PUBG_OCR_BACKEND": "paddle"}):
        assert ocr_engine_descriptor() == "paddle"


def test_engine_response_maps_to_paddle_result_shape():
    response = {
        "text": "您以M24擊倒enemy1",
        "scores": [0.99, 0.97],
        "boxes": [[1, 2, 30, 10]],
        "status": "ok",
        "method": "onnx-cpu",
        "inference_ms": 12.5,
        "frame_ms": 13.0,
        "provider": "cpu",
        "device_id": None,
        "fallback_reason": None,
        "model_version": "PP-OCRv6_medium_onnx",
    }
    item = _response_to_paddle_result(response)
    assert item["rec_texts"] == ["您以M24擊倒enemy1"]
    assert item["rec_scores"] == [0.99, 0.97]
    assert item["boxes"] == [[1, 2, 30, 10]]
    assert item["status"] == "ok"
    assert item["provider"] == "cpu"
    assert item["model_version"] == "PP-OCRv6_medium_onnx"
    assert "res" not in item


def test_engine_response_defaults_for_missing_fields():
    item = _response_to_paddle_result({})
    assert item["rec_texts"] == [""]
    assert item["rec_scores"] == []
    assert item["boxes"] == []
    assert item["status"] == "unknown"


def test_predict_text_consumes_rust_backend_output():
    from pubg_highlight_trim.ocr import _predict_text

    class _FakeRustBackend:
        def predict(self, crop):
            return [_response_to_paddle_result({"text": "abc", "scores": [0.9]})]

    text, scores, _elapsed = _predict_text(_FakeRustBackend(), object())
    assert text == "abc"
    assert scores == "0.900"


def test_normalize_provider_defaults_to_cpu():
    assert normalize_provider(None) == "cpu"
    assert normalize_provider("") == "cpu"
    assert normalize_provider("default") == "cpu"
    assert normalize_provider("auto") == "cpu"


def test_normalize_provider_directml_variants():
    assert normalize_provider("DirectML") == "directml"
    assert normalize_provider("dml") == "directml"
    assert normalize_provider("directml-only") == "directml-only"
    assert normalize_provider("dml-only") == "directml-only"


def test_normalize_provider_rejects_unknown_values():
    with pytest.raises(ValueError):
        normalize_provider("cuda")


def test_describe_exit_hints_for_missing_dll():
    message = _describe_exit(-1073741515)
    assert "code -1073741515" in message
    assert "vcruntime140.dll" in message


def test_describe_exit_plain_code_when_unmapped():
    assert _describe_exit(1) == "Rust OCR exited with code 1"


@pytest.mark.parametrize("status", ["ok", "provider-fallback", "model-unavailable", "inference-error", "invalid-roi", "empty-roi", "unknown"])
def test_engine_status_contract(status):
    import json
    from io import StringIO
    from unittest.mock import Mock

    np = pytest.importorskip("numpy")
    from pubg_highlight_trim.rust_ocr import RustOcrBackend

    backend = RustOcrBackend.__new__(RustOcrBackend)
    backend._process = Mock(
        stdin=StringIO(),
        stdout=StringIO(json.dumps({"status": status, "provider": "none"}) + "\n"),
    )
    backend._process.poll.return_value = None
    crop = np.zeros((2, 2, 3), dtype=np.uint8)
    if status in {"ok", "provider-fallback"}:
        assert backend.predict(crop)[0]["status"] == status
    else:
        with pytest.raises(RuntimeError, match=f"status={status}"):
            backend.predict(crop)
