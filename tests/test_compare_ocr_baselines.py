import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).parents[1]
SCRIPT = ROOT / "scripts" / "compare_ocr_baselines.py"


def write_rows(path: Path, rows: list[dict]) -> None:
    path.write_text("\n".join(json.dumps(row, ensure_ascii=False) for row in rows), encoding="utf-8")


def test_compare_normalizes_text_and_reports_rates(tmp_path):
    left, right = tmp_path / "python.jsonl", tmp_path / "rust.jsonl"
    rows = [{"frame_id": 1, "text": "YOU KNOCKED OUT", "action": "knock", "target": "own-kill", "boxes": [[1, 2]], "inference_ms": 10.0}]
    write_rows(left, rows)
    write_rows(right, [{**rows[0], "text": "YOU   KNOCKED OUT"}])
    result = subprocess.run([sys.executable, str(SCRIPT), str(left), str(right)], capture_output=True, text=True, check=False)
    assert result.returncode == 0
    report = json.loads(result.stdout)
    assert report["text_match_rate"] == 1.0
    assert report["action_match_rate"] == 1.0
    assert report["box_order_match_rate"] == 1.0
    assert report["mean_inference_delta_ms"] == 0.0


def test_compare_fails_when_a_frame_is_missing(tmp_path):
    left, right = tmp_path / "python.jsonl", tmp_path / "rust.jsonl"
    write_rows(left, [{"frame_id": 1, "text": "x"}])
    write_rows(right, [])
    result = subprocess.run([sys.executable, str(SCRIPT), str(left), str(right)], capture_output=True, text=True, check=False)
    assert result.returncode == 1
    assert json.loads(result.stdout)["missing_from_rust"] == ["1"]


def test_compare_fails_on_semantic_mismatch(tmp_path):
    left, right = tmp_path / "python.jsonl", tmp_path / "rust.jsonl"
    write_rows(left, [{"frame_id": 1, "text": "x", "action": "knock", "target": "enemy", "boxes": [[1, 2]]}])
    write_rows(right, [{"frame_id": 1, "text": "x", "action": "miss", "target": "enemy", "boxes": [[1, 2]]}])
    result = subprocess.run([sys.executable, str(SCRIPT), str(left), str(right)], capture_output=True, text=True, check=False)
    assert result.returncode == 1
