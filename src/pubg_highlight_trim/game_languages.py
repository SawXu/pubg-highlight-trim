from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Pattern


@dataclass(frozen=True)
class GameLanguageProfile:
    code: str
    paddle_lang: str
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


ZH_HANS_PROFILE = GameLanguageProfile(
    code="zh-Hans",
    paddle_lang="ch",
    self_strict_re=re.compile(r"(击倒了你|淘汰了你)"),
    self_zone_downed_re=re.compile(r"(你在安全区外倒地了|安全区外倒地了|安全区外倒地)"),
    self_fuzzy_re=re.compile(r"(击倒.{0,2}你|淘.{0,2}了?你|倒了你)"),
    own_kill_strict_re=re.compile(r"(?:^|[，。:：])?你.{0,24}(?:击倒|淘汰)了(?!你)"),
    own_kill_event_re=re.compile(r"你.{0,24}?(?P<action>击倒|淘汰)了(?P<victim>.+?)(?=你.{0,24}?(?:击倒|淘汰)了|$)"),
    own_kill_assist_re=re.compile(r"(协助次数|协助|助.{0,2}攻)"),
    own_kill_delayed_elim_re=re.compile(r"你终于淘汰了"),
    own_kill_victim_stop_re=re.compile(r"(淘汰数?\d*协助次数|淘汰\d+协助次数|淘汰数|协助次数|协助|助.{0,2}攻|你用|击倒了你|淘汰了你)"),
    own_kill_scoreboard_assist_re=re.compile(r"淘汰数?\d*协助次数|淘汰\d+协助次数"),
    own_kill_count_re=re.compile(r"淘汰数"),
    weapon_prefix_re=re.compile(r"(?:你)?(?:使用|用)(?P<weapon>.+)$"),
    self_weapon_re=re.compile(r"(?:使用|用)(?P<weapon>[^用]{1,32})$"),
    molotov_weapon_re=re.compile(r"(燃烧弹|燃燒彈|燃烧瓶|燃燒瓶|molotov)", re.IGNORECASE),
    own_source_re=re.compile(r"\.(?:被击倒|淘汰)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    own_kill_source_re=re.compile(r"\.(?:单次淘汰|双次淘汰|多杀)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    match_end_source_re=re.compile(r"\.比赛结束\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    view_replay_re=re.compile(r"(?:淘汰画面|击倒画面)"),
    source_file_hint=".被击倒.DVR*.mp4, .淘汰.DVR*.mp4, .单次淘汰.DVR*.mp4, .双次淘汰.DVR*.mp4, .多杀.DVR*.mp4",
    self_subject_keys=frozenset({"你", "ni"}),
    canonical_action_patterns=(
        ("eliminate", re.compile(r"淘")),
        ("knock", re.compile(r"击倒|倒了你")),
    ),
)


ZH_HANT_PROFILE = GameLanguageProfile(
    code="zh-Hant",
    paddle_lang="ch",
    self_strict_re=re.compile(r"(擊倒您|擊殺您|撃殺您|擎殺您|淘汰您|击倒了你|淘汰了你)"),
    self_zone_downed_re=re.compile(r"(您.{0,4}(?:安全區|遊戲區域)外(?:倒地|死亡)|您在安全区外倒地了)"),
    self_fuzzy_re=re.compile(r"(擊倒.{0,2}您|击倒.{0,2}你|[擊撃擎]?殺.{0,2}您|淘.{0,2}您|倒了你)"),
    own_kill_strict_re=re.compile(r"(?:^|[，。:：])?您.{0,24}(?:擊倒|击倒|擊殺|撃殺|擎殺|淘汰)(?!您)"),
    own_kill_event_re=re.compile(
        r"您.{0,24}?(?P<action>擊倒|击倒|擊殺|撃殺|擎殺|淘汰)(?P<victim>.+?)"
        r"(?=您.{0,24}?(?:擊倒|击倒|擊殺|撃殺|擎殺|淘汰)|$)"
    ),
    own_kill_assist_re=re.compile(r"(協助次數|協助|助.{0,2}攻|助.{0,2}殺)"),
    own_kill_delayed_elim_re=re.compile(r"您終於.{0,4}(?:擊殺|撃殺|擎殺|淘汰)"),
    own_kill_victim_stop_re=re.compile(
        r"(\(\d+\s*[mM]\)|（\d+\s*[mM]）|(?:\d+\s*)?(?:擊殺[數数]|撃殺[數数]|击杀数|擊殺|撃殺|击杀)|"
        r"協助次數|協助|助.{0,2}攻|助.{0,2}殺|"
        r"您以|擊倒您|擊殺您|撃殺您|擎殺您|淘汰您)"
    ),
    own_kill_scoreboard_assist_re=re.compile(r"(?:擊殺[數数]|撃殺[數数]|擊殺\d*協助次數|撃殺\d*協助次數)"),
    own_kill_count_re=re.compile(r"(擊殺[數数]|撃殺[數数])"),
    weapon_prefix_re=re.compile(r"(?:您)?(?:以|使用|用)(?P<weapon>.+)$"),
    self_weapon_re=re.compile(r"(?:以|使用|用)(?P<weapon>[^以用]{1,32})$"),
    molotov_weapon_re=re.compile(r"(燃燒彈|汽油彈|molotov)", re.IGNORECASE),
    own_source_re=re.compile(r"\.(?:被擊倒|死亡|淘汰)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    own_kill_source_re=re.compile(r"\.(?:單次擊殺|雙殺|多殺|擊倒)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    match_end_source_re=re.compile(r"\.對戰結束\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    view_replay_re=re.compile(r"(?:死亡畫面|淘汰畫面|擊倒畫面)"),
    source_file_hint=".被擊倒.DVR*.mp4, .死亡.DVR*.mp4, .淘汰.DVR*.mp4, .單次擊殺.DVR*.mp4, .雙殺.DVR*.mp4, .擊倒.DVR*.mp4",
    self_subject_keys=frozenset({"您", "你", "ni"}),
    canonical_action_patterns=(
        ("eliminate", re.compile(r"擊殺|撃殺|擎殺|淘汰|殺")),
        ("knock", re.compile(r"擊倒|击倒|倒了你")),
    ),
)


EN_PROFILE = GameLanguageProfile(
    code="en",
    paddle_lang="en",
    self_strict_re=re.compile(
        r"(?P<action>KNOCKEDYOU(?:OUT)?|KILLEDYOU|KILLSYOU|ELIMINATEDYOU)(?:WITH(?P<weapon>.+?))?"
        r"(?=YOU(?:FINALLY)?(?:KNOCKEDOUT|KILLED|ELIMINATED)|\d+KILLS?|ASSISTS?|$)",
        re.IGNORECASE,
    ),
    self_zone_downed_re=re.compile(
        r"(OUTSIDETHEPLAYZONE|OUTSIDETHEBLUEZONE|YOU(?:WERE)?(?:KNOCKEDOUT|KILLED|DIED).{0,18}(?:PLAYZONE|BLUEZONE|ZONE))",
        re.IGNORECASE,
    ),
    self_fuzzy_re=re.compile(
        r"(KNOCKED.{0,8}YOU|KILL(?:ED|S).{0,4}YOU|ELIMINAT(?:ED)?.{0,4}YOU)",
        re.IGNORECASE,
    ),
    own_kill_strict_re=re.compile(r"(?:^|[，。,.、:：])?YOU(?:FINALLY)?(?:KNOCKEDOUT|KILLED|ELIMINATED)(?!YOU)", re.IGNORECASE),
    own_kill_event_re=re.compile(
        r"YOU(?:FINALLY)?(?P<action>KNOCKEDOUT|KILLED|ELIMINATED)(?P<victim>.+?)"
        r"(?:WITH(?P<weapon>.+?))?"
        r"(?=YOU(?:FINALLY)?(?:KNOCKEDOUT|KILLED|ELIMINATED)|ASSISTS?|$)",
        re.IGNORECASE,
    ),
    own_kill_assist_re=re.compile(r"ASSISTS?", re.IGNORECASE),
    own_kill_delayed_elim_re=re.compile(r"YOUFINALLY(?:KILLED|ELIMINATED)", re.IGNORECASE),
    own_kill_victim_stop_re=re.compile(
        r"(\(\d+\s*[mM]\)|（\d+\s*[mM]）|WITH|(?:\d+\s*)?KILLS?|ASSISTS?|"
        r"YOU(?:FINALLY)?(?:KNOCKEDOUT|KILLED|ELIMINATED)|KNOCKEDYOU|KILLEDYOU|KILLSYOU|ELIMINATEDYOU)",
        re.IGNORECASE,
    ),
    own_kill_scoreboard_assist_re=re.compile(r"\d+KILLS?\d*ASSISTS?", re.IGNORECASE),
    own_kill_count_re=re.compile(r"(?:\d+\s*)?KILLS?\b", re.IGNORECASE),
    weapon_prefix_re=re.compile(r"(?:WITH|USING)(?P<weapon>.+)$", re.IGNORECASE),
    self_weapon_re=re.compile(r"(?:WITH|USING)(?P<weapon>.+)$", re.IGNORECASE),
    molotov_weapon_re=re.compile(r"(molotov|fire\s*-?\s*bomb|gas\s*can|c4)", re.IGNORECASE),
    own_source_re=re.compile(r"\.(?:Knockouted|Death)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    own_kill_source_re=re.compile(
        r"\.(?:Knockout|Single kill|Double kill|Triple kill|Multi kill|Multiple kill)\.DVR(?:_\d+)?\.mp4$",
        re.IGNORECASE,
    ),
    match_end_source_re=re.compile(r"\.(?:Match end|Match ended|Match complete|Game over)\.DVR(?:_\d+)?\.mp4$", re.IGNORECASE),
    view_replay_re=re.compile(r"(?:Death\s*cam|Kill\s*cam|Replay)", re.IGNORECASE),
    source_file_hint=".Knockouted.DVR*.mp4, .Death.DVR*.mp4, .Knockout.DVR*.mp4, .Single kill.DVR*.mp4, .Double kill.DVR*.mp4",
    self_subject_keys=frozenset({"you"}),
    canonical_action_patterns=(
        ("eliminate", re.compile(r"KILL|ELIMINAT", re.IGNORECASE)),
        ("knock", re.compile(r"KNOCK", re.IGNORECASE)),
    ),
)


GAME_LANGUAGE_PROFILES = {profile.code: profile for profile in (ZH_HANS_PROFILE, ZH_HANT_PROFILE, EN_PROFILE)}


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
