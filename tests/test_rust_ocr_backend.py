from pubg_highlight_trim.rust_ocr import _response_to_paddle_result


def test_engine_response_maps_to_paddle_result_shape():
    response = {
        "text": "您以M24擊倒enemy1",
        "scores": [0.99, 0.97],
        "boxes": [[1, 2, 30, 10]],
        "status": "ok",
        "method": "onnx-cpu",
        "inference_ms": 12.5,
        "frame_ms": 13.0,
    }
    item = _response_to_paddle_result(response)
    assert item["rec_texts"] == ["您以M24擊倒enemy1"]
    assert item["rec_scores"] == [0.99, 0.97]
    assert item["boxes"] == [[1, 2, 30, 10]]
    assert item["status"] == "ok"
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
