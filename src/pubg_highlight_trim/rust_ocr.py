"""Optional JSONL client for the Rust CPU/ONNX OCR engine."""
from __future__ import annotations

import base64
import json
import os
import subprocess
from typing import Any


class RustOcrBackend:
    def __init__(self, executable: str, model_dir: str, threads: int = 1) -> None:
        self.last_result: dict[str, Any] | None = None
        self._process = subprocess.Popen(
            [executable, model_dir, str(max(1, threads))],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )

    def predict(self, crop: Any) -> list[dict[str, Any]]:
        import numpy as np

        array = np.asarray(crop)
        if array.ndim != 3 or array.shape[2] != 3:
            raise ValueError("Rust OCR expects a three-channel ROI")
        rgb = array[:, :, ::-1].copy()
        request = {
            "pixels_b64": base64.b64encode(rgb.tobytes()).decode("ascii"),
            "width": int(rgb.shape[1]),
            "height": int(rgb.shape[0]),
        }
        assert self._process.stdin is not None and self._process.stdout is not None
        self._process.stdin.write(json.dumps(request) + "\n")
        self._process.stdin.flush()
        if self._process.poll() is not None:
            raise RuntimeError(f"Rust OCR exited with code {self._process.returncode}")
        line = self._process.stdout.readline()
        if not line:
            raise RuntimeError("Rust OCR closed stdout before returning a response")
        try:
            response = json.loads(line)
        except json.JSONDecodeError as exc:
            raise RuntimeError(f"Rust OCR returned invalid JSON: {line[:200]!r}") from exc
        if not response:
            raise RuntimeError("Rust OCR returned an empty response")
        self.last_result = response
        return [{
            "res": {
                "rec_texts": [response.get("text", "")],
                "rec_scores": response.get("scores", []),
                "boxes": response.get("boxes", []),
                "status": response.get("status", "unknown"),
                "method": response.get("method", ""),
                "inference_ms": response.get("inference_ms", 0.0),
                "frame_ms": response.get("frame_ms", 0.0),
            }
        }]

    def close(self) -> None:
        self._process.terminate()
        self._process.wait(timeout=5)


def rust_backend_from_environment() -> RustOcrBackend:
    executable = os.environ.get("PUBG_OCR_ENGINE")
    model_dir = os.environ.get("PUBG_OCR_MODEL_DIR")
    if not executable or not model_dir:
        raise RuntimeError("PUBG_OCR_ENGINE and PUBG_OCR_MODEL_DIR are required for Rust OCR")
    threads = int(os.environ.get("PUBG_OCR_THREADS", "1"))
    return RustOcrBackend(executable, model_dir, threads)
