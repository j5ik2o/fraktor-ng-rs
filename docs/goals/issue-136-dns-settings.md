# ゴール G0issue136：classic の DNS 設定パーサー2スイート

[Issue #136](https://github.com/j5ik2o/fraktor-ng-rs/issues/136) と [Issue #137](https://github.com/j5ik2o/fraktor-ng-rs/issues/137)（Orca 項目204）の対象は、Pekko v1.7.1 の次の2つのテストである。どちらも DNS の設定値を文字列から読む。2つのノードを1つのゴールにまとめた。

- `NameserverAddressParserSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/dns/NameserverAddressParserSpec.scala)。IPv4 と角括弧付き IPv6 の、明示ポートと既定ポート 53 の4ケース
- `ResolvConfParserSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/ResolvConfParserSpec.scala)。resolv.conf 形式の行から search と ndots を読む9ケース
- 原典の実装: [DnsSettings.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala)（`object DnsSettings` の `DnsFallbackPort`、`inetSocketAddress`、`parseNameserverAddress`）と [ResolvConfParser.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/ResolvConfParser.scala)（`ResolvConf`、`parseLines`）

範囲は数値 IP／ポートの解釈と、設定行の解釈だけである。OS の DNS 設定（`/etc/resolv.conf`、JNDI）の取得・変更、`parseFile`、名前解決、DNS 問い合わせ、ネットワーク機能は対象外。

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/io/dns/NameserverAddressParserSpec.scala` | `crates/fraktor-actor/tests/nameserver_address_parser_spec.rs` | 4 | 4 | 4 | 0 |
| `actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/ResolvConfParserSpec.scala` | `crates/fraktor-actor/tests/resolv_conf_parser_spec.rs` | 9 | 9 | 9 | 0 |

合計は原典13件・移植13件・成功13件・除外0件。`needs` はどちらも空で、TestKit の機能は使わない。原典件数は `" in {"` の数で数えた。`ignore` と `pending` はない。

## 実行経路

正規の TAKT で2工程を直列に実行した。工程は2つとも同じゴールに属する。

1. 開始点のブランチ `j5ik2o/issue-136-137-dns-settings` で、台帳の2ノードの `goal:` 行だけを `G0issue136` にした（`a926bc9`、`ledger/README.md` のゴール発行手順）。`rollup.py --goal G0issue136` は2件 `{'unported': 2}`。
2. 担当の端末で takt-mcp（`~/local/takt-custom/bin/takt-mcp`、serverInfo takt 0.76.1）を stdio で起動した。cwd は本体のチェックアウトである。#136・#137 を対象とするゴールとタスクが既存のもの（ゴール全43件、タスク全129件、稼働中0件）にないことを確かめた。そのうえでゴール `7b7dc8b4-ef4e-443f-89b1-c4fc8fc2e4bc`（ブランチ `takt/20261124T1412-goal-7b7dc8b4`、統合先 main）を作った。ワークフローは `simple` で、子のワークフローは review-remediation である。`spark-low` の実効設定は `dgx-flash`（opencode、`dgx/deepseek-v4.1-flash`）で、管理ランタイムの opencode と claude-sdk は `inspectProviderInstallation` で ready だった。
3. 第1工程（workKey `issue-136-137/01-source-red`）はテストの移植と red の実測である。公開 API の名前と原典の入力（`stripMargin` を展開した文字列）を投入文で固定した、手順固定の作業なので、`runtimeAssignment` に `spark-low` を指定した。2026-11-24T14:13:03Z に開始し、14:24:24Z に完了した。差し戻しはなかった。成果 `c2a7f53` を `takt_merge_goal_task` でゴールブランチへ統合した（`f5086f6`）。
4. 第2工程（workKey `issue-136-137/02-implement-green`）は実装、green、回帰の検証である。振る舞いの契約（分割の規則、Java の `trim`・`isWhitespace`・`split("\\s+")` に合わせた文字の扱い、エラーの種類）を投入文で固定したので、こちらも `spark-low` を指定した。14:25:47Z に開始し、14:41:53Z に完了した。差し戻しはなかった。投入文の開始点の SHA が置換漏れ（`__BASE__`）のまま投入されたため、plan の段階で `takt_tell_run` で `f5086f6` と訂正した。成果 `1ad6196` をゴールブランチへ統合した（`405eb30`）。

工程は直列で、並列実行はしていない。TAKT の同時実行上限（`concurrency: 2`）は変えていない。どちらの工程も停滞せず、直接実装への切り替えはしていない。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `a926bc9` | 2ノードの `goal: G0issue136`（台帳2行） |
| テストと台帳 red（第1工程） | `c2a7f53` | 移植テスト2本、2ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green（第2工程） | `1ad6196` | kernel の `io::dns`（5型）、単体テスト31件、2ノードの green。統合テストは不変 |
| ゴール文書 | 本書のコミット | 本書 |

## 原典との照合

### API の対応

| 原典 | 移植 |
|:---|:---|
| `DnsSettings.parseNameserverAddress(str): InetSocketAddress` | `fraktor_actor::kernel::io::dns::DnsSettings::parse_nameserver_address(&str) -> Result<SocketAddr, NameserverAddressError>` |
| `new InetSocketAddress("8.8.8.8", 153)` | `SocketAddr::new("8.8.8.8".parse::<IpAddr>().unwrap(), 153)` |
| `DnsFallbackPort = 53` | 非公開の定数 `FALLBACK_PORT: u16 = 53` |
| `ResolvConfParser.parseLines(lines: Iterator[String]): ResolvConf` | `ResolvConfParser::parse_lines<'a, I: IntoIterator<Item = &'a str>>(I) -> Result<ResolvConf, ResolvConfParseError>` |
| `ResolvConf(search: List[String], ndots: Int)` | `ResolvConf`（`search() -> &[String]`、`ndots() -> i32`、`new(Vec<String>, i32)`） |
| `str.linesIterator`（LF、CR LF、単独の CR で行を分ける） | `text.split(['\n', '\r'])`（CR LF の間にできる空行は `parse_lines` が空行として飛ばす） |
| `shouldEqual` / `should be` | `assert_eq!` |

原典の2つは `@InternalApi private[pekko]` と `private[dns]` である。移植では統合テストから呼ぶために公開した。追加だけで、既存の公開 API は変えていない（`kernel.rs` に `pub mod io;` を1行足した）。`DnsSettings` と `ResolvConfParser` は、原典の `object` に合わせた、フィールドのない型である。

### ケース対応

| 原典のケース | Rust のテスト |
|:---|:---|
| "handle explicit port in IPv4 address" | `handle_explicit_port_in_ipv4_address` |
| "handle explicit port in IPv6 address" | `handle_explicit_port_in_ipv6_address` |
| "handle default port in IPv4 address" | `handle_default_port_in_ipv4_address` |
| "handle default port in IPv6 address" | `handle_default_port_in_ipv6_address` |
| "parse an actual Kubernetes resolv.conf file" | `parse_an_actual_kubernetes_resolv_conf_file` |
| "ignore # comments" | `ignore_hash_comments` |
| "ignore ; comments" | `ignore_semicolon_comments` |
| "use the last search element found" | `use_the_last_search_element_found` |
| "support domain elements" | `support_domain_elements` |
| "use the last domain element found" | `use_the_last_domain_element_found` |
| "ignore non ndots options" | `ignore_non_ndots_options` |
| "ignore tabs and spaces" | `ignore_tabs_and_spaces` |
| "default to ndots 1" | `default_to_ndots_1` |

ResolvConfParserSpec の入力は、Scala の `"""...""".stripMargin` を展開した文字列にした。"use the last domain element found" の入力は、`|` で始まらない最後の行（空白8個）を残した `"domain example.com\ndomain foobar.com\n        "` である。

## 実装

kernel の `io::dns` モジュールに足した。どれも `core` と `alloc` だけを使う（syn-check の `cfg-std-forbid` は違反0）。

- `DnsSettings::parse_nameserver_address`：原典は `(.*?)(?::(\d+))?` の全体一致で host と port に分ける。最短一致の host の後ろに `:\d+` が文字列の終わりまで続く位置は、最後の `:` だけである（数字は `:` を含まない）。そこで、最後の `:` の後ろが1文字以上の ASCII 数字なら、そこで分ける。そうでなければ全体を host にし、port は 53 にする。port は `u16` として読み、範囲外は `InvalidPort` にする（原典では `toInt` か `InetSocketAddress` の範囲検査の例外）。host は数値 IP だけを受け付ける。角括弧付きなら中身を IPv6 として、そうでなければ IPv4 か IPv6 として読む。どちらでもなければ `UnsupportedHost` にする。IPv4 射影の IPv6（`::ffff:a.b.c.d`）は IPv4 にして返す（`IpAddr::to_canonical`）。Java の `InetAddress` が IPv4 射影のリテラルを `Inet4Address` にするのと同じである（PR #138 のレビュー指摘で追加）。
- `ResolvConfParser::parse_lines`：原典の `parseLines` と同じ順に処理する。search の初期値は空、ndots は 1。各行は Java の `String.trim` と同じく両端の `U+0000`〜`U+0020` を取る。空行と `#`・`;` で始まる行は飛ばす。最初の、Java の `Character.isWhitespace` が真になる文字で label と args に分ける。`domain` は trim した args の1要素、`search` は trim した args を `\s+`（ASCII の6文字）で分けた列で、search を置き換える。`options` は trim しない args を同じ規則で分け、`ndots:` で始まる各要素の残りを `i32` で読む（後のものが勝つ。読めなければ `InvalidNdots`）。他の label は無視する。

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2本と、`DnsSettings.scala`・`ResolvConfParser.scala` を、TAKT が生成した移植の実装・テストと突き合わせた。判定は「等価」で、原典13ケースは観測上等価、除外0件である。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| `host:port` の port は数字列、無ければ 53 | `DnsSettings.scala` L172–183 | 統合テスト4件、単体テスト `an_ipv4_address_without_a_port_gets_the_fallback_port` ほか |
| 角括弧付き IPv6 は括弧の外の `:` で分かれる | 正規表現の最短一致 | 統合テストの IPv6 2件、`a_bracketed_ipv6_address_with_a_non_numeric_port_is_unsupported` |
| 角括弧なしの IPv6 は最後の `:` で分かれる（`::1` は host `:`・port 1） | 正規表現の最短一致 | `a_bare_ipv6_address_is_split_at_its_last_colon` |
| IPv4 射影の IPv6 は IPv4 のアドレスになる | Java の `InetAddress.getByName` | `an_ipv4_mapped_ipv6_address_is_returned_as_ipv4`、`an_ipv6_address_that_is_not_ipv4_mapped_stays_ipv6` |
| port の範囲外は失敗 | `toInt`、`InetSocketAddress` の範囲検査 | `a_port_above_the_u16_range_is_rejected`、`the_largest_u16_port_is_accepted` |
| 行は trim し、空行とコメント行は無視 | `ResolvConfParser.scala` L49–54 | `ignore_hash_comments`、`ignore_semicolon_comments`、`ignore_tabs_and_spaces`、`a_comment_after_leading_whitespace_is_ignored` |
| 最初の空白で label と args に分ける | L56（`span(!_.isWhitespace)`） | `an_information_separator_separates_the_label`（`U+001C`）、`a_vertical_tab_separates_labels_and_search_elements`、`a_no_break_space_is_not_a_label_separator` |
| `domain` は1要素、`search` は分割、どちらも最後のものが勝つ | L57–62 | 統合テストの search・domain の5件、`a_domain_line_after_a_search_line_replaces_it`、`a_search_line_without_arguments_holds_one_empty_element` |
| `options` の `ndots:` だけを読み、最後のものが勝つ。読めなければ例外 | L63–70 | `ignore_non_ndots_options`、`the_last_ndots_option_wins`、`a_negative_ndots_option_parses`、`a_non_numeric_ndots_option_is_an_error` |
| ndots の既定値は 1、他の label は無視 | L47、L71 | `default_to_ndots_1`、Kubernetes の例（nameserver 行を無視）、`an_unknown_label_in_a_different_case_is_ignored` |

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典の `InetSocketAddress(host, port)` は、host が数値 IP でなければ名前解決を試みる。移植は数値 IP だけを受け付け、それ以外は `UnsupportedHost` を返す。名前解決は issue の対象外である。Java の `InetAddress` が受け付ける省略形の IPv4（`8.8` など）と、スコープ付き IPv6（`%eth0`）も受け付けない。4つ組の IPv4 は、Java と同じく各部を10進数として読み、先頭のゼロを許す（`008.008.008.008` は `8.8.8.8`。独立レビューの指摘で追加）。数字は ASCII だけを読む。
- 原典は失敗を例外で伝える（`parseNameserverAddress` の `IllegalArgumentException`、`parseLines` の `NumberFormatException`）。移植は `Result` で返す。原典の正規表現の `.` は行終端文字（`\n`、`\r`、`U+0085`、`U+2028`、`U+2029`）に一致しないため、それを含む文字列は全体が一致せず `IllegalArgumentException` になる。移植は、それを `Unparseable` として最初に返す（独立レビューの指摘で追加）。
- `parseFile`（ファイルの読み込みと `Try`）は移植していない。OS の設定の取得は対象外である。
- 原典の `ResolvConf.ndots` は `Int` で、移植は `i32`。`Integer.parseInt` と `i32::from_str` は、どちらも `+5`・`-1` を読み、範囲外を失敗にする。
- `Integer.parseInt` は ASCII 以外の Unicode の10進数字（全角の `５` など）も読む。移植の `i32::from_str` は ASCII の数字だけを読むため、`options ndots:５` は `InvalidNdots` になる。no_std の kernel に Unicode の数字の表を持ち込まないための差で、rustdoc にも書いた。原典テストはこの入力を使わない。（この差は [#159](https://github.com/j5ik2o/fraktor-ng-rs/issues/159) で解消した。末尾の「後続：#159」を参照）
- ndots は専用型にせず `i32` のままにした。`fraktor-coding-rules.md` の domain-primitive の狭い例外（特別な扱いも範囲・書式の確認も要らず、ほかの属性と関係しない属性）に当たる。原典は整数であること以外に条件を置かず（負の値も残る）、このゴールには ndots を使って計算する処理がない。しきい値を解釈する処理を移植するときに専用型を作る。理由は `resolv_conf.rs` のフィールドにも書いた。

### 独立レビュー（別系統のモデル）

- 実施者: 実装者とは別系統の、読み取り専用のレビュー担当。モデルは Codex（OpenAI GPT-6.2 Sol、`gpt-6.2-sol`、medium）で、調整担当が手配した。実装の2工程は DGX（`dgx/deepseek-v4.1-flash`）、自己レビューとレビュー対応は Claude Opus 5.5 なので、`docs/decisions/takt-workflow-design.md` §2.6 に従って別系統のレビューを足した。
- 対象: PR #138 の固定 HEAD `7d7f0399c0f3c919199c04e17ae1f7c731370549`。比較元は `28273d04425ae44c288d7fc2674785acff99e075`。原典は Pekko 固定 SHA `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`。記録は [PR のコメント](https://github.com/j5ik2o/fraktor-ng-rs/pull/138#issuecomment-7135422069)にある。
- 判定: REJECT。原典13件の入力・期待値・件数と台帳は等価で、テストの弱体化はない。既出のボット指摘4件の対応は妥当とされた。新しい指摘は4件（中1、低3）。

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| 中: 先頭ゼロ付きの4つ組 IPv4（`008.008.008.008:53`）を拒否する。OpenJDK では `8.8.8.8:53` | 直す | 4つ組の各部を10進数で読む非公開関数を足した（各部は1文字以上の ASCII 数字で 255 以下、ちょうど4部）。単体テスト `a_zero_padded_ipv4_address_is_read_as_decimal` と `a_zero_padded_ipv4_address_keeps_the_part_rules` を足した。修正前の実装では前者が失敗する |
| 低: 改行を含む値（`8.8.8.8\n:65536`）は、原典では正規表現に一致しない例外の経路に入る。移植は `InvalidPort` を返す | 直す | 行終端文字を含む文字列は、分割の前に `NameserverAddressError::Unparseable`（新しい列挙子）を返す。単体テスト `text_with_a_line_terminator_is_unparseable` を足した。修正前の実装では失敗する |
| 低: 統合テストの補助の `str::lines()` は、Scala の `linesIterator` と違って単独の CR で分けない | 直す | 補助を `text.split(['\n', '\r'])` にし、ヘッダーコメントを直した。原典9ケースの入力・アサーション・件数は変えていない。テストの blob は `a412de3f` から `2755ad0d` になった。製品差分のない `c2a7f53` に新しいテストを置いて red を再実測し、exit 101、`E0433 cannot find io in kernel` を確かめた |
| 低: product-surface-naming（移植元の名前を製品表面に使わない）に、`DnsSettings`・`ResolvConf`・`ResolvConfParser` が当たる | 採らない | この規則の「移植元の名前」は移植元のプロジェクト名（pekko）である。規則は [G0c の改名](g0c-testprobe-and-rename.md)（L13、L68–75）で、`pekko-porting-ng` を `fraktor-porting-ng` にするなど、製品表面とハーネスにあった `pekko` を `fraktor` に改めたときに足された（`1a18a2d`）。原典のクラス名に合わせた型名は、既存の `ByteString`、`ByteStringBuilder`、`FrequencyList`、`RecencyList` と同じ扱いで、規則の対象ではない。製品表面に `pekko` の名前は入れていない |

## 製品編集前の実測

第1工程の成果 `c2a7f539f0c7a58756e9e8b347273b1c4cf85f13`（`a926bc9` からの製品差分0。`git diff --stat a926bc9 c2a7f53 -- 'crates/*/src'` は空）で、担当が自分の作業ツリーで次を再実行した。テストの blob は、`nameserver_address_parser_spec.rs` が `e255defd`、`resolv_conf_parser_spec.rs` が `a412de3f` である。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test nameserver_address_parser_spec -- --test-threads=1` | 101 | compile-red。`E0433 cannot find io in kernel`（L22）。宣言4件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 101 | compile-red。`E0433 cannot find io in kernel`（L29）。宣言9件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 2ノードとも `expected_compile_error`、`excluded: 0`。台帳はすでに red で、書き換えなし |

already_green ではない。製品に `kernel::io` モジュールがなく、2本ともコンパイルに失敗した。第2工程では統合テストを変えていない（`git diff f5086f6 405eb30 -- crates/fraktor-actor/tests` は空、blob は上と同じ）。独立レビューの指摘で `resolv_conf_parser_spec.rs` の補助を変えた後（blob `2755ad0d`）も、`c2a7f53` の作業ツリーにそのファイルを置いて再実測し、exit 101、`E0433 cannot find io in kernel` で compile-red になることを確かめた。

## 実装後の実測

ゴールブランチへの統合 `405eb30ea73a0a9caca175bea2539eb2716ff9f6` で、担当が自分の作業ツリーで次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test nameserver_address_parser_spec -- --test-threads=1` | 0 | 4 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 0 | 9 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 31 passed（ボット指摘の対応後は 34、独立レビューの対応後は 37 passed） |
| `cargo test --workspace` | 0 | 62 スイート、538 passed / 0 failed / 0 ignored（ボット指摘の対応後は 541、独立レビューの対応後は 544 passed） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`。fraktor-actor は 178 files、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

PR #138 のボット指摘と独立レビューの対応（後述）の後の HEAD でも、同じコマンドを再実行して同じ結果を確かめた（`--lib io::dns` は 37 passed）。

台帳は `G0issue136` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK で、対象ノードはすべて green。`G0issue136` は2件で `{'green': 2}`）。

## PR #138 のレビュー指摘への対応

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| Devin: `[::ffff:192.0.2.53]` が IPv6 のまま返る | 直す | Java の `InetAddress` は IPv4 射影のリテラルを `Inet4Address` にする。`to_canonical` で IPv4 にし、単体テスト2件を足した。修正を外すとそのテストが失敗することを確かめた |
| Devin: 全角数字の ndots を Pekko は読むが移植は拒否する | 直す（差異を明記） | 指摘どおり `Integer.parseInt` は Unicode の10進数字を読む。kernel に数字の表を持ち込まず、差異を rustdoc と本書に書いた |
| Devin: ndots に専用型がない | 直す（例外の理由を明記） | domain-primitive の狭い例外に当たる理由を、`resolv_conf.rs` と本書に書いた |
| CodeRabbit: Java 固有のラベル区切りを直接検証する | 直す | `search\u{1C}example.com` が search 行になる単体テストを足した。あわせて、`U+000B` のテストのコメントにあった誤り（Rust の `char::is_whitespace` も `U+000B` を空白とする）を直し、実際に区別している `\s` の分割の説明にした |

## 再現手順

```text
cargo test -p fraktor-actor --test nameserver_address_parser_spec -- --test-threads=1
cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1
cargo test -p fraktor-actor --lib io::dns
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue136
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue136
```

## 後続：#159 ndots の Unicode 十進数字

[Issue #159](https://github.com/j5ik2o/fraktor-ng-rs/issues/159)（Orca 項目239）は、上の「原典との差異」に書いた ndots の差を解消する。PR #158 の独立レビューで、ホスト設定の adaptor を通すと search と ndots がどちらも既定値に戻ることが分かった。原典のテストは増えず、台帳のノード・件数（原典9件・移植9件・成功9件・除外0件）と統合テスト `resolv_conf_parser_spec.rs` は変えていない。記録は、このパーサーのゴールである本書に置く。

- 実行経路: 調整担当の指示で TAKT には登録せず、Claude Opus 5.5 が隔離 worktree（ブランチ `j5ik2o/issue-159-ndots-unicode`、起点 `316d8f5`）で直接実装した
- 原典: [ResolvConfParser.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/ResolvConfParser.scala) の `ndots = option.drop(NdotsOption.length).toInt`

### 原典の受理範囲（一次根拠と実測）

- Pekko v1.7.1 は Scala 2.12.21、2.13.18、3.3.7 でビルドする（`project/Dependencies.scala` L46–49）。`toInt` は、2.13.18 の `StringOps.scala` L907、2.12.21 の `StringLike.scala` L310 で、どちらも `java.lang.Integer.parseInt(s)` である。Scala 3 は 2.13 の標準ライブラリを使う
- Pekko の CI の JDK は 8、11、17、21 である（`.github/workflows/nightly-builds.yml` L118 の `javaVersion: [8, 11, 17, 21]`）
- `Integer.parseInt` を、全コード点（サロゲートを除く `U+0000`〜`U+10FFFF`）の1文字ずつに対して、Temurin 21.0.13、25.0.5、26.0.2.1 で実測した。3版とも結果は同じで、受理は370文字だった。BMP の一般カテゴリ Nd の37の連続10文字（`0`〜`9` の値）だけを受理し、BMP 外の Nd（`U+1D7D3`、`U+104A5` など）は拒否した。`parseInt` は UTF-16 の単位で読み、サロゲートは数字にならないためである
- JDK 8、11、17 は手元にないため実測していない。Unicode の [DerivedAge.txt（16.0.0）](https://www.unicode.org/Public/16.0.0/ucd/DerivedAge.txt) で37の範囲の追加版を調べると、`U+0DE6`（シンハラ文字の Lith 数字）と `U+A9F0`（Tai Laing 数字）だけが Unicode 7.0 で、ほかは 6.0 以前だった。JDK 11 は Unicode 10.0、JDK 17 は 13.0 なので37範囲すべてを持ち、JDK 8（Unicode 6.2）だけがこの2範囲を拒否する
- 符号は先頭の ASCII の `+` か `-` だけ。全角の `＋`・`－`、符号だけ、二重の符号は拒否する。`-2147483648`〜`2147483647` の外は、全角の数字でも拒否する。先頭のゼロは読む。テストに文字列で書いた入力（46件）を Temurin 21.0.13 で `Integer.parseInt` に通し、期待値と一致することを確かめた。範囲ごとのテストは上の全コード点の実測による

### 実装

- `ResolvConfParser::parse_lines` の ndots の読み取りを、`str::parse::<i32>` から非公開関数 `parse_java_int` に替えた。37範囲の `0` のコード点を定数表に持ち、各文字の値を引く。値は Java と同じく負の側で積み、`i32::MIN` で桁あふれしない。採用した表は JDK 11 以降の37範囲である。この基準と JDK 8 との差（2範囲）は、公開 rustdoc（`parse_lines`）と表のコメントに書いた（PR #161 の Devin の指摘で rustdoc に追加）
- kernel の no_std を保つ。`core` と `alloc` だけを使い、syn-check の `cfg-std-forbid` と `port-adaptor-boundary` は違反0である
- 公開 API は変えていない。`ResolvConfParseError::InvalidNdots` と `ResolvConf` の型・関数はそのままで、rustdoc の説明だけを原典に合わせて直した。以前は失敗にしていた入力の一部が成功するだけで、成功していた入力の結果は変わらない。保存済みのデータはない

### テスト

- 単体テスト（`resolv_conf_parser_test.rs`）: 全角と他文字体系の数字、符号つき、37範囲すべての `0` と `9`、各範囲の前後のコード点、BMP 外の数字、Nd 以外の数（`²`、`Ⅴ`、`①`、`五` など）、符号だけと二重の符号、数字以外の文字、先頭ゼロ、`i32` の両端と桁あふれを確かめる。全コード点で受理が370文字になることも確かめる
- adaptor のテスト（`system_host_dns_configuration_test.rs`）: 全角の ndots で search と ndots の両方が読めること、`"default"` 設定で Arabic-Indic の `٣` から ndots 3 と search が入ること、BMP 外の数字では `resolv_conf()` が `None` になり、search が空、ndots が 1、name server は読めることを確かめる

### 製品編集前の実測（red）

テストだけのコミット `a465c71`（起点 `316d8f5` からの製品差分0）で `cargo test -p fraktor-actor --lib -- resolv_conf_parser system_host_dns` を実行し、72件中8件が失敗した。失敗は全角・他文字体系の数字を含む7件（`a_full_width_ndots_digit_parses`、`a_signed_full_width_ndots_value_parses`、`decimal_digits_of_other_scripts_parse`、`every_decimal_digit_run_the_jdk_accepts_parses`、`leading_zeros_parse`、`the_ends_of_the_i32_range_parse`、`a_full_width_ndots_is_read_with_the_search_list`）と `default_settings_take_a_full_width_ndots_from_the_file` である。ASCII だけの入力、BMP 外の数字、符号、桁あふれのテストは修正前から通った。

### 実装後の実測

実装のコミット `ef91eed` で、担当が自分の worktree で次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test --workspace` | 0 | 71 スイート、828 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 0 | 9 passed |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 157 passed |
| `cargo test -p fraktor-actor --lib system_host_dns` | 0 | 43 passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo fmt --check` | 0 | 差分なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`。fraktor-actor は 250 files、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| CI の python テスト5本（`goal_complete_gate_test.py` など） | 0 | すべて成功 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue136` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK、対象ノードはすべて green。`G0issue136` は `{'green': 2}`）。

### 再現手順（JDK の実測）

```java
// Probe.java: java Probe.java で実行する
public class Probe {
  public static void main(String[] a) {
    int accepted = 0;
    for (int cp = 0; cp <= 0x10FFFF; cp++) {
      if (cp >= 0xD800 && cp <= 0xDFFF) continue;
      try { Integer.parseInt(new String(Character.toChars(cp))); accepted++; }
      catch (NumberFormatException e) {}
    }
    System.out.println(System.getProperty("java.version") + " accepted=" + accepted);
  }
}
```
