#!/usr/bin/env python3
"""docs/decisions/fable-graph-model-proposal.md が引用する数字を再計算する。

使い方:
    python3 recount_supplement.py --baseline docs/baseline/pekko-v1.7.1/ledger.yaml
    python3 recount_supplement.py --baseline ... --pekko /path/to/pekko   # v1.7.1 の checkout
    python3 recount_supplement.py --baseline ... --pekko ... --markdown   # 補遺の生成部分

--baseline だけなら台帳から出る値（提案書の【台帳】）を出す。
--pekko を付けると、台帳の 1,262 ファイルを正規表現で数え直した値（提案書の【再計測】）も出す。
--markdown を付けると、docs/baseline/pekko-v1.7.1/supplement-recount.md の生成部分を出す。
【再計測】は baseline に入っていない補足値で、正規表現の存在判定による近似である。
Pekko の checkout からは何も実行しない。読むだけである。

--baseline は YAML か JSON を受ける。PyYAML が無い環境では、先に JSON へ変換して渡す:
    ruby -ryaml -rjson -e 'puts JSON.generate(YAML.safe_load(File.read(ARGV[0])))' ledger.yaml > ledger.json
"""
import argparse
import collections
import json
import math
import os
import re
import sys

TESTKIT_LIKE = {"testkit", "actor-testkit-typed", "stream-testkit", "persistence-testkit", "multi-node-testkit", "persistence-tck"}

UNTAGGED = re.compile(r'^\s*"[^"\n]*"\s+in\b', re.M)
TAGGED = re.compile(r'^\s*"[^"\n]*"\s+taggedAs\b([^\n]*?)\bin\b', re.M)
TAG_NAME = re.compile(r"[A-Za-z_]\w*")
FUNSUITE = re.compile(r'\btest\("')
JUNIT = re.compile(r"@Test\b")
ABSTRACT = re.compile(r"^\s*(abstract\s+class|trait)\s+\w+", re.M)
CONCRETE = re.compile(r"^\s*(final\s+)?class\s+\w+", re.M)
USES_SYSTEM = re.compile(
    r"\b(ActorSystem|PekkoSpec|TestKit|TestKitBase|TestProbe|ActorTestKit|ScalaTestWithActorTestKit|StreamSpec|BehaviorTestKit|MultiNodeSpec)\b"
)

Capability = collections.namedtuple("Capability", "group name pattern tier in_baseline")

STREAM_TESTKIT = "stream testkit (TestSubscriber/TestPublisher/TestSink/TestSource)"
JAVA_SERIALIZATION = "Java 直列化 (JavaSerializer/ObjectOutputStream/ObjectInputStream)"

# tier は提案書 4 章の「テスト基盤の層」。in_baseline は baseline の TestKit 一覧（11 項目）にあるかどうか。
CAPABILITIES = [
    Capability("基底 fixture", "extends PekkoSpec", r"extends\s+PekkoSpec\b", None, False),
    Capability("基底 fixture", "extends StreamSpec", r"extends\s+StreamSpec\b", None, False),
    Capability("基底 fixture", "ActorTestKit 系", r"\b(ScalaTestWithActorTestKit|ActorTestKit)\b", None, False),
    Capability("classic", "PekkoSpec", r"\bPekkoSpec\b", 1, False),
    Capability("classic", "TestKit", r"\bTestKit(Base)?\b", 1, True),
    Capability("classic", "testActor", r"\btestActor\b", 1, False),
    Capability("classic", "ImplicitSender", r"\bImplicitSender\b", 1, False),
    Capability("classic", "DefaultTimeout", r"\bDefaultTimeout\b", 1, False),
    Capability("classic", "expectMsg*", r"\bexpectMsg\w*\b", 1, True),
    Capability("classic", "expectNoMessage", r"\bexpectNoMessage\b", 1, True),
    Capability("classic", "TestProbe", r"\bTestProbe\b", 1, True),
    Capability("classic", "expectTerminated", r"\bexpectTerminated\b", 1, False),
    Capability("classic", "lastSender", r"\blastSender\b", 1, False),
    Capability("classic", "EventFilter", r"\b(EventFilter|filterEvents|filterException)\b", 2, False),
    Capability("classic", "TestLatch", r"\bTestLatch\b", 2, True),
    Capability("classic", "within", r"\bwithin\(", 2, True),
    Capability("classic", "awaitAssert", r"\bawaitAssert\b", 2, False),
    Capability("classic", "awaitCond", r"\bawaitCond\b", 2, False),
    Capability("classic", "fishForMessage", r"\bfishForMessage\b", 2, False),
    Capability("classic", "receiveWhile", r"\breceiveWhile\b", 2, False),
    Capability("classic", "receiveN", r"\breceiveN\b", 2, False),
    Capability("classic", "receiveOne", r"\breceiveOne\b", 2, False),
    Capability("classic", "ignoreMsg", r"\bignoreMsg\b", 2, False),
    Capability("classic", "TestActorRef", r"\bTestActorRef\b", 3, True),
    Capability("classic", "TestFSMRef", r"\bTestFSMRef\b", 3, False),
    Capability("classic", "CallingThreadDispatcher", r"\bCallingThreadDispatcher\b", 3, False),
    Capability("classic", "TestBarrier", r"\bTestBarrier\b", 3, True),
    Capability("classic", "setAutoPilot", r"\bsetAutoPilot\b", 3, False),
    Capability("typed", "expectMessage 系 (typed)", r"\bexpectMessage(Type|Class)?\b", None, False),
    Capability("typed", "receiveMessage 系 (typed)", r"\breceiveMessages?\b", None, False),
    Capability("typed", "BehaviorTestKit", r"\bBehaviorTestKit\b", None, False),
    Capability("typed", "TestInbox", r"\bTestInbox\b", None, False),
    Capability("typed", "LoggingTestKit 系", r"\b(LoggingTestKit|LoggingEventFilter)\b", None, False),
    Capability("typed", "LogCapturing", r"\bLogCapturing\b", None, False),
    Capability("typed", "ManualTime", r"\bManualTime\b", None, False),
    Capability("stream", STREAM_TESTKIT, r"\b(TestSubscriber|TestPublisher|TestSink|TestSource)\b", None, False),
    Capability("その他", JAVA_SERIALIZATION, r"\b(JavaSerializer|ObjectOutputStream|ObjectInputStream)\b", None, False),
]
PATTERNS = {c.name: re.compile(c.pattern) for c in CAPABILITIES}
TIER_NAMES = {tier: {c.name for c in CAPABILITIES if c.tier == tier} for tier in (1, 2, 3)}

REPORT_ORDER = [
    "extends PekkoSpec", "extends StreamSpec", "ActorTestKit 系", "expectMessage 系 (typed)", "receiveMessage 系 (typed)",
    STREAM_TESTKIT, JAVA_SERIALIZATION, "EventFilter", "expectTerminated", "testActor", "ImplicitSender", "awaitAssert",
    "TestProbe", "expectMsg*", "TestLatch", "TestActorRef", "CallingThreadDispatcher",
]
MODULE_MATRIX = [
    ("PekkoSpec 継承", "extends PekkoSpec"),
    ("StreamSpec 継承", "extends StreamSpec"),
    ("ActorTestKit 系", "ActorTestKit 系"),
    ("EventFilter 系", "EventFilter"),
    ("testActor", "testActor"),
    ("expectTerminated", "expectTerminated"),
    ("expectMessage 系", "expectMessage 系 (typed)"),
    ("stream testkit", STREAM_TESTKIT),
]


def load(path):
    with open(path, encoding="utf-8") as f:
        if path.endswith(".json"):
            return json.load(f)
        try:
            import yaml
        except ImportError:
            sys.exit("PyYAML が無い。docstring の ruby ワンライナーで JSON に変換して --baseline に渡すこと。")
        return yaml.safe_load(f)


def package_of(file):
    m = re.search(r"org/apache/pekko/(.*)/[^/]+$", file)
    if m:
        return m.group(1)
    m = re.search(r"src/test/[^/]+/(.*)/[^/]+$", file)
    return "(" + (m.group(1) if m else "?") + ")"


def closure(start, graph):
    seen, stack = set(), [start]
    while stack:
        for w in graph.get(stack.pop(), []):
            if w not in seen:
                seen.add(w)
                stack.append(w)
    return seen


def from_ledger(mods):
    by = {m["directory"]: m for m in mods}
    classes = [(m["directory"], t) for m in mods for t in m["test_classes"]]
    non_docs = [(m, t) for m, t in classes if m != "docs"]
    print("== 台帳から出る値 ==")
    print("テストクラス:", len(classes), "/ docs 以外:", len(non_docs), "/ テストを持つモジュール:", sum(1 for m in mods if m["test_classes"]))

    ids = collections.defaultdict(list)
    for m, t in classes:
        ids[f"{m}:{t['name']}"].append(t["file"])
    dup = {k: v for k, v in ids.items() if len(v) > 1}
    mixed = sum(1 for v in dup.values() if len({f.rsplit(".", 1)[1] for f in v}) > 1)
    print("衝突する <module>:<class>:", len(dup), "件 / 巻き込まれるファイル:", sum(len(v) for v in dup.values()),
          "/ java と scala の同名:", mixed, "/ docs 内:", sum(1 for k in dup if k.startswith("docs:")))
    print("一意なファイルパス:", len({t["file"] for _, t in classes}))

    buckets = collections.Counter()
    for _, t in classes:
        c = t["test_cases_approx"]
        buckets["0" if c == 0 else "1-5" if c <= 5 else "6-10" if c <= 10 else "11-20" if c <= 20 else "21-50" if c <= 50 else "51+"] += 1
    cs = sorted(t["test_cases_approx"] for _, t in classes)
    print("ケース数の分布:", dict(buckets), "/ 中央値", cs[len(cs) // 2], "/ p90", cs[int(len(cs) * 0.9)], "/ 最大", cs[-1])
    print("docs 以外で 21 ケース以上:", sum(1 for _, t in non_docs if t["test_cases_approx"] >= 21))

    graph = {m["directory"]: list(m["depends_on"]) for m in mods}
    depth = {}

    def layer(v, path=()):
        if v in path:
            sys.exit(f"循環: {path + (v,)}")
        if v not in depth:
            depth[v] = 1 + max((layer(w, path + (v,)) for w in graph[v]), default=-1)
        return depth[v]

    print("モジュール依存の辺:", sum(len(v) for v in graph.values()), "/ 循環なし / 最長経路の層: 0 から", max(layer(v) for v in graph))
    test_only = {m["directory"] for m in mods if m["stats"]["main_loc"] == 0 and m["test_classes"]}
    print("主ソース 0 行でテストを持つモジュール:", sorted(test_only))
    print("それらを依存先に持つ辺:", sum(1 for v in graph for w in graph[v] if w in test_only),
          "/ testkit 系モジュールを依存先に持つ辺:", sum(1 for v in graph for w in graph[v] if w in TESTKIT_LIKE))
    reverse = collections.defaultdict(list)
    for v in graph:
        for w in graph[v]:
            reverse[w].append(v)
    down = closure("testkit", reverse)
    print("testkit に推移的に依存するモジュール:", len(down), "/ そのテストクラス:", sum(len(by[x]["test_classes"]) for x in down))

    with_types = sum(1 for _, t in classes if t["referenced_types"])
    with_mods = [(m, t) for m, t in classes if t["referenced_modules"]]
    outside = [(m, t["name"], [x for x in t["referenced_modules"] if x not in closure(m, graph) | {m}]) for m, t in with_mods]
    outside = [o for o in outside if o[2]]
    print("参照型が取れたクラス:", with_types, "/ 参照モジュールが取れたクラス:", len(with_mods), "/ うち所属モジュールの依存閉包の外を指すもの:", outside)

    parts = collections.Counter((m, package_of(t["file"])) for m, t in non_docs)
    sizes = sorted(parts.values())
    print("(モジュール, パッケージ) 区画 (docs 以外):", len(parts), "/ 中央値", sizes[len(sizes) // 2], "/ 最大", sizes[-1],
          "/ 1 クラスだけの区画:", sum(1 for s in sizes if s == 1), "/ 15 超の区画:", sum(1 for s in sizes if s > 15),
          "(", sum(s for s in sizes if s > 15), "クラス )")
    print("15 ノード上限でのゴール数: 区画を割るだけなら", sum(math.ceil(s / 15) for s in sizes),
          "/ 同一モジュール内を併合すれば", sum(math.ceil(len(by[m]["test_classes"]) / 15) for m in by if m != "docs" and by[m]["test_classes"]))
    print("multi-jvm のファイル数:", sum(m["stats"]["multi_jvm_source_files"] for m in mods),
          "/ 対象モジュール数:", sum(1 for m in mods if m["stats"]["multi_jvm_source_files"]))
    d = by["docs"]
    print("docs:", len(d["test_classes"]), "クラス /", sum(t["test_cases_approx"] for t in d["test_classes"]), "ケース / 0 件",
          sum(1 for t in d["test_classes"] if t["test_cases_approx"] == 0), "/ 直接依存", len(d["depends_on"]), "モジュール")


def scan(mods, repo):
    rows = []
    for m in mods:
        for t in m["test_classes"]:
            path = os.path.join(repo, t["file"])
            if not os.path.isfile(path):
                sys.exit(f"ファイルが無い: {t['file']}（checkout が v1.7.1 か確認すること）")
            src = open(path, encoding="utf-8", errors="replace").read()
            has = {k for k, r in PATTERNS.items() if r.search(src)}
            occ = {k: len(PATTERNS[k].findall(src)) for k in has}
            tier = "none" if not USES_SYSTEM.search(src) else "3" if has & TIER_NAMES[3] else "2" if has & TIER_NAMES[2] else "1"
            tag_segments = TAGGED.findall(src)
            rows.append(dict(mod=m["directory"], name=t["name"], file=t["file"], pkg=package_of(t["file"]), ledger=t["test_cases_approx"],
                             recount=len(UNTAGGED.findall(src)) + len(FUNSUITE.findall(src)) + len(JUNIT.findall(src)),
                             tagged=len(tag_segments), tags=[name for segment in tag_segments for name in TAG_NAME.findall(segment)],
                             abstract=bool(ABSTRACT.search(src)) and not CONCRETE.search(src),
                             has=has, occ=occ, tier=tier))
    return rows


def tier_tally(rows):
    return {k: (sum(1 for r in rows if r["tier"] == k), sum(r["ledger"] for r in rows if r["tier"] == k)) for k in ("none", "1", "2", "3")}


TIER_SCOPES = [
    ("actor-tests", lambda r: r["mod"] == "actor-tests"),
    ("actor-tests のパッケージ actor", lambda r: r["mod"] == "actor-tests" and r["pkg"] == "actor"),
    ("actor-tests のパッケージ util", lambda r: r["mod"] == "actor-tests" and r["pkg"] == "util"),
    ("testkit", lambda r: r["mod"] == "testkit"),
    ("docs 以外の全体", lambda r: r["mod"] != "docs"),
]


def report(rows):
    print("\n== Pekko の checkout から数え直した値（正規表現、近似） ==")
    print("台帳のケース数を再現できたファイル:", sum(1 for r in rows if r["ledger"] == r["recount"]), "/", len(rows))
    tagged_by_module = collections.Counter()
    for r in rows:
        tagged_by_module[r["mod"]] += r["tagged"]
    print("taggedAs つきのケース（台帳は未計上）:", sum(r["tagged"] for r in rows), "件 /", sum(1 for r in rows if r["tagged"]), "ファイル",
          "/ モジュール別:", {k: v for k, v in tagged_by_module.most_common() if v})
    zero = [r for r in rows if r["ledger"] == 0]
    zero_tagged = [r for r in zero if r["tagged"]]
    zero_abstract = [r for r in zero if not r["tagged"] and r["abstract"]]
    print("ケース数 0 のクラス:", len(zero), "/ taggedAs のケースだけを持つ:", len(zero_tagged), "(", sum(r["tagged"] for r in zero_tagged), "ケース )",
          "/ abstract class か trait だけ:", len(zero_abstract), "/ その他:", len(zero) - len(zero_tagged) - len(zero_abstract))
    print("能力ごとの出現（ファイル数 / 出現回数）:")
    for k in REPORT_ORDER:
        hit = [r for r in rows if k in r["has"]]
        print(f"  {k}: {len(hit)} / {sum(r['occ'][k] for r in hit)}")
    for label, pick in TIER_SCOPES:
        rs = [r for r in rows if pick(r)]
        print(f"層の分布 {label} ({len(rs)}):", tier_tally(rs), "(クラス数, 台帳ケース数)")
    at = [r for r in rows if r["mod"] == "actor-tests"]
    print("actor-tests で PekkoSpec を継承:", sum(1 for r in at if "extends PekkoSpec" in r["has"]), "/", len(at),
          "| EventFilter を使う:", sum(1 for r in at if "EventFilter" in r["has"]), "/", len(at))


def table(headers, body):
    print("| " + " | ".join(headers) + " |")
    print("|" + "---|" * len(headers))
    for row in body:
        print("| " + " | ".join(str(cell) for cell in row) + " |")
    print()


def markdown(rows):
    def files_and_hits(name, scope=rows):
        hit = [r for r in scope if name in r["has"]]
        return len(hit), sum(r["occ"][name] for r in hit)

    ledger_total = sum(r["ledger"] for r in rows)
    tagged_total = sum(r["tagged"] for r in rows)
    tagged_rows = [r for r in rows if r["tagged"]]
    zero = [r for r in rows if r["ledger"] == 0]
    zero_tagged = [r for r in zero if r["tagged"]]
    zero_abstract = [r for r in zero if not r["tagged"] and r["abstract"]]
    modules = list(dict.fromkeys(r["mod"] for r in rows))

    print("## 要点\n")
    print(f"- 台帳のケース数は、{len(rows):,} ファイル中 {sum(1 for r in rows if r['ledger'] == r['recount']):,} ファイルで再現できた。")
    print(f"- 台帳は `taggedAs` つきのケースを数えていない。該当は {len(tagged_rows)} ファイル、{tagged_total} ケースである。"
          f"台帳の合計 {ledger_total:,} に足すと {ledger_total + tagged_total:,} になる。")
    print(f"- 台帳でケース数が 0 の {len(zero)} ファイルのうち、{len(zero_tagged)} ファイルは `taggedAs` つきのケースだけを持つ"
          f"（{sum(r['tagged'] for r in zero_tagged)} ケース）。")
    for label, name in [("`EventFilter` 系", "EventFilter"), ("typed の `expectMessage` 系", "expectMessage 系 (typed)"), ("stream の testkit 系", STREAM_TESTKIT)]:
        files, hits = files_and_hits(name)
        print(f"- {label}は baseline の TestKit 一覧に無い。{files} ファイルに {hits:,} 箇所ある。")
    print()

    print("## 1. ケース数\n")
    print("### モジュール別\n")
    print("`taggedAs` つきのケースを持つモジュールだけを挙げる。合計の行は全モジュールの値である。\n")
    body = []
    for mod in modules:
        rs = [r for r in rows if r["mod"] == mod]
        tagged = sum(r["tagged"] for r in rs)
        if tagged:
            ledger = sum(r["ledger"] for r in rs)
            body.append((f"`{mod}`", f"{ledger:,}", tagged, f"{ledger + tagged:,}"))
    body.sort(key=lambda row: -row[2])
    body.append(("合計", f"{ledger_total:,}", tagged_total, f"{ledger_total + tagged_total:,}"))
    table(["モジュール", "台帳のケース数", "`taggedAs` つき", "和"], body)

    print("### タグ別\n")
    print("1 つのケースに複数のタグが付く場合は、タグごとに数える。\n")
    table(["タグ", "ケース数"], collections.Counter(name for r in rows for name in r["tags"]).most_common())

    print("### `taggedAs` つきのケースを持つファイル\n")
    table(["ファイル", "台帳のケース数", "`taggedAs` つき"],
          [(f"`{r['file']}`", r["ledger"], r["tagged"]) for r in sorted(tagged_rows, key=lambda r: (-r["tagged"], r["file"]))])

    print("### 台帳でケース数が 0 のファイルの内訳\n")
    table(["内訳", "ファイル数"], [
        ("`taggedAs` つきのケースだけを持つ", len(zero_tagged)),
        ("abstract class か trait だけを宣言する", len(zero_abstract)),
        ("その他", len(zero) - len(zero_tagged) - len(zero_abstract)),
        ("合計", len(zero)),
    ])
    print("abstract class か trait だけを宣言するファイルは次のとおりである。`fixture` の候補だが、種別を変えるには人間の承認が要る。\n")
    for r in sorted(zero_abstract, key=lambda r: r["file"]):
        print(f"- `{r['file']}`")
    print()

    print("## 2. TestKit の能力\n")
    print("### 能力ごとの出現\n")
    print("ファイル数は、その正規表現に 1 回以上一致したファイルの数である。"
          "「baseline の一覧」は、baseline の TestKit 一覧（11 項目）にその能力があるかどうかを示す。\n")
    table(["分類", "能力", "正規表現", "ファイル数", "出現回数", "baseline の一覧"],
          [(c.group, c.name, "`" + c.pattern.replace("|", "\\|") + "`", *(f"{n:,}" for n in files_and_hits(c.name)), "あり" if c.in_baseline else "なし")
           for c in CAPABILITIES])

    print("### モジュール別のファイル数\n")
    print("値は、その能力に 1 回以上一致したファイルの数である。\n")
    table(["モジュール", "ファイル数"] + [label for label, _ in MODULE_MATRIX],
          [(f"`{mod}`", sum(1 for r in rows if r["mod"] == mod), *(files_and_hits(name, [r for r in rows if r["mod"] == mod])[0] for _, name in MODULE_MATRIX))
           for mod in modules])

    print("## 3. テスト基盤の層\n")
    print("層は classic の TestKit の能力だけで定義する。層の番号は、ファイルが参照する能力のうち最も高い層である。\n")
    print("- 層なしは、ActorSystem、TestKit、TestProbe、ActorTestKit、StreamSpec などを 1 つも参照しないファイルである。")
    for tier in (1, 2, 3):
        print(f"- 層 {tier} の能力は、" + "、".join(f"`{c.name}`" for c in CAPABILITIES if c.tier == tier) + " である。")
    print()
    body = []
    for label, pick in TIER_SCOPES[:-1]:
        rs = [r for r in rows if pick(r)]
        tally = tier_tally(rs)
        body.append((label, len(rs), *(f"{tally[k][0]}（{tally[k][1]:,}）" for k in ("none", "1", "2", "3"))))
    table(["対象", "ファイル数", "層なし", "層 1", "層 2", "層 3"], body)
    none_files, none_cases = tier_tally([r for r in rows if r["mod"] != "docs"])["none"]
    print(f"括弧の中は台帳のケース数である。docs 以外の全体では、層なしは {none_files} ファイル（{none_cases:,} ケース）である。\n")

    print("## 4. Java 直列化を参照するファイル\n")
    table(["ファイル", "出現回数"],
          [(f"`{r['file']}`", r["occ"][JAVA_SERIALIZATION]) for r in sorted(rows, key=lambda r: r["file"]) if JAVA_SERIALIZATION in r["has"]])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--pekko", default=None, help="Pekko v1.7.1 の checkout。指定すると【再計測】も出す")
    ap.add_argument("--markdown", action="store_true", help="補遺の生成部分を Markdown で出す。--pekko が要る")
    args = ap.parse_args()
    mods = load(args.baseline)["modules"]
    if args.markdown:
        if not args.pekko:
            ap.error("--markdown には --pekko が要る")
        markdown(scan(mods, args.pekko))
        return 0
    from_ledger(mods)
    if args.pekko:
        report(scan(mods, args.pekko))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
