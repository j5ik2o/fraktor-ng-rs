"""#206 の線引きの材料：ファイルの本文から、原典（Pekko v2.0.0）に由来する根拠を探す。

材料と判定の規則は docs/design/issue-206-license-policy.md §3.3 にある。
"""
import re
from pathlib import Path

from header_core import MARKER_WINDOW, PORTED_MARKER as MARKER, split_lines_strict, strip_header_for_scan

# 2. 原典への対応（強い根拠）。上から順に採用する。
PORTED = re.compile(r"\b(Ported from|Port of)\b")

MIRRORS = re.compile(r"Mirrors[^\n]{0,160}")

PEKKO_PATH = re.compile(r"org[/.]apache[/.]pekko[/.][\w./]*")

# 5. Pekko への言及だけ（弱い根拠）。`pekko://` のような URI スキームや、
#    テストが使う名前（`PekkoCustomSpec` など）は原典への言及とみなさない。
#    Rust はコメント行だけを見る。
MENTION = re.compile(r"\bPekko\w*|apache/pekko")

PEKKO_PATH_ANY = re.compile(r"org[/.]apache[/.]pekko\b")

RUST_COMMENT = re.compile(r"^\s*(?://|/\*|\*)")

# 6. 原典を指す言及だが、それが Pekko だと書かれていないもの（弱い根拠）。
ORIGINAL = re.compile(r"\bthe original\b|\bupstream\b|\boriginal (?:impl|implementation|code|behaviour|behavior|spec|suite|class|test)s?\b", re.IGNORECASE)

# H1（PR #215 の設計レビュー 2 回目）の規則。どれも crates/ の、ほかの材料が当たらない
# ファイルだけに使い、当たったものは要確認にする。
#   (1) 型名の照合で大文字小文字を区別しない（`AddressFromUriString` と `AddressFromURIString`）。
#   (2) コメントにある Scala 流の `Type.camelMethod` で、Type が原典の型名のもの。
#       snake_case の Rust のメソッドは `\b` の位置で外れる。
#   (3) 「corresponding to」「the ported」「original `X`」。
TYPE_METHOD = re.compile(r"\b([A-Z][A-Za-z0-9]*)\.([a-z][A-Za-z0-9]*)\b")

PORTED_WORDS = re.compile(r"\bcorresponding to\b|\bthe ported\b|\boriginal `[^`]+`", re.IGNORECASE)

# 原典のファイル先頭にない第三者の由来（原典の本文や LICENSE の付録にだけ書かれているもの）。
# 本文に名前が出たら、その表示を写す候補にする（安全側。写しすぎても害は無い）。
TEXT_THIRD_PARTY = (
    (re.compile(r"\bscala\.util\.hashing\.MurmurHash3\b|\bMurmurHash3\.[a-z]\w*"), "Scala（scala-library）: Copyright EPFL and Lightbend, Inc."),
    (re.compile(r"hash-prospector", re.IGNORECASE), "hash-prospector: public domain（Unlicense）"),
)

THIRD_PARTY_TEXT = re.compile("|".join(f"(?:{pattern.pattern})" for pattern, _ in TEXT_THIRD_PARTY), re.IGNORECASE)

GENERIC_STEMS = {"mod", "lib", "main", "prelude", "tests", "test", "setup", "util", "utils", "index"}

RUST_DECL = re.compile(
    r"^\s*pub(?:\s*\([^)]*\))?\s+(?:struct|enum|trait|union)\s+([A-Z][A-Za-z0-9_]*)",
    re.MULTILINE,
)

COMMENT_PREFIX = re.compile(r"^\s*(?://[/!]?|/\*+|\*+/?)\s?")


def strip_own_header(text):
    """ツール管理のライセンスヘッダーブロックを除く（冪等判定のため）。"""
    return strip_header_for_scan(text)


def marker_evidence(text):
    body = strip_own_header(text)
    head_lines = [content for content, _eol in split_lines_strict(body)[:MARKER_WINDOW]]
    for line in head_lines:
        found = MARKER.search(line)
        if found:
            return found.group(0), line.strip()
    return None

def shorten(line):
    snippet = line.strip()
    return snippet if len(snippet) <= 160 else snippet[:157] + "..."

def first_line(text, pattern, require_pekko=False):
    for line in text.splitlines():
        if pattern.search(line) and (not require_pekko or MENTION.search(line)):
            return shorten(line)
    return None

def doc_evidence(text, pekko_any):
    """原典への対応の記述。最初に当たった 1 件を返す。

    `Ported from` / `Port of` は Pekko 以外（旧 Rust ツールなど）からの移植にも
    使われるので、その行が Pekko に触れているときだけ原典の対応とみなす。
    同じく `Mirrors` は、ファイルのどこかに Pekko の記述があるときだけ採用する。
    """
    ported = first_line(text, PORTED, require_pekko=True)
    if ported:
        return ("Ported from" if "Ported" in ported else "Port of"), ported
    if pekko_any:
        mirrors = first_line(text, MIRRORS)
        if mirrors:
            return "Mirrors", mirrors
        path = first_line(text, PEKKO_PATH)
        if path:
            return "原典パス", path
        return None
    mirrors = first_line(text, MIRRORS)
    if mirrors:
        return "Mirrors（原典の明記なし）", mirrors
    return None

def mention_evidence(text, suffix):
    lines = text.splitlines()
    if suffix == ".rs":
        # コメント行の言及を先に採る。コード行の `pekko://` などは最後の手段。
        for line in lines:
            if RUST_COMMENT.match(line) and MENTION.search(line):
                return shorten(line)
    # `pekko_mentioned` は `org.apache.pekko` だけの行でも真になるので、ここでも拾う
    # （拾わないと根拠が None になり、TSV を書く前に落ちる）。
    for line in lines:
        if MENTION.search(line) or PEKKO_PATH_ANY.search(line):
            return shorten(line)
    return None

def original_evidence(text, suffix):
    for line in text.splitlines():
        if suffix == ".rs" and not RUST_COMMENT.match(line):
            continue
        if ORIGINAL.search(line):
            return shorten(line)
    return None

def pekko_mentioned(text):
    return bool(MENTION.search(text) or PEKKO_PATH_ANY.search(text))

def best_path(paths):
    return sorted(paths, key=lambda path: ("/main/" not in path, not path.endswith(".scala"), path))[0]

def type_evidence(text, types):
    """ファイル内で宣言している公開型のうち、原典に同名の型があるもの。"""
    for name in RUST_DECL.findall(text):
        if len(name) < 9:
            continue
        hits = types.get(name)
        if not hits or len(hits) > 6:
            continue
        # テスト専用ファイルにしか無い名前は、本体の型とみなさない。
        if not any("/main/" in path for path in hits):
            continue
        best = sorted(hits, key=lambda path: ("/main/" not in path, not path.endswith(".scala")))[0]
        return f"{name} <- {best}"
    return None

def type_evidence_folded(text, folded):
    """H1 の規則 1：宣言している公開型が、大文字小文字を区別しなければ原典の型と同名。"""
    for name in RUST_DECL.findall(text):
        if len(name) < 9:
            continue
        hits = folded.get(name.lower())
        if not hits or len(hits) > 6 or not any("/main/" in path for path in hits):
            continue
        return f"{name} <- {best_path(hits)}"
    return None

def comment_lines(text):
    return [line for line in text.splitlines() if RUST_COMMENT.match(line)]

def type_method_evidence(text, folded):
    """H1 の規則 2：コメントの `Type.camelMethod` で、Type が原典の型名。"""
    for line in comment_lines(text):
        for found in TYPE_METHOD.finditer(line):
            hits = folded.get(found.group(1).lower())
            if hits:
                return f"{found.group(0)} <- {best_path(hits)} : {shorten(line)}"
    return None

def ported_words_evidence(text):
    """H1 の規則 3：「corresponding to」「the ported」「original `X`」と、材料 8 の語。

    行をまたいだ言い回し（`... as in the` の次の行が `// original: ...`）も拾うため、
    続いたコメントの行をつないだ文で探し、当たった箇所の行を返す。
    """
    paragraph = []
    for line in text.splitlines() + [""]:
        if RUST_COMMENT.match(line):
            paragraph.append(line)
            continue
        if paragraph:
            joined = " ".join(COMMENT_PREFIX.sub("", part, count=1).strip() for part in paragraph)
            found = PORTED_WORDS.search(joined) or ORIGINAL.search(joined)
            if found:
                offset = 0
                for part in paragraph:
                    offset += len(COMMENT_PREFIX.sub("", part, count=1).strip()) + 1
                    if offset > found.start():
                        return shorten(part)
            paragraph = []
    return None

def third_party_text_evidence(text):
    """規則 4（この設計で足した）：Pekko が使う第三者のコード（scala-library、hash-prospector）
    をコメントで引く。原典の表示（§2.1、§3.1）を落とさないため、要確認にする。"""
    for line in comment_lines(text):
        if THIRD_PARTY_TEXT.search(line):
            return shorten(line)
    return None

def camel(stem):
    return "".join(part.capitalize() for part in stem.split("_") if part)

def name_candidates(rust_path):
    stem = Path(rust_path).stem
    bases = [stem]
    for suffix in ("_spec", "_test", "_tests"):
        if stem.endswith(suffix):
            bases.append(stem[: -len(suffix)])
    candidates = set()
    for base in bases:
        if len(base) < 4 or base.lower() in GENERIC_STEMS:
            continue
        candidates.add(base.lower())
        candidates.add(base.lower().replace("_", ""))
        camel_name = camel(base)
        if camel_name:
            candidates.add(camel_name.lower())
    return {candidate for candidate in candidates if len(candidate) >= 4}

def name_evidence(rust_path, index):
    if Path(rust_path).suffix != ".rs":
        return None
    for candidate in sorted(name_candidates(rust_path)):
        hits = index.get(candidate)
        if hits:
            # 本体の定義（main 配下の Scala）を先に見る。docs のサンプルは後に回す。
            hits = sorted(hits, key=lambda path: ("/main/" not in path, not path.endswith(".scala")))
            return hits[0]
    return None

STRONG = {"移植マーカー", "Ported from", "Port of", "Mirrors", "原典パス"}

FLAG_BY_KIND = {
    "台帳": "台帳には移植先として記録されているが、ファイル内に原典対応の記述とマーカーが無い",
    "原典同名": "ファイル内に Pekko の記述が無く、原典に同名のファイルがあるだけ",
    "原典同名（型名）": "ファイル内に Pekko の記述が無いが、宣言している公開型と同名の型が原典にある",
    "Pekko言及": "Pekko への言及はあるが、原典との対応の明記が無い（設計の参考か写しか未確認）",
    "原典への言及（Pekko の明記なし）": "「original」「upstream」への言及はあるが、原典が Pekko だと明記が無い",
    "Mirrors（原典の明記なし）": "「original」への対応は書かれているが、それが Pekko だと明記が無い",
    "原典同名（型名・大文字小文字を区別しない）": "宣言している公開型が、大文字小文字を区別しなければ原典の型と同名（H1 の規則 1）",
    "原典の型とメソッド（Type.method）": "コメントが原典の型とメソッドを `Type.method` の形で引く（H1 の規則 2）",
    "原典への対応・移植の語（Pekko の明記なし）": "「corresponding to」「the ported」「original `X`」、または行をまたいだ「the original」がある（H1 の規則 3）",
    "第三者のコードへの言及": "Pekko が使う第三者のコード（scala-library、hash-prospector）を引く（規則 4）",
}

H1_KINDS = (
    "原典同名（型名・大文字小文字を区別しない）",
    "原典の型とメソッド（Type.method）",
    "原典への対応・移植の語（Pekko の明記なし）",
    "第三者のコードへの言及",
)

def evidence_path(snippet):
    """根拠の中身にある `<- 原典のパス` を取り出す。"""
    if " <- " not in snippet:
        return None
    return snippet.split(" <- ", 1)[1].split(" : ", 1)[0]

def classify(rel, text, scope, ledger, pekko):
    """1 ファイルを判定し、(判定, 根拠の種類, 根拠の中身, 要確認, 要確認の理由) を返す。

    `pekko` は origin_pekko.PekkoIndex（stems・types・folded を使う）。
    """
    marker = marker_evidence(text)
    pekko_any = pekko_mentioned(text)
    doc = doc_evidence(text, pekko_any)
    ledger_hit = ledger.get(rel)
    name_hit = name_evidence(rel, pekko.stems) if scope == "crates" else None
    type_hit = type_evidence(text, pekko.types) if scope == "crates" else None
    crates = scope == "crates"

    if marker:
        kind, snippet = "移植マーカー", marker[1]
    elif doc:
        kind, snippet = doc
    elif original_evidence(text, Path(rel).suffix):
        kind, snippet = "原典への言及（Pekko の明記なし）", original_evidence(text, Path(rel).suffix)
    elif ledger_hit:
        kind, snippet = "台帳", f"{ledger_hit[0]} ({ledger_hit[1]}=rust.test_file)"
    elif name_hit:
        kind, snippet = "原典同名", name_hit
    elif type_hit:
        kind, snippet = "原典同名（型名）", type_hit
    elif pekko_any:
        kind, snippet = "Pekko言及", mention_evidence(text, Path(rel).suffix)
    elif crates and type_evidence_folded(text, pekko.folded):
        kind, snippet = H1_KINDS[0], type_evidence_folded(text, pekko.folded)
    elif crates and type_method_evidence(text, pekko.folded):
        kind, snippet = H1_KINDS[1], type_method_evidence(text, pekko.folded)
    elif crates and ported_words_evidence(text):
        kind, snippet = H1_KINDS[2], ported_words_evidence(text)
    elif crates and third_party_text_evidence(text):
        kind, snippet = H1_KINDS[3], third_party_text_evidence(text)
    else:
        return ("自分で書いた", "なし", "", "", "")

    # 判定は Pekko 由来。要確認の印を決める。
    flag = ""
    reason = ""
    if scope != "crates":
        # 文書・ツールは Pekko のコードの転載かどうかを機械で決められない。
        flag = "要確認"
        if kind in STRONG or kind.startswith("Mirrors"):
            reason = "文書/ツール: 原典のパスや記述を引く（転載の有無は未確認）"
        else:
            reason = "文書/ツール: Pekko の語が出るだけ（転載の可能性は低い）"
    elif kind not in STRONG:
        flag, reason = "要確認", FLAG_BY_KIND[kind]
    elif not ledger_hit and rel.endswith(".rs") and "/tests/" in rel:
        flag, reason = "要確認", "移植の根拠はあるが台帳の test_file に無い"

    return ("Pekko 由来", kind, snippet, flag, reason)
