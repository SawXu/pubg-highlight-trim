from __future__ import annotations

import argparse
import os
from pathlib import Path

from pubg_highlight_trim.game_languages import AUTO_GAME_LANGUAGE, game_language_choices, get_game_language_profile


def main() -> int:
    parser = argparse.ArgumentParser(description="Download PaddleOCR models into a local PaddleX cache for release packaging.")
    parser.add_argument("--cache-dir", type=Path, default=Path("vendor/paddlex_cache"))
    parser.add_argument("--game-lang", choices=[AUTO_GAME_LANGUAGE, *game_language_choices()], default=AUTO_GAME_LANGUAGE)
    args = parser.parse_args()
    profiles = (
        [get_game_language_profile(code) for code in game_language_choices()]
        if args.game_lang == AUTO_GAME_LANGUAGE
        else [get_game_language_profile(args.game_lang)]
    )

    cache_dir = args.cache_dir.resolve()
    cache_dir.mkdir(parents=True, exist_ok=True)
    os.environ["PADDLE_PDX_CACHE_HOME"] = str(cache_dir)
    os.environ.setdefault("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True")

    from paddleocr import PaddleOCR  # type: ignore

    seen_paddle_langs: set[str] = set()
    for profile in profiles:
        if profile.paddle_lang in seen_paddle_langs:
            continue
        seen_paddle_langs.add(profile.paddle_lang)
        print(f"Prefetching PaddleOCR models for game_lang={profile.code} lang={profile.paddle_lang} into {cache_dir}", flush=True)
        PaddleOCR(
            lang=profile.paddle_lang,
            use_doc_orientation_classify=False,
            use_doc_unwarping=False,
            use_textline_orientation=False,
        )

    official_models = cache_dir / "official_models"
    required = [official_models / "PP-OCRv6_medium_det", official_models / "PP-OCRv6_medium_rec"]
    missing = [str(path) for path in required if not path.exists()]
    if missing:
        raise SystemExit("Missing expected PaddleOCR model directories: " + ", ".join(missing))
    for path in required:
        size_mb = sum(file.stat().st_size for file in path.rglob("*") if file.is_file()) / 1024 / 1024
        print(f"{path.name}: {size_mb:.1f} MB", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
