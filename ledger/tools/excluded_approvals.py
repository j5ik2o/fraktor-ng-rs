#!/usr/bin/env python3
"""台帳 `cases.excluded` の承認記録の読み込みと件数計算（verify_red から利用する）。

台帳の除外は「人間が承認した除外」だけを already_green の件数式へ加える。この
モジュールは、その承認記録の読み込み（重複・merge を拒否する厳格な SafeLoader）、
要素ごとの承認判定、対象ノードの件数計算を持つ。呼出し側の `verify_red` は
`count_excluded(node_id)` の入口を保ち、呼出し時点の台帳パスを渡す。

PyYAML は読み込み処理の内部で import する。`record_already_green.py` は PyYAML なしの
`uv run python3` から起動され `verify_red` を import するため、このモジュールの
import だけでは PyYAML を要求しない。
"""
import datetime
import os
import re

# 台帳の除外要素が直下に持つ承認記録の形（approved_at は YYYY-MM-DD の暦日）。
APPROVED_AT = re.compile(r"\d{4}-\d{2}-\d{2}")


def ledger_loader():
    """台帳を読む専用の SafeLoader。timestamp を文字列にし、重複キーと merge を拒否する。

    既定の `yaml.SafeLoader` は `approved_at: 2026-02-30` のような引用なしの不正な暦日で
    ValueError を送出し、台帳全体の読み込みが失敗する。対象ノードの選択前に起きると、
    別ノードの誤記だけで対象ノードの承認済み除外を数えられなくなる。専用ローダでは
    timestamp を文字列として返し、暦日の妥当性は対象ノードの `approved_at` だけを
    `is_approved_by_human` で検証する（対象の不正日は fail closed、対象外は判定に影響しない）。
    共有の `yaml.SafeLoader` は変更しない。

    マッピングの重複キーは後勝ちで先の値を失わせるため、同じ `cases` に未承認の
    `excluded` を先、承認済みを後に書くと未承認が隠れる。merge キー (`<<`) は他ノードの
    承認記録を要素へ取り込む。alias (`*anchor`) は他ノードの承認済みマッピングを対象の
    除外要素としてそのまま複製する。いずれも「対象ノード/要素の直下に明示された記録だけを
    受理する」境界を破るので、読み込み境界で拒否する。標準の `construct_mapping` を
    小さく上書きし、(1) `flatten_mapping` の前に生の `node.value` のキーが merge タグなら
    拒否し、(2) `super().construct_mapping(node, deep=deep)` の結果キー数が元の項目数より
    少なければ（後勝ちの上書きが起きた）重複として拒否する。`compose_node` を上書きし、
    alias を構築する前に `AliasEvent` を拒否する（anchor の解決を待たずに落とす）。明示キー
    のみの通常マッピングは PyYAML 標準の構築をそのまま使い、キー数が一致するため受理する。
    拒否は既存の `yaml.YAMLError` 捕捉対象へ伝える。
    """
    import yaml

    class LedgerLoader(yaml.SafeLoader):

        def compose_node(self, parent, index):
            if self.check_event(yaml.AliasEvent):
                event = self.get_event()
                raise yaml.constructor.ConstructorError(
                    None, None,
                    "found alias '*%s' (approval records must be explicit)"
                    % event.anchor, event.start_mark)
            return super().compose_node(parent, index)

        def construct_mapping(self, node, deep=False):
            if isinstance(node, yaml.MappingNode):
                for key_node, _ in node.value:
                    if key_node.tag == "tag:yaml.org,2002:merge":
                        raise yaml.constructor.ConstructorError(
                            "while constructing a mapping", node.start_mark,
                            "found merge key '<<' (approval records must be explicit)",
                            key_node.start_mark)
                entries = len(node.value)
            mapping = super().construct_mapping(node, deep=deep)
            if isinstance(node, yaml.MappingNode) and len(mapping) < entries:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping", node.start_mark,
                    "found duplicate key (%d entries collapsed to %d)"
                    % (entries, len(mapping)))
            return mapping

    LedgerLoader.add_constructor(
        "tag:yaml.org,2002:timestamp",
        lambda loader, node: loader.construct_scalar(node),
    )
    return LedgerLoader


def _load_node(ledger_path: str, node_id: str) -> dict | None:
    """専用ローダで台帳を読み、対象ノードを返す。読めない・無いときは None。"""
    if not os.path.exists(ledger_path):
        return None
    import yaml

    try:
        doc = yaml.load(open(ledger_path, encoding="utf-8").read(),
                        Loader=ledger_loader())
    except (yaml.YAMLError, ValueError, OSError, UnicodeDecodeError):
        return None
    nodes = doc.get("nodes") if isinstance(doc, dict) else None
    if not isinstance(nodes, list):
        return None
    node = next((n for n in nodes
                 if isinstance(n, dict) and n.get("id") == node_id), None)
    if node is None:
        return None
    return node


def count_excluded(ledger_path: str, node_id: str) -> int | None:
    """台帳の対象ノードの cases.excluded の件数を返す。読めないときは None。

    excluded は人間が承認した除外の一覧である。要素は `name`・`reason` に加えて、
    人による承認記録 `approved_by`（空白以外の文字列）と `approved_at`
    （YYYY-MM-DD の暦日。引用なしの表記は `ledger_loader` が文字列として読む）を直下に持つ。
    承認記録が欠落・空欄・null・不正な日付・不正な型の要素が 1 件でもあれば None を返し、
    件数式（passed + excluded == original_cases）が成り立っても already_green にしない
    （既存の name/reason だけの要素も未承認として扱う）。

    台帳を YAML として読み、対象ノードの `cases.excluded` だけを検査する。同じノードの
    `cases` の外にある `excluded` や、除外要素の下位（metadata など）にある承認欄は
    流用しない。`excluded:` が空配列・null なら 0。台帳が YAML として読めない、
    `cases` が無い、要素がマッピングでない、または `excluded` がリストでないときは None
    （除外の件数を確かめられないと判断する）。台帳が無い・ノードが無いときも None。
    重複したマッピングキーや merge キー (`<<`) を含む台帳も、専用ローダが読み込みを
    拒否するため None となる（先の未承認除外を隠す入力や、他ノードの承認記録の流用を
    件数に反映しない）。
    """
    node = _load_node(ledger_path, node_id)
    if node is None:
        return None
    cases = node.get("cases")
    if not isinstance(cases, dict) or "excluded" not in cases:
        return None
    excluded = cases["excluded"]
    if excluded is None:
        return 0
    if excluded_problems(excluded):
        return None
    return len(excluded)


def excluded_problems(excluded) -> list[str]:
    """cases.excluded の記録の不足を返す。rollup.py の混在の勘定と count_excluded が共有する。

    配列でない、要素がマッピングでない、人による承認記録が無い（is_approved_by_human）、
    ケース名が同じ要素が複数ある、のどれかがあれば、その理由を返す。同名の要素を 2 回数えて
    件数式を満たすことを防ぐ。空配列と null は不足なしとする。
    """
    if excluded is None:
        return []
    if not isinstance(excluded, list):
        return ["cases.excluded が配列でない"]
    problems = []
    for index, item in enumerate(excluded):
        if not isinstance(item, dict) or not is_approved_by_human(item):
            problems.append(f"cases.excluded[{index}] に人による承認記録が無い")
    names = [item.get("name").strip() for item in excluded
             if isinstance(item, dict) and is_text(item.get("name"))]
    duplicated = sorted({name for name in names if names.count(name) > 1})
    if duplicated:
        problems.append(f"cases.excluded のケース名が重複している: {', '.join(duplicated)}")
    return problems


def is_approved_by_human(item: dict) -> bool:
    """除外要素が人による承認記録（approved_by と approved_at）を直下に持つかを返す。

    `approved_at` は暦日だけを受理する。台帳は `ledger_loader` で読むため timestamp は
    文字列で届き、`\\d{4}-\\d{2}-\\d{2}` の暦日だけを受理する（時刻を含む timestamp や
    不正な暦日は文字列のまま一致せず拒否する）。
    """
    by = item.get("approved_by")
    if not isinstance(by, str) or not by.strip():
        return False
    at = item.get("approved_at")
    if not isinstance(at, str) or not APPROVED_AT.fullmatch(at):
        return False
    try:
        datetime.date.fromisoformat(at)
    except ValueError:
        return False
    return True


# 作らないケースの要素のうち、空白以外の文字列であるべき欄。承認の2欄は is_approved_by_human が検査する。
NOT_BUILT_TEXT_KEYS = ("name", "reason", "alternative")


def is_text(value) -> bool:
    return isinstance(value, str) and bool(value.strip())


def is_case_name(value) -> bool:
    """ケース名は空白以外の文字列で、前後に空白を持たない（重複の判定をすり抜けさせない）。"""
    return is_text(value) and value == value.strip()


def is_valid_not_built_case(record) -> bool:
    """作らないケースの1要素が、3つの文字列欄と人による承認記録を持つかを返す。"""
    return (isinstance(record, dict)
            and all(is_text(record.get(k)) for k in NOT_BUILT_TEXT_KEYS)
            and is_case_name(record.get("name"))
            and is_approved_by_human(record))


def count_not_built(ledger_path: str, node_id: str) -> int | None:
    """台帳の対象ノードの cases.not_built の件数を返す。数えられないときは None。

    一部のケースだけを作らない区分にしたノード（混在）の already_green の件数式
    （passed + excluded + not_built == original_cases）に加える件数で、rollup.py の混在の
    勘定と同じ規則で数える。欄が無い・null・空配列なら 0。要素が1件でも記録不足
    （is_valid_not_built_case を満たさない）、ケース名の重複、cases.excluded と同じケース名、
    配列でない記録があれば None を返し、already_green にしない（fail closed）。
    """
    node = _load_node(ledger_path, node_id)
    if node is None:
        return None
    cases = node.get("cases")
    if not isinstance(cases, dict):
        return None
    records = cases.get("not_built")
    if records is None or records == []:
        return 0
    if not isinstance(records, list) or not all(is_valid_not_built_case(r) for r in records):
        return None
    names = [r["name"] for r in records]
    if len(set(names)) != len(names):
        return None
    excluded = cases.get("excluded") or []
    if not isinstance(excluded, list):
        return None
    excluded_names = {e.get("name").strip() for e in excluded if isinstance(e, dict) and is_text(e.get("name"))}
    if excluded_names & set(names):
        return None
    return len(records)
