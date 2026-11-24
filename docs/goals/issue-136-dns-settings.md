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
| `str.linesIterator` | `str.lines()` |
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

- `DnsSettings::parse_nameserver_address`：原典は `(.*?)(?::(\d+))?` の全体一致で host と port に分ける。最短一致の host の後ろに `:\d+` が文字列の終わりまで続く位置は、最後の `:` だけである（数字は `:` を含まない）。そこで、最後の `:` の後ろが1文字以上の ASCII 数字なら、そこで分ける。そうでなければ全体を host にし、port は 53 にする。port は `u16` として読み、範囲外は `InvalidPort` にする（原典では `toInt` か `InetSocketAddress` の範囲検査の例外）。host は数値 IP だけを受け付ける。角括弧付きなら中身を IPv6 として、そうでなければ IPv4 か IPv6 として読む。どちらでもなければ `UnsupportedHost` にする。
- `ResolvConfParser::parse_lines`：原典の `parseLines` と同じ順に処理する。search の初期値は空、ndots は 1。各行は Java の `String.trim` と同じく両端の `U+0000`〜`U+0020` を取る。空行と `#`・`;` で始まる行は飛ばす。最初の、Java の `Character.isWhitespace` が真になる文字で label と args に分ける。`domain` は trim した args の1要素、`search` は trim した args を `\s+`（ASCII の6文字）で分けた列で、search を置き換える。`options` は trim しない args を同じ規則で分け、`ndots:` で始まる各要素の残りを `i32` で読む（後のものが勝つ。読めなければ `InvalidNdots`）。他の label は無視する。

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2本と、`DnsSettings.scala`・`ResolvConfParser.scala` を、TAKT が生成した移植の実装・テストと突き合わせた。判定は「等価」で、原典13ケースは観測上等価、除外0件である。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| `host:port` の port は数字列、無ければ 53 | `DnsSettings.scala` L172–183 | 統合テスト4件、単体テスト `an_ipv4_address_without_a_port_gets_the_fallback_port` ほか |
| 角括弧付き IPv6 は括弧の外の `:` で分かれる | 正規表現の最短一致 | 統合テストの IPv6 2件、`a_bracketed_ipv6_address_with_a_non_numeric_port_is_unsupported` |
| 角括弧なしの IPv6 は最後の `:` で分かれる（`::1` は host `:`・port 1） | 正規表現の最短一致 | `a_bare_ipv6_address_is_split_at_its_last_colon` |
| port の範囲外は失敗 | `toInt`、`InetSocketAddress` の範囲検査 | `a_port_above_the_u16_range_is_rejected`、`the_largest_u16_port_is_accepted` |
| 行は trim し、空行とコメント行は無視 | `ResolvConfParser.scala` L49–54 | `ignore_hash_comments`、`ignore_semicolon_comments`、`ignore_tabs_and_spaces`、`a_comment_after_leading_whitespace_is_ignored` |
| 最初の空白で label と args に分ける | L56（`span(!_.isWhitespace)`） | `a_vertical_tab_separates_labels_and_search_elements`、`a_no_break_space_is_not_a_label_separator` |
| `domain` は1要素、`search` は分割、どちらも最後のものが勝つ | L57–62 | 統合テストの search・domain の5件、`a_domain_line_after_a_search_line_replaces_it`、`a_search_line_without_arguments_holds_one_empty_element` |
| `options` の `ndots:` だけを読み、最後のものが勝つ。読めなければ例外 | L63–70 | `ignore_non_ndots_options`、`the_last_ndots_option_wins`、`a_negative_ndots_option_parses`、`a_non_numeric_ndots_option_is_an_error` |
| ndots の既定値は 1、他の label は無視 | L47、L71 | `default_to_ndots_1`、Kubernetes の例（nameserver 行を無視）、`an_unknown_label_in_a_different_case_is_ignored` |

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典の `InetSocketAddress(host, port)` は、host が数値 IP でなければ名前解決を試みる。移植は数値 IP だけを受け付け、それ以外は `UnsupportedHost` を返す。名前解決は issue の対象外である。Java の `InetAddress` が受け付ける省略形の IPv4（`8.8` など）と、スコープ付き IPv6（`%eth0`）も、`core::net` が読まないため受け付けない。
- 原典は失敗を例外で伝える（`parseNameserverAddress` の到達しない `IllegalArgumentException`、`parseLines` の `NumberFormatException`）。移植は `Result` で返す。
- `parseFile`（ファイルの読み込みと `Try`）は移植していない。OS の設定の取得は対象外である。
- 原典の `ResolvConf.ndots` は `Int` で、移植は `i32`。`Integer.parseInt` と `i32::from_str` は、どちらも `+5`・`-1` を読み、範囲外を失敗にする。

### 独立レビュー（別系統のモデル）

`docs/decisions/takt-workflow-design.md` §2.6 に従い、調整担当が手配する。結果はこの節と PR に記録する。

## 製品編集前の実測

第1工程の成果 `c2a7f539f0c7a58756e9e8b347273b1c4cf85f13`（`a926bc9` からの製品差分0。`git diff --stat a926bc9 c2a7f53 -- 'crates/*/src'` は空）で、担当が自分の作業ツリーで次を再実行した。テストの blob は、`nameserver_address_parser_spec.rs` が `e255defd`、`resolv_conf_parser_spec.rs` が `a412de3f` である。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test nameserver_address_parser_spec -- --test-threads=1` | 101 | compile-red。`E0433 cannot find io in kernel`（L22）。宣言4件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 101 | compile-red。`E0433 cannot find io in kernel`（L29）。宣言9件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 2ノードとも `expected_compile_error`、`excluded: 0`。台帳はすでに red で、書き換えなし |

already_green ではない。製品に `kernel::io` モジュールがなく、2本ともコンパイルに失敗した。第2工程では統合テストを変えていない（`git diff f5086f6 405eb30 -- crates/fraktor-actor/tests` は空、blob は上と同じ）。

## 実装後の実測

ゴールブランチへの統合 `405eb30ea73a0a9caca175bea2539eb2716ff9f6` で、担当が自分の作業ツリーで次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test nameserver_address_parser_spec -- --test-threads=1` | 0 | 4 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 0 | 9 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 31 passed |
| `cargo test --workspace` | 0 | 62 スイート、538 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`。fraktor-actor は 178 files、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue136` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK で、対象ノードはすべて green。`G0issue136` は2件で `{'green': 2}`）。

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
