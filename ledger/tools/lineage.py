#!/usr/bin/env python3
"""台帳の退役したノード（最上位の `retired`）と引き継ぎ（`cases.carried`）の検査。

rollup.py、check_needs.py、pekko_baseline.py の check-ledger が同じ規則で使う。規則の説明は
docs/decisions/ledger-schema-draft.md の「退役と引き継ぎ」にある。
"""
import collections
import datetime
import re

import yaml

from excluded_approvals import is_approved_by_human, ledger_loader


def is_text(value) -> bool:
    return isinstance(value, str) and bool(value.strip())


def mapping(value) -> dict:
    """マッピングならそのまま、そうでなければ空のマッピングを返す（壊れた記録で止めない）。"""
    return value if isinstance(value, dict) else {}


def is_case_name(value) -> bool:
    """ケース名は空白以外の文字列で、前後に空白を持たない（重複の判定をすり抜けさせない）。"""
    return is_text(value) and value == value.strip()


def load_retired(path: str):
    """台帳の最上位 `retired` を返す（退役したノード。集計と照合の対象外）。欄が無ければ空配列。

    配列でない値（`{}` など）はそのまま返し、lineage_problems が不正として診断する。
    """
    with open(path, encoding="utf-8") as f:
        retired = (yaml.load(f, Loader=ledger_loader()) or {}).get("retired")
    return [] if retired is None else retired


# 引き継いだケースの要素のうち、空白以外の文字列であるべき欄。
CARRIED_TEXT_KEYS = ("name", "from", "source", "rust_test", "verified_against", "evidence")
RETIRED_AT = re.compile(r"\d{4}-\d{2}-\d{2}")


def is_calendar_day(value) -> bool:
    """`YYYY-MM-DD` の書式で、実在する暦日か（時刻つきや 2026-02-30 は拒否する）。"""
    if not (isinstance(value, str) and RETIRED_AT.fullmatch(value)):
        return False
    try:
        datetime.date.fromisoformat(value)
    except ValueError:
        return False
    return True


def known_baseline_tags() -> set[str]:
    """基準ファイルが知るタグ（現行と過去）。読めなければ空集合（引き継ぎのタグはすべて拒否される）。"""
    try:
        import pekko_baseline
        return pekko_baseline.load().known_tags()
    except Exception:  # noqa: BLE001 — 基準を読めないなら、どのタグも確かめられない
        return set()


def retired_side_problems(by_id: dict, retired: list, problems: dict) -> tuple[dict, dict]:
    """退役した側の記録の不足を problems に足し、id ごとの最初の退役記録と、id ごとの後継の一覧を返す。

    退役側の不足と id の重複は、その id を後継として指すすべてのノードにも伝える。
    """
    retired_by_id: dict[str, dict] = {}
    successors_by_retired: dict[str, list] = collections.defaultdict(list)
    duplicated_ids: set[str] = set()
    for index, r in enumerate(retired):
        rid = r.get("id") if isinstance(r, dict) else None
        if not is_case_name(rid):
            problems[f"retired[{index}]"].append("退役したノードの id が空でない文字列でない")
            successor_id = mapping(r.get("retired")).get("successor") if isinstance(r, dict) else None
            if is_text(successor_id) and successor_id in by_id:
                problems[successor_id].append(f"引き継ぎ元の退役記録 retired[{index}] の id が不正")
            continue
        own: list[str] = []
        if rid in retired_by_id:
            own.append("退役したノードの id が重複している")
            duplicated_ids.add(rid)
        retired_by_id.setdefault(rid, r)
        successors_by_retired[rid].append(mapping(r.get("retired")).get("successor"))
        if rid in by_id:
            own.append("退役したノードと同じ id が現行の nodes にある")
        info = r.get("retired")
        if not isinstance(info, dict):
            problems[rid] += own + ["retired がマッピングでない"]
            continue
        if not is_calendar_day(info.get("at")):
            own.append("retired.at が実在する暦日でない")
        if not is_text(info.get("reason")):
            own.append("retired.reason が空")
        successor_id = info.get("successor")
        successor = by_id.get(successor_id) if is_text(successor_id) else None
        if successor is None:
            own.append("retired.successor が現行のノードの id でない")
        else:
            carried = mapping(successor.get("cases")).get("carried")
            if not (isinstance(carried, list) and any(isinstance(c, dict) and c.get("from") == rid for c in carried)):
                own.append("後継の cases.carried にこのノードから引き継いだケースが無い")
        if own:
            problems[rid] += own
            if successor is not None:
                problems[successor_id].append(f"引き継ぎ元の退役記録 {rid} に不足がある")
    for rid in duplicated_ids:
        for successor_id in successors_by_retired[rid]:
            if is_text(successor_id) and successor_id in by_id:
                problems[successor_id].append(f"引き継ぎ元の退役 id {rid} が重複している")
    return retired_by_id, successors_by_retired


def carried_count_problems(n: dict, cases: dict, carried: list) -> list[str]:
    """引き継ぎを持つノードの件数の不足を返す。green は total = ported + 有効で一意な承認済みの除外、
    passing = ported、ported >= 引き継いだ件数、
    部分移植は ported = passing = 引き継いだ件数かつ total > ported。
    """
    own: list[str] = []
    if n.get("status") == "green":
        total, ported, passing = cases.get("total"), cases.get("ported"), cases.get("passing")
        excluded = cases.get("excluded")
        excluded = [] if excluded is None else excluded
        # 有効で一意な承認済みの除外だけを数える（同じケースの繰り返しで total を埋めない）。
        names = [e.get("name").strip() for e in excluded if isinstance(e, dict) and is_text(e.get("name"))] \
            if isinstance(excluded, list) else []
        carried_names = {c["name"].strip() for c in carried if isinstance(c, dict) and is_text(c.get("name"))}
        approved = (isinstance(excluded, list) and len(names) == len(excluded) == len(set(names))
                    and all(is_text(e.get("reason")) and is_approved_by_human(e) for e in excluded)
                    and not carried_names & set(names))
        if not (approved and all(type(v) is int for v in (total, ported, passing))
                and passing == ported and ported >= len(carried) and total == ported + len(excluded)):
            own.append("引き継ぎを持つ green のノードが total = ported + 有効で一意な承認済みの除外、"
                       "passing = ported、ported >= 引き継いだ件数を満たさない")
    if n.get("status") != "green":
        total, ported, passing = cases.get("total"), cases.get("ported"), cases.get("passing")
        if not (type(ported) is int and type(passing) is int and ported == passing == len(carried)):
            own.append("部分移植のノードの ported と passing が引き継いだ件数と違う")
        if not (type(total) is int and type(ported) is int and total > ported):
            own.append("部分移植のノードの total が ported より大きい整数でない")
    return own


def lineage_problems(nodes: list[dict], retired, known_tags: set[str] | None = None) -> dict[str, list[str]]:
    """退役したノードと、そのケースを引き継いだノード（cases.carried）の記録の不足を、id ごとに返す。

    退役の一覧は配列で、各要素は空でない文字列の id を持ち、id は一意で現行の nodes に無い。
    retired.at は実在する暦日、retired.reason は空でない文字列、retired.successor は現行のノードの id
    （文字列）で、後継の cases.carried に from が自分の要素がある。
    cases.carried は配列で、各要素は CARRIED_TEXT_KEYS を空白以外の文字列で持つ。from の退役記録の
    successor がそのノード自身であること（指定された後継のほかに同じケースを置いて二重に数えない）、
    verified_against は基準ファイルが知るタグであること、ケース名は前後の空白を除いて重複しないこと。
    後継が green でなければ（部分移植）、ported と passing は引き継いだ件数に等しく、total はそれより
    大きい整数でなければならない（suite 全体を green と数えない）。
    退役側だけの不正は、その退役記録の successor のノードにも理由を付けて返す（後継が属するゴールを
    完了にしない）。型の違う欄でも例外を出さず、診断にする。
    """
    tags = known_baseline_tags() if known_tags is None else known_tags
    problems: dict[str, list[str]] = collections.defaultdict(list)
    by_id = {n.get("id"): n for n in nodes if isinstance(n, dict) and is_text(n.get("id"))}
    if not isinstance(retired, list):
        problems["retired"].append("台帳の retired が配列でない")
        retired = []
    retired_by_id, successors_by_retired = retired_side_problems(by_id, retired, problems)
    carried_from: dict[str, int] = collections.Counter()
    evidence_seen: set[tuple] = set()
    for n in by_id.values():
        cases = mapping(n.get("cases"))
        carried = cases.get("carried")
        if carried is None or carried == []:
            continue
        nid = n["id"]
        if not isinstance(carried, list):
            problems[nid].append("cases.carried が配列でない")
            continue
        names = []
        for index, c in enumerate(carried):
            if not (isinstance(c, dict) and all(is_text(c.get(k)) for k in CARRIED_TEXT_KEYS)):
                problems[nid].append(f"cases.carried[{index}] の {'・'.join(CARRIED_TEXT_KEYS)} が揃っていない")
                continue
            names.append(c["name"].strip())
            origin = retired_by_id.get(c["from"])
            if origin is None:
                problems[nid].append(f"cases.carried[{index}] の from が退役したノードでない")
            elif mapping(origin.get("retired")).get("successor") != nid:
                problems[nid].append(f"cases.carried[{index}] の from の退役記録が、このノードを後継に指定していない")
            else:
                # 指定された後継に置いた分だけを数える（ほかのノードの分は直前で拒否している）。
                # 証拠の一意性も正式な後継の中だけで数えるので、並び順で判定は変わらない。
                carried_from[c["from"]] += 1
                evidence = (c["from"], c["source"].strip(), c["rust_test"].strip())
                if evidence in evidence_seen:
                    problems[nid].append(f"cases.carried[{index}] は、同じ退役元・原典の位置・Rust テストのケースと同じ証拠を使っている")
                evidence_seen.add(evidence)
            if c["verified_against"] not in tags:
                problems[nid].append(f"cases.carried[{index}] の verified_against が基準ファイルの知るタグでない")
        if len(set(names)) != len(names):
            problems[nid].append("cases.carried のケース名が重複している（前後の空白を除いて比べる）")
        counted = carried_count_problems(n, cases, carried)
        if counted:
            problems[nid] += counted
    for rid, count in carried_from.items():
        origin = retired_by_id.get(rid)
        ported = mapping(origin.get("cases")).get("ported") if origin else None
        if origin is not None and not (type(ported) is int and count <= ported):
            for successor_id in successors_by_retired[rid]:
                if is_text(successor_id) and successor_id in by_id:
                    problems[successor_id].append(
                        f"退役元 {rid} から引き継いだケース {count} 件が、退役元の移植数（{ported}）を超える")
    return problems
