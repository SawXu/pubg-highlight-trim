"""Optional real Rust OCR -> production event extraction regression tests."""
import os
from pathlib import Path

import pytest

from pubg_highlight_trim import ocr
from pubg_highlight_trim.game_languages import get_game_language_profile
from pubg_highlight_trim.rust_ocr import RustOcrBackend


FIXTURES = Path(__file__).parent / "fixtures" / "missed_event_ocr"


@pytest.fixture(scope="module")
def rust_backend():
    executable = os.environ.get("PUBG_OCR_ENGINE")
    model_dir = os.environ.get("PUBG_OCR_MODEL_DIR")
    if not executable or not model_dir:
        pytest.skip("set PUBG_OCR_ENGINE and PUBG_OCR_MODEL_DIR to run real OCR regressions")
    # Explicitly configured but broken engines/models must fail, not silently skip.
    backend = RustOcrBackend(executable, model_dir, threads=1, provider="cpu")
    try:
        yield backend
    finally:
        backend.close()


@pytest.mark.parametrize(
    "image,target,action,subject_identity",
    [
        ("23.04.41.02_32.png", "own-kill", "knock", "heartbreak_kidz"),
        ("23.09.16.04_64.png", "own-kill", "eliminate", "lc100p"),
        ("23.20.15.09_2.png", "self-death", "knock", "ag_guhan"),
    ],
    ids=["02-32s-own-knock", "04-64s-own-eliminate", "09-2s-self-knock"],
)
def test_real_missed_event_roi(rust_backend, image, target, action, subject_identity):
    import cv2

    crop = cv2.imread(str(FIXTURES / image))
    assert crop is not None, image
    assert crop.shape[1] == 768
    text, _scores, _elapsed = ocr._predict_text(rust_backend, crop)
    profile = get_game_language_profile("zh-Hant")
    classified = ocr.classify_target_kind(text, "both", profile)
    assert classified is not None, f"{image}: not classified; OCR={text!r}"
    assert classified[1] == target, f"{image}: OCR={text!r}"
    events = ocr.extract_text_events(text, "both", profile)
    # Check the player's identity, not a brittle full OCR transcription: tags,
    # whitespace and trailing HUD noise need not be identical across models.
    matching = [
        event for event in events
        if event.target == target
        and event.action == action
        and subject_identity in event.subject.casefold()
        and event.key.startswith(f"{target}:{action}:")
        and subject_identity in event.key.casefold()
    ]
    assert matching, f"{image}: OCR={text!r}; events={events!r}"
