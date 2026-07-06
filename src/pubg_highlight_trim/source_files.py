from __future__ import annotations

import re
from collections import Counter
from pathlib import Path

from .game_languages import GAME_LANGUAGE_PROFILES, GameLanguageProfile, default_game_language_profile


_PROFILE_ORDER = {code: index for index, code in enumerate(GAME_LANGUAGE_PROFILES)}


def _iter_mp4_files(folder: Path, recursive: bool) -> list[Path]:
    pattern = "**/*.mp4" if recursive else "*.mp4"
    return sorted(folder.glob(pattern), key=lambda p: str(p).lower())


def _source_file_matches_profile(name: str, target: str, profile: GameLanguageProfile) -> bool:
    if target == "own-kill":
        return bool(profile.own_kill_source_re.search(name))
    if target == "both":
        return bool(
            (
                profile.own_source_re.search(name)
                or profile.own_kill_source_re.search(name)
                or profile.match_end_source_re.search(name)
            )
            and not profile.view_replay_re.search(name)
        )
    return bool(profile.own_source_re.search(name) and not profile.view_replay_re.search(name))


def _source_terms(profile: GameLanguageProfile, target: str) -> tuple[str, ...]:
    table = profile.regex_table
    if target == "own-kill":
        return table.own_kill_source_terms
    if target == "both":
        return table.own_source_terms + table.own_kill_source_terms + table.match_end_source_terms
    return table.own_source_terms


def _source_term_score(name: str, target: str, profile: GameLanguageProfile) -> int:
    flags = profile.regex_table.flags | re.IGNORECASE
    score = 0
    for term in _source_terms(profile, target):
        match = re.search(term, name, flags)
        if match:
            score = max(score, len(match.group(0)))
    return score


def _candidate_language_scores(path: Path, target: str) -> list[tuple[GameLanguageProfile, int]]:
    candidates: list[tuple[GameLanguageProfile, int]] = []
    for profile in GAME_LANGUAGE_PROFILES.values():
        if not _source_file_matches_profile(path.name, target, profile):
            continue
        candidates.append((profile, max(1, _source_term_score(path.name, target, profile))))
    return sorted(candidates, key=lambda item: (-item[1], _PROFILE_ORDER[item[0].code]))


def infer_source_file_languages(paths: list[Path], target: str = "self-death") -> dict[Path, GameLanguageProfile]:
    selected: dict[Path, GameLanguageProfile] = {}
    ambiguous: dict[Path, list[tuple[GameLanguageProfile, int]]] = {}
    language_counts: Counter[str] = Counter()
    language_scores: Counter[str] = Counter()

    for path in paths:
        candidates = _candidate_language_scores(path, target)
        if not candidates:
            continue
        top_score = candidates[0][1]
        top_candidates = [candidate for candidate in candidates if candidate[1] == top_score]
        if len(top_candidates) == 1:
            profile = top_candidates[0][0]
            selected[path] = profile
            language_counts[profile.code] += 1
            language_scores[profile.code] += top_score
        else:
            ambiguous[path] = top_candidates

    dominant_codes = [
        code
        for code, _ in sorted(
            language_counts.items(),
            key=lambda item: (-item[1], -language_scores[item[0]], _PROFILE_ORDER[item[0]]),
        )
    ]

    for path, candidates in ambiguous.items():
        candidate_codes = {profile.code for profile, _ in candidates}
        chosen_code = next((code for code in dominant_codes if code in candidate_codes), None)
        if chosen_code is None:
            chosen = min((profile for profile, _ in candidates), key=lambda profile: _PROFILE_ORDER[profile.code])
        else:
            chosen = next(profile for profile, _ in candidates if profile.code == chosen_code)
        selected[path] = chosen

    return {path: selected[path] for path in paths if path in selected}


def iter_source_file_languages(
    folder: Path,
    recursive: bool = False,
    target: str = "self-death",
) -> list[tuple[Path, GameLanguageProfile]]:
    matches = infer_source_file_languages(_iter_mp4_files(folder, recursive), target)
    return list(matches.items())


def iter_source_files(
    folder: Path,
    recursive: bool = False,
    target: str = "self-death",
    language: GameLanguageProfile | None = None,
) -> list[Path]:
    profile = language or default_game_language_profile()
    files: list[Path] = []
    for path in _iter_mp4_files(folder, recursive):
        if _source_file_matches_profile(path.name, target, profile):
            files.append(path)
    return files
