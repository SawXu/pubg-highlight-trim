from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Literal


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
