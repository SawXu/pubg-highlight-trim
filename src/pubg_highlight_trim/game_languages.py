from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Pattern

from .game_language_tables import ActionTerm, LANGUAGE_REGEX_TABLES, LanguageRegexTable


@dataclass(frozen=True)
class GameLanguageProfile:
    code: str
    paddle_lang: str
    regex_table: LanguageRegexTable
    self_strict_re: Pattern[str]
    self_zone_downed_re: Pattern[str]
    self_fuzzy_re: Pattern[str]
    own_kill_strict_re: Pattern[str]
    own_kill_event_re: Pattern[str]
    own_kill_assist_re: Pattern[str]
    own_kill_delayed_elim_re: Pattern[str]
    own_kill_victim_stop_re: Pattern[str]
    own_kill_scoreboard_assist_re: Pattern[str]
    own_kill_count_re: Pattern[str]
    weapon_prefix_re: Pattern[str]
    self_weapon_re: Pattern[str]
    weapon_suffix_noise_re: Pattern[str]
    subject_suffix_noise_re: Pattern[str]
    molotov_weapon_re: Pattern[str]
    own_source_re: Pattern[str]
    own_kill_source_re: Pattern[str]
    multi_kill_source_re: Pattern[str]
    match_end_source_re: Pattern[str]
    view_replay_re: Pattern[str]
    source_file_hint: str
    self_subject_keys: frozenset[str]
    canonical_action_patterns: tuple[tuple[str, Pattern[str]], ...]

    def canonical_action(self, text: str) -> str:
        for action, pattern in self.canonical_action_patterns:
            if pattern.search(text or ""):
                return action
        return text


AUTO_GAME_LANGUAGE = "auto"
DEFAULT_GAME_LANGUAGE = "zh-Hans"
_EVENT_PREFIX_PUNCT_RE = r"[，。,.、:：]"
_DISTANCE_RE = r"\(\d+\s*[mM]\)|（\d+\s*[mM]）"
_DVR_MP4_SUFFIX_RE = r"\.DVR(?:_\d+)?\.mp4$"


def _alt(parts: tuple[str, ...] | list[str]) -> str:
    return "(?:" + "|".join(parts) + ")"


def _compile_alt(parts: tuple[str, ...] | list[str], flags: int = 0) -> Pattern[str]:
    return re.compile(_alt(parts), flags)


def _action_alt(terms: tuple[ActionTerm, ...]) -> str:
    return _alt([term.pattern for term in terms])


def _compiled_action_patterns(table: LanguageRegexTable) -> tuple[tuple[str, Pattern[str]], ...]:
    terms = table.own_action_terms + table.self_action_terms + table.fuzzy_self_terms
    return tuple((term.canonical, re.compile(term.pattern, table.flags)) for term in terms)


def _own_next_event_re(table: LanguageRegexTable) -> str:
    action = _action_alt(table.own_action_terms)
    return fr"{table.own_actor_re}{table.own_action_gap_re}(?:{action}){table.own_victim_prefix_re}"


def _build_own_kill_event_re(table: LanguageRegexTable) -> Pattern[str]:
    action = _action_alt(table.own_action_terms)
    next_event = _own_next_event_re(table)
    if table.own_weapon_position == "after_victim":
        weapon_marker = _alt(table.weapon_marker_terms)
        assist = _alt(table.assist_terms)
        pattern = (
            fr"{table.own_actor_re}{table.own_action_gap_re}(?P<action>{action})"
            fr"{table.own_victim_prefix_re}(?P<victim>.+?)"
            fr"(?:{weapon_marker}(?P<weapon>.+?))?"
            fr"(?={next_event}|{assist}|$)"
        )
    else:
        pattern = (
            fr"{table.own_actor_re}{table.own_action_gap_re}(?P<action>{action})"
            fr"{table.own_victim_prefix_re}(?P<victim>.+?)(?={next_event}|$)"
        )
    return re.compile(pattern, table.flags)


def _build_own_kill_strict_re(table: LanguageRegexTable) -> Pattern[str]:
    action = _action_alt(table.own_action_terms)
    pattern = (
        fr"(?:^|{_EVENT_PREFIX_PUNCT_RE})?{table.own_actor_re}{table.own_action_gap_re}"
        fr"(?:{action}){table.own_victim_prefix_re}(?!{table.self_subject_re})"
    )
    return re.compile(pattern, table.flags)


def _build_self_strict_re(table: LanguageRegexTable) -> Pattern[str]:
    action = _action_alt(table.self_action_terms)
    pattern = fr"(?P<action>{action})"
    if table.self_weapon_position == "after_action":
        weapon_marker = _alt(table.weapon_marker_terms)
        own_action = _action_alt(table.own_action_terms)
        kill_count = _alt(table.kill_count_terms)
        assist = _alt(table.assist_terms)
        pattern += (
            fr"(?:{weapon_marker}(?P<weapon>.+?))?"
            fr"(?={table.own_actor_re}{table.own_action_gap_re}(?:{own_action})|{kill_count}|{assist}|$)"
        )
    return re.compile(pattern, table.flags)


def _build_victim_stop_re(table: LanguageRegexTable) -> Pattern[str]:
    action = _action_alt(table.own_action_terms)
    weapon_marker = _alt(table.weapon_marker_terms)
    scoreboard_kill_count = _alt(table.scoreboard_kill_count_terms or table.kill_count_terms)
    stops = [
        _DISTANCE_RE,
        fr"(?:\d+\s*)?(?:{scoreboard_kill_count})",
        *table.kill_count_terms,
        *table.scoreboard_kill_count_terms,
        *table.assist_terms,
        *table.weapon_marker_terms,
        fr"{table.own_actor_re}(?:{weapon_marker})",
        fr"{table.own_actor_re}{table.own_action_gap_re}(?:{action}){table.own_victim_prefix_re}",
        *(term.pattern for term in table.self_action_terms),
    ]
    return _compile_alt(stops, table.flags)


def _build_scoreboard_assist_re(table: LanguageRegexTable) -> Pattern[str]:
    kill_count = _alt(table.scoreboard_kill_count_terms or table.kill_count_terms)
    assist = _alt(table.assist_terms)
    return re.compile(fr"{kill_count}\d*{assist}", table.flags)


def _build_weapon_suffix_noise_re(table: LanguageRegexTable) -> Pattern[str]:
    return re.compile(fr"(?:{_alt(table.weapon_suffix_noise_terms)})+$", table.flags)


def _build_subject_suffix_noise_re(table: LanguageRegexTable) -> Pattern[str]:
    if not table.subject_suffix_noise_terms:
        return re.compile(r"(?!x)x")
    return re.compile(fr"(?:{_alt(table.subject_suffix_noise_terms)})+$", table.flags)


def _build_source_re(names: tuple[str, ...]) -> Pattern[str]:
    return re.compile(fr"\.(?:{'|'.join(names)}){_DVR_MP4_SUFFIX_RE}", re.IGNORECASE)


def _build_profile(table: LanguageRegexTable) -> GameLanguageProfile:
    weapon_marker = _alt(table.weapon_marker_terms)
    return GameLanguageProfile(
        code=table.code,
        paddle_lang=table.paddle_lang,
        regex_table=table,
        self_strict_re=_build_self_strict_re(table),
        self_zone_downed_re=_compile_alt(table.zone_self_terms, table.flags),
        self_fuzzy_re=re.compile(fr"(?P<action>{_action_alt(table.fuzzy_self_terms)})", table.flags),
        own_kill_strict_re=_build_own_kill_strict_re(table),
        own_kill_event_re=_build_own_kill_event_re(table),
        own_kill_assist_re=_compile_alt(table.assist_terms, table.flags),
        own_kill_delayed_elim_re=_compile_alt(table.delayed_elim_terms, table.flags),
        own_kill_victim_stop_re=_build_victim_stop_re(table),
        own_kill_scoreboard_assist_re=_build_scoreboard_assist_re(table),
        own_kill_count_re=_compile_alt(table.kill_count_terms, table.flags),
        weapon_prefix_re=re.compile(fr"(?:{table.self_subject_re})?(?:{weapon_marker})(?P<weapon>.+)$", table.flags),
        self_weapon_re=re.compile(fr"(?:{weapon_marker})(?P<weapon>.+)$", table.flags),
        weapon_suffix_noise_re=_build_weapon_suffix_noise_re(table),
        subject_suffix_noise_re=_build_subject_suffix_noise_re(table),
        molotov_weapon_re=_compile_alt(table.molotov_terms, table.flags),
        own_source_re=_build_source_re(table.own_source_terms),
        own_kill_source_re=_build_source_re(table.own_kill_source_terms),
        multi_kill_source_re=_build_source_re(table.multi_kill_source_terms),
        match_end_source_re=_build_source_re(table.match_end_source_terms),
        view_replay_re=_compile_alt(table.view_replay_terms, table.flags),
        source_file_hint=table.source_file_hint,
        self_subject_keys=table.self_subject_keys,
        canonical_action_patterns=_compiled_action_patterns(table),
    )


GAME_LANGUAGE_PROFILES = {table.code: _build_profile(table) for table in LANGUAGE_REGEX_TABLES}


def game_language_choices() -> list[str]:
    return sorted(GAME_LANGUAGE_PROFILES)


def game_language_cli_choices() -> list[str]:
    return [AUTO_GAME_LANGUAGE, *game_language_choices()]


def default_game_language_profile() -> GameLanguageProfile:
    return GAME_LANGUAGE_PROFILES[DEFAULT_GAME_LANGUAGE]


def get_game_language_profile(code: str | None) -> GameLanguageProfile:
    key = code or DEFAULT_GAME_LANGUAGE
    try:
        return GAME_LANGUAGE_PROFILES[key]
    except KeyError as exc:
        choices = ", ".join(game_language_choices())
        raise ValueError(f"Unsupported PUBG game language: {key}. Supported: {choices}") from exc
