from __future__ import annotations

from pathlib import Path

from .game_languages import GameLanguageProfile, default_game_language_profile


def iter_source_files(
    folder: Path,
    recursive: bool = False,
    target: str = "self-death",
    language: GameLanguageProfile | None = None,
) -> list[Path]:
    profile = language or default_game_language_profile()
    pattern = "**/*.mp4" if recursive else "*.mp4"
    files: list[Path] = []
    for path in sorted(folder.glob(pattern), key=lambda p: str(p).lower()):
        name = path.name
        if target == "own-kill":
            if profile.own_kill_source_re.search(name):
                files.append(path)
            continue
        if target == "both":
            if (
                profile.own_source_re.search(name)
                or profile.own_kill_source_re.search(name)
                or profile.match_end_source_re.search(name)
            ) and not profile.view_replay_re.search(name):
                files.append(path)
            continue
        if profile.own_source_re.search(name) and not profile.view_replay_re.search(name):
            files.append(path)
    return files
