"""Ad-hoc latency benchmark for per-read_at frame grabs (ticket #52).

Measures ffmpeg single-frame grab latency on an AV1 source vs its H.264 twin
under several decode strategies (software, -hwaccel X, -c:v <dec>).

Usage:
    PYTHONPATH=src PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR=<dir> python scripts/bench_frame_read.py
"""

from __future__ import annotations

import argparse
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from pubg_highlight_trim.ffmpeg_tools import find_binary


def corpus_pair(corpus_root: Path) -> tuple[Path, Path]:
    root = corpus_root
    if (root / "av1_h264_staging").is_dir():
        staging = root / "av1_h264_staging"
    else:
        root = next(p for p in root.iterdir() if p.is_dir() and p.name.startswith("PLAYERUNKNOWN"))
        staging = root / "av1_h264_staging"
    ffprobe = find_binary("ffprobe.exe")

    def codec(p: Path) -> str:
        return subprocess.check_output(
            [ffprobe, "-v", "error", "-select_streams", "v:0",
             "-show_entries", "stream=codec_name", "-of", "csv=p=0", str(p)],
            text=True,
        ).strip().split(",")[0]

    for top in sorted(root.glob("*.mp4")):
        twin = staging / top.name
        if codec(top) == "av1" and twin.exists() and codec(twin) == "h264":
            return top, twin
    raise SystemExit("no AV1/H264 pair found")


# Representative read_at schedule: monotonic coarse scan (~4 s steps over a
# ~58 s clip) then a refine cluster that walks backward then bisects forward,
# then forward assist reads.
def schedule(duration: float) -> list[float]:
    times = [round(t, 1) for t in range(4, int(duration) - 2, 4)]
    anchor = times[len(times) // 2]
    refine = [anchor - 2, anchor - 4, anchor - 6, anchor - 4.5, anchor - 5, anchor - 3.5, anchor + 0.0]
    assist = [anchor + 1.0, anchor + 1.5, anchor + 2.0]
    return times + refine + assist


def build_cmd(ffmpeg: str, path: Path, sec: float, extra: list[str]) -> list[str]:
    return [
        ffmpeg, "-hide_banner", "-loglevel", "error",
        *extra,
        "-ss", f"{sec:.3f}", "-i", str(path),
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgr24", "-",
    ]


def run_variant(name: str, ffmpeg: str, path: Path, times: list[float], extra: list[str], expect_bytes: int) -> dict:
    lat = []
    bad = 0
    ref_hash = None
    for t in times:
        started = time.perf_counter()
        proc = subprocess.run(
            build_cmd(ffmpeg, path, t, extra),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        lat.append((time.perf_counter() - started) * 1000)
        if proc.returncode != 0 or len(proc.stdout) != expect_bytes:
            bad += 1
            if bad == 1:
                print(f"    ! fail rc={proc.returncode} bytes={len(proc.stdout)} stderr={proc.stderr[-200:]!r}")
        elif ref_hash is None and t == times[0]:
            ref_hash = hash(proc.stdout[:4096])
    return {
        "name": name,
        "n": len(times),
        "bad": bad,
        "total_ms": sum(lat),
        "mean_ms": statistics.mean(lat),
        "median_ms": statistics.median(lat),
        "p90_ms": sorted(lat)[int(len(lat) * 0.9)],
        "frame_hash": ref_hash,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repeat", type=int, default=2)
    parser.add_argument(
        "--corpus",
        type=Path,
        default=Path("F:/NVIDIA/TEMP/Highlights"),
        help="Highlight corpus dir (or its parent) containing av1_h264_staging/ A/B pairs.",
    )
    args = parser.parse_args()

    ffmpeg = find_binary("ffmpeg.exe")
    ffprobe = find_binary("ffprobe.exe")
    av1, h264 = corpus_pair(args.corpus)
    print(f"AV1 : {av1.name}")
    print(f"H264: {h264.name}")

    dims = subprocess.check_output(
        [ffprobe, "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream=width,height", "-of", "csv=p=0", str(av1)],
        text=True,
    ).strip().split(",")
    expect_bytes = int(dims[0]) * int(dims[1]) * 3
    dur = float(subprocess.check_output(
        [ffprobe, "-v", "error", "-show_entries", "format=duration",
         "-of", "default=nw=1:nk=1", str(av1)], text=True))
    times = schedule(dur)
    print(f"schedule: {len(times)} reads over {dur:.1f}s clip, frame={dims[0]}x{dims[1]}")

    variants = [
        ("sw", []),
        ("hwaccel-cuda", ["-hwaccel", "cuda"]),
        ("hwaccel-d3d11va", ["-hwaccel", "d3d11va"]),
        ("hwaccel-d3d12va", ["-hwaccel", "d3d12va"]),
        ("av1_cuvid", ["-c:v", "av1_cuvid"]),
        ("h264_cuvid", ["-c:v", "h264_cuvid"]),
    ]

    for label, path in (("AV1", av1), ("H264", h264)):
        print(f"\n=== {label} ===")
        for name, extra in variants:
            if label == "AV1" and name == "h264_cuvid":
                continue
            if label == "H264" and name == "av1_cuvid":
                continue
            # one warmup
            subprocess.run(build_cmd(ffmpeg, path, times[0], extra), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            best = None
            for _ in range(args.repeat):
                r = run_variant(name, ffmpeg, path, times, extra, expect_bytes)
                if best is None or r["total_ms"] < best["total_ms"]:
                    best = r
            r = best
            print(f"  {name:18s} n={r['n']:2d} bad={r['bad']} mean={r['mean_ms']:7.1f}ms "
                  f"median={r['median_ms']:7.1f}ms p90={r['p90_ms']:7.1f}ms total={r['total_ms']/1000:6.2f}s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
