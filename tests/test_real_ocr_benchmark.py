import json
from pathlib import Path


FIXTURE = Path(__file__).parents[1] / "rust" / "ocr_engine" / "fixtures" / "real"


def test_real_benchmark_fixture_has_language_coverage_and_images():
    rows = [json.loads(line) for line in (FIXTURE / "baseline.jsonl").read_text(encoding="utf-8").splitlines()]
    assert len(rows) == 18
    assert {row["language"] for row in rows} == {"zh-Hans", "zh-Hant", "en"}
    assert {row["target"] for row in rows} == {"self-death", "own-kill"}
    assert all(sum(row["target"] == target and row["language"] == lang for row in rows) >= 1 for target in {"self-death", "own-kill"} for lang in {"zh-Hans", "zh-Hant", "en"})
    assert {row["event_kind"] for row in rows} >= {"knock", "eliminate", "assist", "delayed-elimination"}
    assert all((FIXTURE / row["image"]).is_file() for row in rows)
    assert all(row["source"] and row["event_sec"] >= 0 for row in rows)
