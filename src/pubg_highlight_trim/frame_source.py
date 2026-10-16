from __future__ import annotations

from pathlib import Path
from typing import Any, Callable, Protocol


class VideoUnreadable(RuntimeError):
    """Container/codec cannot be decoded at all (open or first-frame failure)."""


class FrameSource(Protocol):
    """BGR uint8 ndarray frames at arbitrary timestamps; one per source file."""

    @property
    def is_open(self) -> bool: ...

    def read_at(self, sec: float) -> Any | None:
        """Frame nearest/at sec, BGR uint8 HxWx3, or None on per-frame failure."""

    def close(self) -> None: ...

    def __enter__(self) -> "FrameSource": ...

    def __exit__(self, *exc: Any) -> None: ...


FrameSourceFactory = Callable[[Path], FrameSource]


class Cv2FrameSource:
    """FrameSource backed by cv2_module.VideoCapture; keeps existing seek+read semantics."""

    def __init__(self, path: Path, cv2_module: Any) -> None:
        self._cv2 = cv2_module
        self._cap = cv2_module.VideoCapture(str(path))

    @property
    def is_open(self) -> bool:
        if self._cap is None:
            return False
        is_opened = getattr(self._cap, "isOpened", None)
        return True if is_opened is None else bool(is_opened())

    def read_at(self, sec: float) -> Any | None:
        self._cap.set(self._cv2.CAP_PROP_POS_MSEC, max(0.0, sec) * 1000)
        ok, frame = self._cap.read()
        if not ok or frame is None:
            return None
        return frame

    def close(self) -> None:
        if self._cap is not None:
            self._cap.release()
            self._cap = None

    def __enter__(self) -> "Cv2FrameSource":
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()
