#!/usr/bin/env python3
"""docs/decisions/fable-graph-model-proposal.md が引用する数字を再計算する。

使い方:
    python3 recount_supplement.py --baseline docs/baseline/pekko-v1.7.1/ledger.yaml
    python3 recount_supplement.py --baseline ... --pekko /path/to/pekko   # v1.7.1 の checkout

--baseline だけなら台帳から出る値（提案書の【台帳】）を出す。
--pekko を付けると、台帳の 1,262 ファイルを正規表現で数え直した値（提案書の【再計測】）も出す。
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
TAGGED = re.compile(r'^\s*"[^"\n]*"\s+taggedAs\b[^\n]*\bin\b', re.M)
FUNSUITE = re.compile(r'\btest\("')
JUNIT = re.compile(r"@Test\b")
ABSTRACT = re.compile(r"^\s*(abstract\s+class|trait)\s+\w+", re.M)
CONCRETE = re.compile(r"^\s*(final\s+)?class\s+\w+", re.M)
USES_SYSTEM = re.compile(
    r"\b(ActorSystem|PekkoSpec|TestKit|TestKitBase|TestProbe|ActorTestKit|ScalaTestWithActorTestKit|StreamSpec|BehaviorTestKit|MultiNodeSpec)\b"
)

TIER1 = {
    "PekkoSpec": r"\bPekkoSpec\b",
    "TestKit": r"\bTestKit(Base)?\b",
    "testActor": r"\btestActor\b",
    "ImplicitSender": r"\bImplicitSender\b",
    "DefaultTimeout": r"\bDefaultTimeout\b",
    "expectMsg*": r"\bexpectMsg\w*\b",
    "expectNoMessage": r"\bexpectNoMessage\b",
    "TestProbe": r"\bTestProbe\b",
    "expectTerminated": r"\bexpectTerminated\b",
    "lastSender": r"\blastSender\b",
}
TIER2 = {
    "EventFilter": r"\b(EventFilter|filterEvents|filterException)\b",
    "TestLatch": r"\bTestLatch\b",
    "within": r"\bwithin\(",
    "awaitAssert": r"\bawaitAssert\b",
    "awaitCond": r"\bawaitCond\b",
    "fishForMessage": r"\bfishForMessage\b",
    "receiveWhile": r"\breceiveWhile\b",
    "receiveN": r"\breceiveN\b",
    "receiveOne": r"\breceiveOne\b",
    "ignoreMsg": r"\bignoreMsg\b",
}
TIER3 = {
    "TestActorRef": r"\bTestActorRef\b",
    "TestFSMRef": r"\bTestFSMRef\b",
    "CallingThreadDispatcher": r"\bCallingThreadDispatcher\b",
    "TestBarrier": r"\bTestBarrier\b",
    "setAutoPilot": r"\bsetAutoPilot\b",
}
OTHER_CAPS = {
    "extends PekkoSpec": r"extends\s+PekkoSpec\b",
    "extends StreamSpec": r"extends\s+StreamSpec\b",
    "ActorTestKit 系": r"\b(ScalaTestWithActorTestKit|ActorTestKit)\b",
    "expectMessage 系 (typed)": r"\bexpectMessage(Type|Class)?\b",
    "receiveMessage 系 (typed)": r"\breceiveMessages?\b",
    "stream testkit (TestSubscriber/TestPublisher/TestSink/TestSource)": r"\b(TestSubscriber|TestPublisher|TestSink|TestSource)\b",
    "Java 直列化 (JavaSerializer/ObjectOutputStream/ObjectInputStream)": r"\b(JavaSerializer|ObjectOutputStream|ObjectInputStream)\b",
}


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


def from_pekko(mods, repo):
    rows = []
    patterns = {k: re.compile(v) for group in (TIER1, TIER2, TIER3, OTHER_CAPS) for k, v in group.items()}
    for m in mods:
        for t in m["test_classes"]:
            path = os.path.join(repo, t["file"])
            if not os.path.isfile(path):
                sys.exit(f"ファイルが無い: {t['file']}（checkout が v1.7.1 か確認すること）")
            src = open(path, encoding="utf-8", errors="replace").read()
            has = {k for k, r in patterns.items() if r.search(src)}
            occ = {k: len(patterns[k].findall(src)) for k in has}
            tier = "none" if not USES_SYSTEM.search(src) else "3" if has & set(TIER3) else "2" if has & set(TIER2) else "1"
            rows.append(dict(mod=m["directory"], name=t["name"], pkg=package_of(t["file"]), ledger=t["test_cases_approx"],
                             recount=len(UNTAGGED.findall(src)) + len(FUNSUITE.findall(src)) + len(JUNIT.findall(src)),
                             tagged=len(TAGGED.findall(src)), abstract=bool(ABSTRACT.search(src)) and not CONCRETE.search(src),
                             has=has, occ=occ, tier=tier))
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
    for k in list(OTHER_CAPS) + ["EventFilter", "expectTerminated", "testActor", "ImplicitSender", "awaitAssert", "TestProbe", "expectMsg*", "TestLatch", "TestActorRef", "CallingThreadDispatcher"]:
        hit = [r for r in rows if k in r["has"]]
        print(f"  {k}: {len(hit)} / {sum(r['occ'][k] for r in hit)}")
    for label, pick in [("actor-tests", lambda r: r["mod"] == "actor-tests"),
                        ("actor-tests のパッケージ actor", lambda r: r["mod"] == "actor-tests" and r["pkg"] == "actor"),
                        ("actor-tests のパッケージ util", lambda r: r["mod"] == "actor-tests" and r["pkg"] == "util"),
                        ("testkit", lambda r: r["mod"] == "testkit"),
                        ("docs 以外の全体", lambda r: r["mod"] != "docs")]:
        rs = [r for r in rows if pick(r)]
        tiers = collections.Counter(r["tier"] for r in rs)
        print(f"層の分布 {label} ({len(rs)}):", {k: (tiers[k], sum(r["ledger"] for r in rs if r["tier"] == k)) for k in ("none", "1", "2", "3")}, "(クラス数, 台帳ケース数)")
    at = [r for r in rows if r["mod"] == "actor-tests"]
    print("actor-tests で PekkoSpec を継承:", sum(1 for r in at if "extends PekkoSpec" in r["has"]), "/", len(at),
          "| EventFilter を使う:", sum(1 for r in at if "EventFilter" in r["has"]), "/", len(at))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--baseline", required=True)
    ap.add_argument("--pekko", default=None, help="Pekko v1.7.1 の checkout。指定すると【再計測】も出す")
    args = ap.parse_args()
    mods = load(args.baseline)["modules"]
    from_ledger(mods)
    if args.pekko:
        from_pekko(mods, args.pekko)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
