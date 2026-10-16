from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Any, Callable, Protocol

from .ffmpeg_tools import find_binary


class VideoUnreadable(RuntimeError):
    """Container/codec cannot be decoded at all (open or first-frame failure)."""


class FrameSource(Protocol):
    """BGR uint8 ndarray frames at arbitrary timestamps; one per source file."""

    @property
    def is_open(self) -> bool: ...

    def read_at(self, sec: float) -> Any | None:
        """Frame nearest/at sec, BGR uint8 HxWx3, or None on per-frame failure."""

    def close(self) -> None:
        """Release the source; must be exception-safe (may run while an error propagates)."""

    def __enter__(self) -> "FrameSource": ...

    def __exit__(self, *exc: Any) -> None: ...


FrameSourceFactory = Callable[[Path], FrameSource]


class Cv2FrameSource:
    """FrameSource backed by cv2_module.VideoCapture; keeps existing seek+read semantics.

    Opening is eager: construction raises VideoUnreadable when the capture
    cannot be opened at all (cv2's silent ``isOpened() == False`` included).
    Captures without ``isOpened()`` (test fakes) are treated as open, and
    per-frame read failures still surface as ``None`` from ``read_at`` rather
    than raising — only a real open failure raises here.
    """

    def __init__(self, path: Path, cv2_module: Any) -> None:
        self._cv2 = cv2_module
        self._cap = cv2_module.VideoCapture(str(path))
        if not self.is_open:
            self.close()
            raise VideoUnreadable(f"cv2 could not open video source: {path}")

    @property
    def is_open(self) -> bool:
        if self._cap is None:
            return False
        is_opened = getattr(self._cap, "isOpened", None)
        return True if is_opened is None else bool(is_opened())

    def read_at(self, sec: float) -> Any | None:
        if self._cap is None:
            return None
        self._cap.set(self._cv2.CAP_PROP_POS_MSEC, max(0.0, sec) * 1000)
        ok, frame = self._cap.read()
        if not ok or frame is None:
            return None
        return frame

    def close(self) -> None:
        cap, self._cap = self._cap, None
        if cap is not None:
            try:
                cap.release()
            except Exception:
                pass

    def __enter__(self) -> "Cv2FrameSource":
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()


class FfmpegFrameSource:
    """FrameSource decoding one BGR frame per read via ffmpeg rawvideo/bgr24.

    ``hwaccel`` names an ffmpeg ``-hwaccel`` method (e.g. ``"cuda"``,
    ``"d3d11va"``). It is opt-in: measured on the seek-grab OCR pattern,
    per-spawn hardware decode costs more than it saves versus libdav1d
    software decode, so production wiring leaves it off. When it is set and
    the hardware path fails to decode, the source transparently retries once
    in software and disables hwaccel for the rest of its lifetime — a missing
    or incompatible GPU can never turn a decodable file into a read failure.
    """

    _STDERR_TAIL = 400

    def __init__(self, path: Path, ffmpeg: str, ffprobe: str | None = None, hwaccel: str | None = None) -> None:
        self._path = Path(path)
        self._ffmpeg = str(ffmpeg)
        self._ffprobe = str(ffprobe) if ffprobe else find_binary("ffprobe.exe")
        self._hwaccel = hwaccel
        self._closed = False
        self._decoded_any = False
        self._stderr_tail = ""
        self._width, self._height = self._probe_dimensions()

    @property
    def is_open(self) -> bool:
        return not self._closed

    @property
    def width(self) -> int:
        return self._width

    @property
    def height(self) -> int:
        return self._height

    def read_at(self, sec: float) -> Any | None:
        frame = self._decode(sec)
        if frame is not None:
            self._decoded_any = True
            return frame
        if self._decoded_any or self._decode(0.0) is not None:
            self._decoded_any = True
            return None
        tail = f": {self._stderr_tail}" if self._stderr_tail else ""
        raise VideoUnreadable(f"ffmpeg cannot decode any frame from {self._path}{tail}")

    def _probe_dimensions(self) -> tuple[int, int]:
        try:
            out = subprocess.check_output(
                [
                    self._ffprobe,
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height",
                    "-of",
                    "csv=p=0",
                    str(self._path),
                ],
                text=True,
                stderr=subprocess.PIPE,
            ).strip()
            width, height = (int(part) for part in out.splitlines()[0].split(",")[:2])
        except Exception as exc:
            raise VideoUnreadable(f"ffprobe cannot read {self._path}: {exc}") from exc
        if width <= 0 or height <= 0:
            raise VideoUnreadable(f"ffprobe reported invalid dimensions {width}x{height} for {self._path}")
        return width, height

    def _decode(self, sec: float) -> Any | None:
        frame = self._grab(sec, self._hwaccel)
        if frame is None and self._hwaccel is not None:
            # First hardware-path failure (no such GPU/driver, unsupported
            # codec, broken hwdownload — or simply an out-of-range seek):
            # disable hwaccel permanently and retry in software. Correctness
            # is unaffected either way; a spurious disable only costs the
            # opt-in speedup for the rest of this source.
            self._hwaccel = None
            frame = self._grab(sec, None)
        return frame

    def _grab(self, sec: float, hwaccel: str | None) -> Any | None:
        import numpy as np

        cmd = [self._ffmpeg, "-hide_banner", "-loglevel", "error"]
        if hwaccel:
            cmd += ["-hwaccel", hwaccel]
        cmd += [
            "-ss",
            f"{max(0.0, sec):.3f}",
            "-i",
            str(self._path),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgr24",
            "-",
        ]
        try:
            proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except OSError as exc:
            raise VideoUnreadable(f"cannot run {self._ffmpeg}: {exc}") from exc
        self._stderr_tail = proc.stderr[-self._STDERR_TAIL :].decode("utf-8", "replace").strip()
        if proc.returncode != 0 or len(proc.stdout) != self._height * self._width * 3:
            return None
        return np.frombuffer(proc.stdout, dtype=np.uint8).reshape(self._height, self._width, 3)

    def close(self) -> None:
        self._closed = True

    def __enter__(self) -> "FfmpegFrameSource":
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()
