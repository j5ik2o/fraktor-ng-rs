# ゴール G0issue139：classic のバージョン比較と互換性判定の2スイート

[Issue #139](https://github.com/j5ik2o/fraktor-ng-rs/issues/139) と [Issue #140](https://github.com/j5ik2o/fraktor-ng-rs/issues/140)（Orca 項目217）の対象は、Pekko v1.7.1 の次の2つのテストである。どちらもバージョン文字列を扱うので、2つのノードを1つのゴールにまとめた。ただし契約は別々で、比較は汎用の順序、互換性判定は独自の規則である。

- `VersionSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/VersionSpec.scala)。1〜3成分の比較と等価、4成分と数字でない成分のエラー、修飾子、dynver、英数字の8ケース（アサーション49個）
- `PekkoVersionSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/PekkoVersionSpec.scala)。正式版・RC・マイルストーン、メジャー違い、マイナー・パッチの不足、SNAPSHOT、判読できない形式の13ケース（`require` の呼び出し26個）
- 原典の実装: [Version.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/util/Version.scala)（`parse`、`compareTo`、`equals`、`hashCode`、`toString`）と [PekkoVersion.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/PekkoVersion.scala)（3引数の `require`）

範囲は、バージョン値の比較と、要求・実バージョンを明示的に渡す互換性判定だけである。Cargo の依存解決、crate の公開版、ビルド時・起動時の強制（原典の2引数 `require` と `Version.current`）、既存のリリース手順、公開 API の非互換変更は対象外。`Version.Zero` も使い道がないので移植していない。

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/VersionSpec.scala` | `crates/fraktor-actor/tests/version_spec.rs` | 8 | 8 | 8 | 0 |
| `actor-tests/src/test/scala/org/apache/pekko/PekkoVersionSpec.scala` | `crates/fraktor-actor/tests/fraktor_version_spec.rs` | 13 | 13 | 13 | 0 |

合計は原典21件・移植21件・成功21件・除外0件。`needs` はどちらも空で、TestKit の機能は使わない。原典件数は `" in {"` の数で数えた。`ignore` と `pending` はない。アサーションは原典と同じ順・同じ入力で1対1に移した（49個と26個）。

## 実行経路

正規の TAKT で2工程を直列に実行した。工程は2つとも同じゴールに属する。

1. 開始点のブランチ `j5ik2o/issue-139-140-version` で、台帳の2ノードの `goal:` 行だけを `G0issue139` にした（`c17c22f`、`ledger/README.md` のゴール発行手順）。`rollup.py --goal G0issue139` は2件 `{'unported': 2}`、`check_needs.py` は閉性 OK。
2. 担当の端末で takt-mcp（`~/local/takt-custom/bin/takt-mcp`、serverInfo takt 0.76.1）を stdio で起動した。cwd は本体のチェックアウトである。#139・#140 を対象とするゴールとタスクが既存のもの（ゴール全44件、タスク全131件、稼働中0件）にないことを確かめた。そのうえでゴール `2672f1dd-edca-4b9c-9219-4d3856fed446`（ブランチ `takt/20261124T1521-goal-2672f1dd`、統合先 main）を作った。ワークフローは `simple`。`spark-low` の実効設定は `dgx-flash`（opencode、`dgx/deepseek-v4.1-flash`）で、DGX の vLLM（`/v1/models` に `deepseek-v4.1-flash`）と、管理ランタイムの opencode・claude-sdk（`inspectProviderInstallation` で ready）を確かめた。
3. 第1工程（workKey `issue-139-140/01-source-red`）はテストの移植と red の実測である。公開 API の名前と原典の対応を投入文で固定した、手順固定の作業なので、`runtimeAssignment` に `spark-low` を指定した。2026-11-24T15:22:59Z に開始し、15:34:33Z に完了した。差し戻しはなかった。成果 `632513c` を `takt_merge_goal_task` でゴールブランチへ統合した（`b3d9f23`）。
4. 第2工程（workKey `issue-139-140/02-implement-green`）は実装、green、回帰の検証である。解析・比較・判定の契約を投入文で固定したので、こちらも `spark-low` を指定した。15:35:56Z に開始し、15:59:13Z に完了した。差し戻しはなかった。成果 `4caacbd` をゴールブランチへ統合した（`a44bcd7`）。
5. 担当の自己レビューで、互換性判定の解析が入力によって panic する不具合を見つけ、PR のブランチで直した（`3e13d5c`、後述）。

工程は直列で、並列実行はしていない。TAKT の同時実行上限（`concurrency: 2`）は変えていない。どちらの工程も停滞せず、直接実装への切り替えはしていない。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `c17c22f` | 2ノードの `goal: G0issue139`（台帳2行） |
| テストと台帳 red（第1工程） | `632513c` | 移植テスト2本、2ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green（第2工程） | `4caacbd` | kernel の `version`（4つの公開型と非公開の `ReleaseVersion`）、単体テスト33件、2ノードの green。統合テストは不変 |
| 自己レビューの修正 | `3e13d5c` | 空や ASCII 以外の修飾子で panic していた解析を直し、単体テスト2件を足した |
| ゴール文書 | `5be8022` | 本書 |
| PR #141 の指摘対応 | 本書を更新したコミット | M・RC の種別を `PreReleaseKind` 列挙型にした（後述）。単体テスト2件 |

## 原典との照合

### API の対応

| 原典 | 移植 |
|:---|:---|
| `Version(str)`（構築時に解析し、失敗は例外） | `fraktor_actor::kernel::version::Version::parse(&str) -> Result<Version, VersionParseError>` |
| `IllegalArgumentException`（空、成分数が0か4以上） | `VersionParseError::Empty`、`VersionParseError::UnsupportedSegmentCount` |
| `NumberFormatException`（数字でない成分） | `VersionParseError::InvalidNumber` |
| `equals` / `compareTo` / `hashCode` | `PartialEq`・`Eq` / `PartialOrd`・`Ord` / `Hash`（手書きで、互いに整合させた） |
| `toString`（元の文字列） | `Display` と `as_str()` |
| `PekkoVersion.require(libraryName, requiredVersion, currentVersion)` | `FraktorVersion::require(&str, &str, &str) -> Result<(), UnsupportedFraktorVersion>` |
| `UnsupportedPekkoVersion`（文言の例外） | `UnsupportedFraktorVersion`（`library_name()`・`required_version()`・`current_version()`、`Display`、`core::error::Error`） |
| `should ===` / `!==` / `be >` / `be <` / `intercept[...]` | `assert_eq!` / `assert_ne!` / `assert!(a > b)` / `assert!(a < b)` / `Err` の比較か `is_err()` |

命名規則 product-surface-naming（`.takt/workflows/rules/fraktor-coding-rules.md`）により、製品の型名に移植元のプロジェクト名を使わない。`PekkoVersion` を `FraktorVersion`、`UnsupportedPekkoVersion` を `UnsupportedFraktorVersion` にし、エラーの文言の `Pekko` も `Fraktor` にした。[G0c の改名](g0c-testprobe-and-rename.md)と同じ扱いである。`Version` は原典のクラス名で、規則の対象ではない（`FrequencyList` などと同じ）。原典テストのデータ文字列（ライブラリ名 `"PekkoVersionSpec"`）はそのまま使った。テスト関数名の `current Pekko version` は `current_version` にし、原典のケース名はコメントに残した。追加だけで、既存の公開 API は変えていない（`kernel.rs` に `pub mod version;` を1行足した）。

### ケース対応

| 原典のケース | Rust のテスト |
|:---|:---|
| "compare 3 digit version" | `compare_3_digit_version` |
| "not support more than 3 digits version" | `not_support_more_than_3_digits_version` |
| "compare 2 digit version" | `compare_2_digit_version` |
| "compare single digit version" | `compare_single_digit_version` |
| "compare extra" | `compare_extra` |
| "require digits" | `require_digits` |
| "compare dynver format" | `compare_dynver_format` |
| "compare extra without digits" | `compare_extra_without_digits` |
| "succeed if version is ok" | `succeed_if_version_is_ok` |
| "succeed if version is RC and ok" | `succeed_if_version_is_rc_and_ok` |
| "fail if version is RC and not ok" | `fail_if_version_is_rc_and_not_ok` |
| "succeed if version is milestone and ok" | `succeed_if_version_is_milestone_and_ok` |
| "fail if version is milestone and not ok" | `fail_if_version_is_milestone_and_not_ok` |
| "fail if major version is different" | `fail_if_major_version_is_different` |
| "fail if minor version is too low" | `fail_if_minor_version_is_too_low` |
| "fail if patch version is too low" | `fail_if_patch_version_is_too_low` |
| "succeed if current Pekko version is SNAPSHOT" | `succeed_if_current_version_is_snapshot` |
| "succeed if current Pekko version is timestamped SNAPSHOT" | `succeed_if_current_version_is_timestamped_snapshot` |
| "succeed if required Pekko version is SNAPSHOT" | `succeed_if_required_version_is_snapshot` |
| "succeed if required Pekko version is timestamped SNAPSHOT" | `succeed_if_required_version_is_timestamped_snapshot` |
| "silently comply if current version is incomprehensible" | `silently_comply_if_current_version_is_incomprehensible` |

## 実装

kernel の `version` モジュールに足した。どれも `core` と `alloc` だけを使う（syn-check の `cfg-std-forbid` は違反0）。

- `Version::parse`：原典の `parse` と同じ順に読む。`.` で分け、Java の `String.split` と同じく末尾の空の成分を捨てる（`"1.2."` は `"1.2"` と等しく、`"."` は成分0個で `UnsupportedSegmentCount`）。成分1つなら、先頭が数字で `i32` として読めれば1つ目の数、そうでなければ全体を修飾子にする。成分2つと3つは、最後の成分を最初の `-` か `+` で数と残りに分け、残りの先頭が `数字列-` なら dynver の数として読む（読めなければ残りを修飾子に残し、エラーにしない）。数は `[i32; 4]`、修飾子は文字列で持つ。
- `Version` の比較：数を先頭から比べ、等しければ修飾子を比べる。修飾子のない正式版は、同じ数の修飾子付きより大きい。両方に修飾子があれば文字列の順序で比べる。`PartialEq` と `Hash` は数と修飾子だけを見る（元の文字列は見ない。原典の `hashCode` と同じ成分）。`a == b` と `a.cmp(&b) == Equal` は同値で、等しい値のハッシュは等しい。
- `FraktorVersion::require`：原典の3引数 `require` と同じ判定をする。`Version` は使わない。2つの文字列が等しければ受理する。それぞれを `(\d+)\.(\d+)\.(\d+)(?:-(M|RC)(\d+))?` の全体一致で読む非公開の `ReleaseVersion` に渡し（正規表現のクレートは足さず手書き）、どちらかが一致しなければ受理する。major は文字列として比べる。minor の文字列が違えば minor の数値だけを比べ、patch は見ない。minor が同じで patch の文字列が違えば patch の数値だけを比べる。3つとも同じなら M・RC を比べる（RC は M より新しい。要求が正式版で実が M・RC なら不適合。要求が M・RC で実が正式版なら受理）。判定のロジックは `ReleaseVersion::satisfies` に置いた（rich-domain-model）。M・RC の種別は非公開の列挙型 `PreReleaseKind`（`Milestone`・`ReleaseCandidate`）で持ち、比較は4つの組を網羅する `match` にした（PR #141 のレビュー指摘で追加）。

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2本と、`Version.scala`・`PekkoVersion.scala` を、TAKT が生成した移植の実装・テストと突き合わせた。判定は「等価」で、原典21ケースは観測上等価、除外0件である。不具合を1件見つけて直した。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| 1〜3成分の数と修飾子の比較。欠けた成分は0 | `parse`、`compareTo` | 統合テストの比較5件、`drops_trailing_empty_components_when_splitting` |
| 4成分以上と空は `IllegalArgumentException` | `parse` の `else` と空の検査 | `not_support_more_than_3_digits_version`、`classifies_empty_short_and_overlong_inputs` |
| 数字でない成分は `NumberFormatException` | `toInt` | `require_digits`、`rejects_numbers_that_do_not_fit_in_i32`、`splits_qualifier_on_the_first_sign` |
| 成分1つで数として読めなければ英数字として扱う | `parse` の成分1つの分岐 | `compare_extra_without_digits`、`keeps_an_out_of_range_single_component_as_a_qualifier` |
| dynver の数は読めなければ修飾子に残る | `parseDynverPart` の `catch` | `compare_dynver_format`、`keeps_an_unreadable_dynver_number_in_the_qualifier` |
| 等価は比較が0、ハッシュは数と修飾子から | `equals`、`hashCode` | `keeps_equality_ordering_and_hashing_in_agreement` |
| 文字列が等しければ受理 | `require` の最初の `if` | `keeps_the_original_cases_for_an_identical_version`、`silently_comply_if_current_version_is_incomprehensible` |
| パターンに全体一致しなければ受理 | `case _ =>` の2か所 | SNAPSHOT の4件、`complies_when_the_pattern_does_not_match_the_whole_string`、`complies_when_a_lowercase_qualifier_does_not_match`、`an_empty_or_non_ascii_qualifier_is_accepted_as_unknown` |
| major は文字列比較、minor・patch は文字列が違うときだけ数値比較 | `require` の入れ子の `if` | `fail_if_major_version_is_different`、`rejects_a_textually_different_major`、`compares_only_minors_when_the_minor_text_differs`、`compares_patch_as_a_number_ignoring_leading_zeros` |
| M・RC の順序 | `require` の `match` | RC・M の統合テスト5件、`accepts_a_milestone_requirement_against_a_release_candidate`、`rejects_a_requirement_that_is_a_newer_milestone` |

見つけた不具合：第2工程の `ReleaseVersion` は、`-` の後ろの修飾子を `split_at(1)` で分けていた。修飾子が空（`"2.5.6-"`）か、先頭が複数バイトの文字（`"2.5.6-é1"`）だと、文字境界でない位置で分けて panic する。原典の正規表現は一致しないだけなので、`FraktorVersion::require` は受理を返すべきである。`strip_prefix("RC")` と `strip_prefix('M')` で分けるように直し、単体テスト `a_dash_without_a_milestone_does_not_match` と `an_empty_or_non_ascii_qualifier_is_accepted_as_unknown` を足した。修正前の `a44bcd7` では2件とも panic で失敗することを確かめた。あわせて、1か所からしか呼ばれない補助関数 `mismatch` をやめ、`UnsupportedFraktorVersion::new` を直接呼ぶようにした。

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典の `compareTo` は、数が等しいときの修飾子の比較で `else` が抜けている。そのため `Version("1.2-M1").compareTo(Version("1.2.0"))` は負、`Version("1.2.0").compareTo(Version("1.2-M1"))` も負（`"".compareTo("M1")`）になり、反対称でない。原典テストは前者の向きだけを見る。移植は Rust の `Ord` の契約に合わせて反対称にし、修飾子のない正式版を常に大きくした（`orders_an_unqualified_release_above_the_same_number_with_a_qualifier`）。
- 原典の `compareTo` は数の差を `Int` の引き算で求めるため、`2147483647` と `-1` のように差が桁あふれする組で符号が逆になる。移植は `i32` を直接比べ、桁あふれしない（`orders_i32_extremes_without_overflow`）。
- 修飾子の比較は、原典が `String.compareTo`（UTF-16 の符号単位の順）、移植が `str::cmp`（UTF-8 のバイト順、つまり符号位置の順）である。違いが出るのは、`U+10000` 以上の文字と `U+E000`〜`U+FFFF` の文字を比べるときだけである。
- `Integer.parseInt` と `Character.isDigit` は ASCII 以外の Unicode の10進数字も数字とみなす。移植は ASCII の数字だけを読む。たとえば `"١.٢"` は原典では `1.2`、移植では `InvalidNumber` になる。no_std の kernel に Unicode の数字の表を持ち込まないための差で、[G0issue136](issue-136-dns-settings.md) の ndots と同じ扱いである。`require` の `\d` は Java の既定でも ASCII だけなので、差はない。
- 原典の `require` は、minor・patch・M・RC の番号が `Int` を超えると `toInt` で `NumberFormatException` を投げる。移植は数字列を値として比べ、桁あふれしない（`compares_large_numbers_without_overflow`）。原典テストはこの入力を使わない。
- 原典は失敗を例外で伝える。移植は `Result` で返す。
- 原典の2引数 `require`（`Version.current` を使う）と `Version.Zero` は移植していない。起動時の強制と crate の版は issue の対象外である。

### 独立レビュー（別系統のモデル）

`docs/decisions/takt-workflow-design.md` §2.6 に従い、既出の指摘に対応した後の固定 HEAD で、調整担当が別系統の読み取り専用レビューを手配する。結果と処理はこの節と PR に記録する。

## PR #141 のレビュー指摘への対応

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| Devin: M・RC の種別を `&str` で持つと、パーサーの制約が型に残らない。列挙型にすれば比較の分岐漏れをコンパイラが検出できる（domain-primitive） | 直す | 非公開の `PreReleaseKind` を `pre_release_kind.rs` に足し、`ReleaseVersion` の種別をこの型にした。`satisfies_milestone` は4つの組を網羅する `match` になり、ワイルドカードはない。修飾子から種別を切り出す処理も `PreReleaseKind::split` に移した。単体テスト2件を足し、統合テストと判定の結果は変えていない |
| Codex: 指摘なし（`5be8022` のレビュー完了） | — | 対応不要 |
| CodeRabbit: 実行できる指摘なし。リスクは Minimal | — | 対応不要 |

## 製品編集前の実測

第1工程の成果 `632513c4454861e7f6b038f6fe12bf0a1d5eaa39`（`c17c22f` からの製品差分0。`git diff --stat c17c22f 632513c -- 'crates/*/src'` は空）で、担当が別の作業ツリーで次を再実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test version_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::version`（L24）。宣言8件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test fraktor_version_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::version`（L23）。宣言13件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典 `9a54d129` を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 2ノードとも `expected_compile_error`、`excluded: 0`。台帳はすでに red で、書き換えなし |

already_green ではない。製品に `kernel::version` モジュールがなく、2本ともコンパイルに失敗した。第2工程と自己レビューの修正では統合テストを変えていない（`git diff b3d9f23 -- crates/fraktor-actor/tests` は空）。

## 実装後の実測

PR #141 の指摘対応の後の HEAD で、担当が自分の作業ツリーで次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test version_spec -- --test-threads=1` | 0 | 8 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test fraktor_version_spec -- --test-threads=1` | 0 | 13 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib version` | 0 | 37 passed（第2工程の33件、自己レビューの修正の2件、指摘対応の2件） |
| `cargo test --workspace` | 0 | 64 スイート、602 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`。違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue139` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK で、対象ノードはすべて green。`G0issue139` は2件で `{'green': 2}`）。

## 再現手順

```text
cargo test -p fraktor-actor --test version_spec -- --test-threads=1
cargo test -p fraktor-actor --test fraktor_version_spec -- --test-threads=1
cargo test -p fraktor-actor --lib version
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue139
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue139
```
