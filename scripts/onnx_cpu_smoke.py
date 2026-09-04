"""Run a deterministic CPU ONNX smoke test against the pinned model pair."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np
import onnxruntime as ort


def run(path: Path, shape: list[int]) -> dict:
    session = ort.InferenceSession(str(path), providers=["CPUExecutionProvider"])
    input_name = session.get_inputs()[0].name
    outputs = session.run(None, {input_name: np.zeros(shape, dtype=np.float32)})
    return {
        "input": input_name,
        "input_shape": shape,
        "output_names": [output.name for output in session.get_outputs()],
        "output_shapes": [list(output.shape) for output in outputs],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", type=Path, required=True)
    args = parser.parse_args()
    report = {
        "detector": run(args.model_dir / "det.onnx", [1, 3, 960, 960]),
        "recognizer": run(args.model_dir / "rec.onnx", [1, 3, 48, 320]),
    }
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
