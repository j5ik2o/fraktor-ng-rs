#!/usr/bin/env python3
"""台帳（ledger/nodes.yaml）からロールアップとゴール完了判定を計算する。

使い方:
    python3 rollup.py --ledger ledger/nodes.yaml
    python3 rollup.py --ledger ledger/nodes.yaml --goal G0a

ロールアップの定義（承認済みグラフモデル）:
  主指標はクラス数ベースの合格率 G / (N - X - B)。N は kind=test のノード数、
  G はそのうち status=green の数、X は status=do_not_port の数、B は status=not_built
  （作らない区分）の数。除外率 X / N と作らない率 B / N を必ず並べて出す。
  fixture は分母に入らない。全体はモジュール平均ではなく、全ノードの合算で出す。
  ケースの勘定として、移植（cases.ported）、除外（cases.excluded）、作らない
  （cases.not_built）の件数を kind=test の全ノードで合算して出す。除外と作らないは、
  承認記録の有効な件数と、台帳に書かれた件数（括弧内）を分けて出す（case_counts）。

作らない区分（not_built）の記録の検査:
  status=not_built のノードは、triage.kind が not_built で triage.reason があり、
  cases.ported と cases.passing が整数の 0 で、cases.not_built の件数が cases.total に等しく、各要素の
  name・reason・alternative が空白以外の文字列で、承認記録（approved_by、approved_at）が
  除外と同じ is_approved_by_human を満たし、ケース名がノード内で重複しないこと。
  満たさないノードは一覧に出し、ゴールの完了に数えない。台帳は除外の検査と同じ
  専用ローダ（excluded_approvals.ledger_loader）で読む。

ゴール完了判定:
  ゴールに属するノード（goal 欄が一致するもの）がすべて green か do_not_port か、
  記録の揃った not_built で、かつ1件以上あること。未完了の内訳（status 別の件数）も出す。
  終了コード: ゴールを指定した場合、完了していなければ 1。
"""
import argparse
import collections
import datetime
import re
import sys

import yaml

from excluded_approvals import is_approved_by_human, ledger_loader

# ゴールの完了に数える status。check_needs.py も同じ集合を使い、
# TAKT の triage（fraktor-triage.md、fraktor-port-node.yaml）も同じ status を処理済みとする。
DONE_STATUSES = ("green", "do_not_port", "not_built")

# 作らないケースの要素のうち、空白以外の文字列であるべき欄。承認の2欄は is_approved_by_human が検査する。
NOT_BUILT_TEXT_KEYS = ("name", "reason", "alternative")


def load_nodes(path: str) -> list[dict]:
    """台帳を読み、ノードの一覧を返す。

    除外の承認検査（excluded_approvals）と同じ専用ローダで読む。timestamp は文字列のまま
    届くので、引用なしの `approved_at: 2026-11-25` も is_approved_by_human が暦日として
    検査できる。重複キー、merge キー、alias は読み込みで拒否する。
    """
    with open(path, encoding="utf-8") as f:
        return (yaml.load(f, Loader=ledger_loader()) or {}).get("nodes", [])


def is_text(value) -> bool:
    return isinstance(value, str) and bool(value.strip())


def mapping(value) -> dict:
    """マッピングならそのまま、そうでなければ空のマッピングを返す（壊れた記録で止めない）。"""
    return value if isinstance(value, dict) else {}


def is_case_name(value) -> bool:
    """ケース名は空白以外の文字列で、前後に空白を持たない（重複の判定をすり抜けさせない）。"""
    return is_text(value) and value == value.strip()


def is_zero(value) -> bool:
    """整数の 0 か（bool の False と、欠落・null は 0 と見なさない）。"""
    return type(value) is int and value == 0


def is_valid_not_built_case(record) -> bool:
    """作らないケースの1要素が、3つの文字列欄と人による承認記録を持つかを返す。"""
    return (isinstance(record, dict)
            and all(is_text(record.get(k)) for k in NOT_BUILT_TEXT_KEYS)
            and is_case_name(record.get("name"))
            and is_approved_by_human(record))


def not_built_problems(node: dict) -> list[str]:
    """status=not_built のノードの記録の不足を返す。不足が無ければ空。"""
    if node.get("status") != "not_built":
        return []
    problems = []
    if not isinstance(node.get("triage"), dict):
        problems.append("triage がマッピングでない")
    if not isinstance(node.get("cases"), dict):
        problems.append("cases がマッピングでない")
    triage = mapping(node.get("triage"))
    if triage.get("kind") != "not_built":
        problems.append("triage.kind が not_built でない")
    if not is_text(triage.get("reason")):
        problems.append("triage.reason が空")
    cases = mapping(node.get("cases"))
    records = cases.get("not_built")
    if not isinstance(records, list) or not records:
        return problems + ["cases.not_built が空"]
    for key in ("ported", "passing"):
        if not is_zero(cases.get(key)):
            problems.append(f"cases.{key} が整数の 0 でない")
    total = cases.get("total")
    if type(total) is not int or total <= 0:
        problems.append("cases.total が正の整数でない")
    elif len(records) != total:
        problems.append("cases.not_built の件数が cases.total と違う")
    # 全件を作らないノードでは、同じケースを除外にも記録できない（区分が二重になる）。
    if cases.get("excluded"):
        problems.append("全件を作らないノードに cases.excluded がある")
    for index, record in enumerate(records):
        if not is_valid_not_built_case(record):
            problems.append(f"cases.not_built[{index}] の name・reason・alternative・承認記録が揃っていない")
    names = [r.get("name").strip() for r in records
             if isinstance(r, dict) and is_text(r.get("name"))]
    duplicated = sorted({name for name in names if names.count(name) > 1})
    if duplicated:
        problems.append(f"cases.not_built のケース名が重複している: {', '.join(duplicated)}")
    return problems


def is_done(node: dict) -> bool:
    return node.get("status") in DONE_STATUSES and not not_built_problems(node)


def case_counts(test_nodes: list[dict]) -> dict:
    """ケースの勘定。除外と作らないは、承認記録の有効な要素の数と、記録された要素の数を分けて出す。

    有効な要素の判定は、除外が is_approved_by_human、作らないが is_valid_not_built_case である。
    作らないは、記録の揃ったノード（not_built_problems が空）の要素だけを有効に数える。
    status が not_built のノードの除外は、区分が二重になるので有効に数えない（記録件数には入る）。
    """
    counts = {"ported": 0, "excluded": 0, "excluded_recorded": 0,
              "not_built": 0, "not_built_recorded": 0}
    for n in test_nodes:
        cases = mapping(n.get("cases"))
        ported = cases.get("ported")
        if type(ported) is int:
            counts["ported"] += ported
        # 配列でない記録は数えない。not_built の不正な記録は not_built_problems が診断に出す。
        excluded = cases.get("excluded")
        if isinstance(excluded, list):
            counts["excluded_recorded"] += len(excluded)
            if n.get("status") != "not_built":
                counts["excluded"] += sum(1 for e in excluded
                                          if isinstance(e, dict) and is_approved_by_human(e))
        not_built = cases.get("not_built")
        if isinstance(not_built, list):
            counts["not_built_recorded"] += len(not_built)
            if n.get("status") == "not_built" and not not_built_problems(n):
                counts["not_built"] += len(not_built)
    return counts


def summarize(nodes: list[dict]) -> dict:
    test_nodes = [n for n in nodes if n.get("kind") == "test"]
    n_total = len(test_nodes)
    green = sum(1 for n in test_nodes if n.get("status") == "green")
    excluded = sum(1 for n in test_nodes if n.get("status") == "do_not_port")
    not_built = sum(1 for n in test_nodes if n.get("status") == "not_built")
    denom = n_total - excluded - not_built
    return {
        "n": n_total,
        "green": green,
        "excluded": excluded,
        "not_built": not_built,
        "pass_rate": (green / denom) if denom else None,
        "exclusion_rate": (excluded / n_total) if n_total else None,
        "not_built_rate": (not_built / n_total) if n_total else None,
        "cases": case_counts(test_nodes),
        "by_status": collections.Counter(n.get("status") for n in test_nodes),
        "fixtures": len(nodes) - n_total,
    }


def fmt_rate(x) -> str:
    return "-" if x is None else f"{x * 100:.1f}%"


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
    """引き継ぎを持つノードの件数の不足を返す。green は total = ported + 承認済みの除外と passing = ported、
    部分移植は ported = passing = 引き継いだ件数かつ total > ported。
    """
    own: list[str] = []
    if n.get("status") == "green":
        total, ported, passing = cases.get("total"), cases.get("ported"), cases.get("passing")
        excluded = cases.get("excluded")
        excluded = [] if excluded is None else excluded
        approved = (isinstance(excluded, list)
                    and all(isinstance(e, dict) and is_approved_by_human(e) for e in excluded))
        if not (approved and all(type(v) is int for v in (total, ported, passing))
                and passing == ported and total == ported + len(excluded)):
            own.append("引き継ぎを持つ green のノードが total = ported + 承認済みの除外、"
                       "passing = ported を満たさない")
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
            evidence = (c["from"], c["source"].strip(), c["rust_test"].strip())
            if evidence in evidence_seen:
                problems[nid].append(f"cases.carried[{index}] は、同じ退役元・原典の位置・Rust テストのケースと同じ証拠を使っている")
            evidence_seen.add(evidence)
            origin = retired_by_id.get(c["from"])
            if origin is None:
                problems[nid].append(f"cases.carried[{index}] の from が退役したノードでない")
            elif mapping(origin.get("retired")).get("successor") != nid:
                problems[nid].append(f"cases.carried[{index}] の from の退役記録が、このノードを後継に指定していない")
            else:
                # 指定された後継に置いた分だけを数える（ほかのノードの分は直前で拒否している）。
                carried_from[c["from"]] += 1
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


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--goal", default=None, help="ゴール ID を指定すると完了判定も行う")
    args = ap.parse_args()

    nodes = load_nodes(args.ledger)

    overall = summarize(nodes)
    print(
        f"全体: N={overall['n']} green={overall['green']} do_not_port={overall['excluded']} "
        f"not_built={overall['not_built']} "
        f"合格率={fmt_rate(overall['pass_rate'])} 除外率={fmt_rate(overall['exclusion_rate'])} "
        f"作らない率={fmt_rate(overall['not_built_rate'])} "
        f"(fixture {overall['fixtures']} 件は分母外)"
    )
    print(f"  status 内訳: {dict(overall['by_status'])}")
    cases = overall["cases"]
    print(
        f"  ケース: 移植={cases['ported']} "
        f"除外={cases['excluded']}（記録 {cases['excluded_recorded']}） "
        f"作らない={cases['not_built']}（記録 {cases['not_built_recorded']}）"
        f"  ※除外・作らないは承認記録の有効な件数、括弧内は台帳に書かれた件数"
    )
    for n in nodes:
        problems = not_built_problems(n)
        if problems:
            print(f"  作らない区分の記録が不足: {n['id']}: {'; '.join(problems)}")
    retired = load_retired(args.ledger)
    lineage = lineage_problems(nodes, retired)
    print(f"  退役: {len(retired) if isinstance(retired, list) else 0} 件（集計の対象外）")
    for node_id, problems in lineage.items():
        print(f"  退役・引き継ぎの記録が不足: {node_id}: {'; '.join(problems)}")

    by_module: dict[str, list[dict]] = collections.defaultdict(list)
    for n in nodes:
        by_module[n["pekko"]["module"]].append(n)
    print("モジュール別:")
    for module in sorted(by_module):
        s = summarize(by_module[module])
        if s["n"] == 0:
            continue
        print(
            f"  {module:28s} N={s['n']:4d} green={s['green']:4d} "
            f"合格率={fmt_rate(s['pass_rate']):>6s} 除外率={fmt_rate(s['exclusion_rate']):>6s} "
            f"作らない率={fmt_rate(s['not_built_rate']):>6s}"
        )

    if args.goal:
        members = [n for n in nodes if n.get("goal") == args.goal]
        if not members:
            gone = [r for r in (retired if isinstance(retired, list) else [])
                    if isinstance(r, dict) and r.get("goal") == args.goal]
            if gone:
                for r in gone:
                    print(f"ゴール {args.goal}: 属するノード {r.get('id')} は退役した"
                          f"（後継 {mapping(r.get('retired')).get('successor')}）。完了の判定はしない")
                return 1
            print(f"ゴール {args.goal}: 属するノードが無い（未発行か ID の誤り）")
            return 1
        pending = [n for n in members if not is_done(n) or n.get("id") in lineage]
        print(
            f"ゴール {args.goal}: ノード {len(members)} 件、"
            f"status 内訳 {dict(collections.Counter(n.get('status') for n in members))}"
        )
        if pending:
            print(f"  未完了 {len(pending)} 件:")
            for n in pending:
                problems = not_built_problems(n)
                detail = f"、作らない区分の記録が不足: {'; '.join(problems)}" if problems else ""
                print(f"    - {n['id']} ({n.get('status')}{detail})")
            return 1
        print(f"  完了: 全ノードが green か do_not_port か not_built")
        return 0
    # ゴールを指定しない全体の検査では、退役と引き継ぎの記録の不正を 0 以外の終了値で知らせる。
    return 1 if lineage else 0


if __name__ == "__main__":
    raise SystemExit(main())
