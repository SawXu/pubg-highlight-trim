from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Literal, Pattern


@dataclass(frozen=True)
class ActionTerm:
    canonical: str
    pattern: str


@dataclass(frozen=True)
class LanguageRegexTable:
    code: str
    paddle_lang: str
    own_actor_re: str
    self_subject_re: str
    own_action_terms: tuple[ActionTerm, ...]
    self_action_terms: tuple[ActionTerm, ...]
    fuzzy_self_terms: tuple[ActionTerm, ...]
    zone_self_terms: tuple[str, ...]
    assist_terms: tuple[str, ...]
    delayed_elim_terms: tuple[str, ...]
    kill_count_terms: tuple[str, ...]
    scoreboard_kill_count_terms: tuple[str, ...]
    weapon_marker_terms: tuple[str, ...]
    molotov_terms: tuple[str, ...]
    weapon_suffix_noise_terms: tuple[str, ...]
    own_source_terms: tuple[str, ...]
    own_kill_source_terms: tuple[str, ...]
    match_end_source_terms: tuple[str, ...]
    view_replay_terms: tuple[str, ...]
    source_file_hint: str
    self_subject_keys: frozenset[str]
    own_victim_prefix_re: str = ""
    own_action_gap_re: str = r".{0,24}?"
    own_weapon_position: Literal["before_action", "after_victim"] = "before_action"
    self_weapon_position: Literal["before_action", "after_action"] = "before_action"
    flags: int = 0


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
    molotov_weapon_re: Pattern[str]
    own_source_re: Pattern[str]
    own_kill_source_re: Pattern[str]
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
        molotov_weapon_re=_compile_alt(table.molotov_terms, table.flags),
        own_source_re=_build_source_re(table.own_source_terms),
        own_kill_source_re=_build_source_re(table.own_kill_source_terms),
        match_end_source_re=_build_source_re(table.match_end_source_terms),
        view_replay_re=_compile_alt(table.view_replay_terms, table.flags),
        source_file_hint=table.source_file_hint,
        self_subject_keys=table.self_subject_keys,
        canonical_action_patterns=_compiled_action_patterns(table),
    )


LANGUAGE_REGEX_TABLES = (
    LanguageRegexTable(
        code="zh-Hans",
        paddle_lang="ch",
        own_actor_re=r"你",
        self_subject_re=r"你",
        own_action_terms=(ActionTerm("knock", r"击倒"), ActionTerm("eliminate", r"淘汰")),
        self_action_terms=(ActionTerm("knock", r"击倒了你"), ActionTerm("eliminate", r"淘汰了你")),
        fuzzy_self_terms=(ActionTerm("knock", r"击倒.{0,2}你|倒了你"), ActionTerm("eliminate", r"淘.{0,2}了?你")),
        zone_self_terms=(r"你在安全区外倒地了", r"安全区外倒地了", r"安全区外倒地"),
        assist_terms=(r"协助次数", r"协助", r"助.{0,2}攻"),
        delayed_elim_terms=(r"你终于淘汰了",),
        kill_count_terms=(r"淘汰数",),
        scoreboard_kill_count_terms=(r"淘汰数?\d*", r"淘汰\d+"),
        weapon_marker_terms=(r"使用", r"用"),
        molotov_terms=(r"燃烧弹", r"燃燒彈", r"燃烧瓶", r"燃燒瓶", r"molotov"),
        weapon_suffix_noise_terms=(r"淘汰数?\d*", r"淘汰\d+", r"协助次数", r"协助", r"助.{0,2}攻"),
        own_source_terms=(r"被击倒", r"淘汰"),
        own_kill_source_terms=(r"单次淘汰", r"双次淘汰", r"多杀"),
        match_end_source_terms=(r"比赛结束",),
        view_replay_terms=(r"淘汰画面", r"击倒画面"),
        source_file_hint=".被击倒.DVR*.mp4, .淘汰.DVR*.mp4, .单次淘汰.DVR*.mp4, .双次淘汰.DVR*.mp4, .多杀.DVR*.mp4",
        self_subject_keys=frozenset({"你", "ni"}),
        own_victim_prefix_re=r"了",
    ),
    LanguageRegexTable(
        code="zh-Hant",
        paddle_lang="ch",
        own_actor_re=r"您",
        self_subject_re=r"您",
        own_action_terms=(
            ActionTerm("knock", r"擊倒|击倒"),
            ActionTerm("eliminate", r"擊殺|撃殺|擎殺|淘汰"),
        ),
        self_action_terms=(
            ActionTerm("knock", r"擊倒您|击倒了你"),
            ActionTerm("eliminate", r"擊殺您|撃殺您|擎殺您|淘汰您|淘汰了你"),
        ),
        fuzzy_self_terms=(
            ActionTerm("knock", r"擊倒.{0,2}您|击倒.{0,2}你|倒了你"),
            ActionTerm("eliminate", r"[擊撃擎]?殺.{0,2}您|淘.{0,2}您"),
        ),
        zone_self_terms=(r"您.{0,4}(?:安全區|遊戲區域)外(?:倒地|死亡)", r"您在安全区外倒地了"),
        assist_terms=(r"協助次數", r"協助", r"助.{0,2}攻", r"助.{0,2}殺"),
        delayed_elim_terms=(r"您終於.{0,4}(?:擊殺|撃殺|擎殺|淘汰)",),
        kill_count_terms=(r"擊殺[數数]", r"撃殺[數数]", r"击杀数"),
        scoreboard_kill_count_terms=(r"擊殺[數数]\d*", r"撃殺[數数]\d*", r"擊殺\d+", r"撃殺\d+"),
        weapon_marker_terms=(r"以", r"使用", r"用"),
        molotov_terms=(r"燃燒彈", r"汽油彈", r"molotov"),
        weapon_suffix_noise_terms=(r"(?:\d+\s*)?擊殺[數数]\d*", r"(?:\d+\s*)?撃殺[數数]\d*", r"協助次數", r"協助", r"助.{0,2}攻", r"助.{0,2}殺"),
        own_source_terms=(r"被擊倒", r"死亡", r"淘汰"),
        own_kill_source_terms=(r"單次擊殺", r"雙殺", r"多殺", r"擊倒"),
        match_end_source_terms=(r"對戰結束",),
        view_replay_terms=(r"死亡畫面", r"淘汰畫面", r"擊倒畫面"),
        source_file_hint=".被擊倒.DVR*.mp4, .死亡.DVR*.mp4, .淘汰.DVR*.mp4, .單次擊殺.DVR*.mp4, .雙殺.DVR*.mp4, .擊倒.DVR*.mp4",
        self_subject_keys=frozenset({"您", "你", "ni"}),
    ),
    LanguageRegexTable(
        code="en",
        paddle_lang="en",
        own_actor_re=r"YOU(?:FINALLY)?",
        self_subject_re=r"YOU",
        own_action_terms=(
            ActionTerm("knock", r"KNOCKEDOUT"),
            ActionTerm("eliminate", r"KILLED|ELIMINATED"),
        ),
        self_action_terms=(
            ActionTerm("knock", r"KNOCKEDYOU(?:OUT)?"),
            ActionTerm("eliminate", r"KILLEDYOU|KILLSYOU|ELIMINATEDYOU"),
        ),
        fuzzy_self_terms=(
            ActionTerm("knock", r"KNOCKED.{0,8}YOU"),
            ActionTerm("eliminate", r"KILL(?:ED|S).{0,4}YOU|ELIMINAT(?:ED)?.{0,4}YOU"),
        ),
        zone_self_terms=(
            r"OUTSIDETHEPLAYZONE",
            r"OUTSIDETHEBLUEZONE",
            r"YOU(?:WERE)?(?:KNOCKEDOUT|KILLED|DIED).{0,18}(?:PLAYZONE|BLUEZONE|ZONE)",
        ),
        assist_terms=(r"(?:\d+)?ASSISTS?",),
        delayed_elim_terms=(r"YOUFINALLY(?:KILLED|ELIMINATED)",),
        kill_count_terms=(r"(?:\d+)?KILLS?\b",),
        scoreboard_kill_count_terms=(r"\d+KILLS?",),
        weapon_marker_terms=(r"WITH", r"USING"),
        molotov_terms=(r"molotov", r"fire\s*-?\s*bomb", r"gas\s*can", r"c4"),
        weapon_suffix_noise_terms=(r"\d(?:KILLS?|ASSISTS?)", r"ASSISTS?"),
        own_source_terms=(r"Knockouted", r"Death"),
        own_kill_source_terms=(r"Knockout", r"Single kill", r"Double kill", r"Triple kill", r"Multi kill", r"Multiple kill"),
        match_end_source_terms=(r"Match end", r"Match ended", r"Match complete", r"Game over"),
        view_replay_terms=(r"Death\s*cam", r"Kill\s*cam", r"Replay"),
        source_file_hint=".Knockouted.DVR*.mp4, .Death.DVR*.mp4, .Knockout.DVR*.mp4, .Single kill.DVR*.mp4, .Double kill.DVR*.mp4",
        self_subject_keys=frozenset({"you"}),
        own_action_gap_re="",
        own_weapon_position="after_victim",
        self_weapon_position="after_action",
        flags=re.IGNORECASE,
    ),
)


GAME_LANGUAGE_PROFILES = {table.code: _build_profile(table) for table in LANGUAGE_REGEX_TABLES}


def game_language_choices() -> list[str]:
    return sorted(GAME_LANGUAGE_PROFILES)


def default_game_language_profile() -> GameLanguageProfile:
    return GAME_LANGUAGE_PROFILES[DEFAULT_GAME_LANGUAGE]


def get_game_language_profile(code: str | None) -> GameLanguageProfile:
    key = code or DEFAULT_GAME_LANGUAGE
    try:
        return GAME_LANGUAGE_PROFILES[key]
    except KeyError as exc:
        choices = ", ".join(game_language_choices())
        raise ValueError(f"Unsupported PUBG game language: {key}. Supported: {choices}") from exc
