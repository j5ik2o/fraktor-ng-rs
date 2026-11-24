# ゴール G0issue128：classic FrequencyListSpec

[Issue #128](https://github.com/j5ik2o/fraktor-ng-rs/issues/128)（Orca 項目140）の対象は、Pekko v1.7.1 の classic `FrequencyListSpec`。原典は「FrequencyList must」の下に3ケースを持つ。1ケースは頻度の昇順・降順と、同じ頻度の中での更新順、件数指定と skip 付きの両方向削除を確かめる。1ケースは1秒ずつ進める制御時計で全体の新旧順と窓による削除を確かめる。1ケースは dynamic aging の並びと削除結果を確かめる。 [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/FrequencyListSpec.scala) [原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/util/FrequencyList.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/FrequencyListSpec.scala` | `crates/fraktor-actor/tests/frequency_list_spec.rs` | 3 |

台帳は原典3件、移植3件、成功3件、除外0件。`needs` は空のままで、TestKit の機能は使わない。

## 実行経路

正規 TAKT の2工程で進めた。ゴールを発行する前に、開始点のブランチ `j5ik2o/issue-128-takt` で台帳の対象ノードの `goal:` 行だけを `G0issue128` にした（`5798643`、`ledger/README.md` のゴール発行手順）。そのうえでゴール `12812e6c-bba8-411d-b1cf-c4cc863d7128`（ブランチ `takt/20261124T0841-goal-12812e6c`、統合先 main）を作り、共有 takt-mcp（f6c0d505）から2工程を順に投入した。ワークフローはどちらも `simple`（親 simple、修正は子 review-remediation）、runtimeAssignment は既定の `opus-standard`。

| 工程 | workKey / タスク | run | 結果 |
|:---|:---|:---|:---|
| 第1工程（テストと台帳 red） | `issue-128/01-source-red` / `issue-128-1-frequencylistspec` | `20261124-084202-issue-128-1-frequencylistspec-8pvg2d` | completed（08:58:34Z）、ゴールブランチへ統合（`cfc3080`） |
| 第2工程（実装・等価性・台帳 green） | `issue-128/02-implementation-ledger` / `issue-128-2-frequencylist-wo-s` | `20261124-085940-issue-128-2-frequencylist-wo-s-vpjzao` | completed（09:54:50Z）、ゴールブランチへ統合（`6469f09`） |

step_start の provider / model の実測は、plan・review・review-adjudication・fix-plan・fix・fix-verifier・follow-up-review・supervise が claude-sdk / claude-opus-5-5、write_tests・implement が opencode / dgx/deepseek-v4.1-flash。codex や Spark への切替は起きていない。

- 第1工程の成果は、台帳の対象ノードで status・cases・rust を書いていた。cases の total・ported・passing は green の時点で書く欄（`.takt/facets/instructions/fraktor-ledger-update.md` L9）、red への遷移は status 行だけ（`ledger/tools/verify_red.py` の `mark_red`）、rust は port-test が記録する欄（`.takt/facets/instructions/fraktor-port-test.md` L32–34）である。TAKT が統合を自動で済ませたため、統合後のゴールブランチで cases を null に戻した（`7e68734`）。第1工程で台帳に残る差分は status `red` と rust の2欄で、製品差分は0。
- 第2工程の review は、計算量の記述と原典の行番号の誤りで REJECT した。担当が run に追加指示を送り、記述ではなく実装を直した（各値がバケット内の配置番号を持ち、削除を対数時間にした）。あわせて、同じキーの別の値で更新したときに保持している値を残すこと、時計なしの全体順の削除が panic することを単体テストで確かめた。

## 原典との照合

| 原典 | 移植 |
|:---|:---|
| Spec `TestClock`（1 tick = 1秒、`earlierTime(d) = currentTime - d.toSeconds`） | テスト内の `TestClock`。カーネルの単調時計 `Clock` を実装し、秒単位の `Duration` を返す。原典の `earlierTime` の引き算は移植ではリストが `Duration` から行う |
| Spec `check`（`size`、`leastToMostFrequent`、`mostToLeastFrequent`） | `check`（`len`、`least_to_most_frequent`、`most_to_least_frequent`） |
| Spec `checkRecency`（`size`、`overallLeastToMostRecent`、`overallMostToLeastRecent`） | `check_recency`（`len`、`overall_least_to_most_recent`、`overall_most_to_least_recent`） |
| Spec「track frequency of elements」 | `frequency_list_must_track_frequency_of_elements` |
| Spec「track overall recency of elements when enabled」 | `frequency_list_must_track_overall_recency_of_elements_when_enabled` |
| Spec「support dynamic aging with least frequently used eviction」 | `frequency_list_must_support_dynamic_aging_with_least_frequently_used_eviction` |

各ケースの操作は原典と同じ順序で並べ、原典と同じ位置で `check` / `checkRecency` する。削除の戻り値は要素と順序まで比べる。

原典の `check`/`checkRecency` の先頭行 `expected.forall(frequencyList.contains)` は結果を捨てており、何も検証していない。移植でも評価して結果を捨てる形にし、アサーションには変えていない。

## 実装

`fraktor_actor::kernel::frequency_list::FrequencyList` を追加した。時計には既存の `kernel::Clock` を使う。原典の「頻度バケットの双方向リスト + バケット内の値の双方向リスト + ハッシュマップ」は、#124・#126 と同じ考え方で置き換えた。頻度バケットは `u64` の id をキーにした `BTreeMap` に置き、`less`/`more` で双方向リストをつなぐ。バケット内の値は配置の通し番号をキーにした `BTreeMap` に置き、同じ頻度の中の更新順を持つ。値からノードを引く表も順序マップにした。そのため値には `Ord` と `Clone` が必要になる。各値はバケット内の配置番号を持ち、バケットへの配置、参照、バケットからの削除は、その番号と順序マップで1回ごとに対数時間になる。dynamic aging で移動先のバケットを探すときは、原典と同じくバケットを前へたどり、1歩ごとに対数時間がかかる。

バケットの挿入は、整列の規則にせず、原典の手順（実装 L195–241）と `DoubleLinkedList`（L69–93、L129–137）をそのまま写した。`getFirstOrElsePrepend`・`getNextOrElseInsert`・`findNextOrElseInsert` を、それぞれ `first_or_else_prepend`・`next_or_else_insert`・`find_next_or_else_insert` に対応させた。`increaseAdjustedFrequency` は今のバケットを起点に進め、新しい priority が今のバケット以下でも今のバケットの直後に入る。

更新回数・age・バケットの priority は、どれも原典では `Long` だが、移植ではそれぞれ専用の型 `Accesses`・`Age`・`Priority` にした（規約 domain-primitive、`.takt/workflows/rules/fraktor-coding-rules.md` L46–60。PR #129 のレビュー指摘）。priority は `Priority::of(accesses, aging)` だけで作り、dynamic aging がないときは更新回数そのもの、あるときは更新回数 + age になる。age は最小頻度の削除で `Age::of_removed(priority)` からだけ変わる。dynamic aging の有無は `Option<Age>` で持ち、無効のリストには age が存在しない。

age を更新するのは `remove_least_frequent` と `remove_least_frequent_skipping` だけにした。値ごとに、そのバケットの priority を削除の直前に age に設定する。`remove`・`remove_most_*`・`remove_overall_*` は age を変えない。

| 原典 | 移植 |
|:---|:---|
| `FrequencyList.empty(dynamicAging = false)` | `FrequencyList::empty(bool)` |
| `new FrequencyList[A](dynamicAging, OptionVal[Clock])` | `FrequencyList::new(bool, Option<Arc<dyn Clock>>)` |
| `size` / `contains` | `len`（と `is_empty`）/ `contains` |
| `update` | `update`。値が既にあれば、保持している値を残して位置だけ動かす |
| `remove` | `remove`（値で渡す） |
| `leastToMostFrequent` / `mostToLeastFrequent` | `least_to_most_frequent` / `most_to_least_frequent` |
| `overallLeastToMostRecent` / `overallMostToLeastRecent` | `overall_least_to_most_recent` / `overall_most_to_least_recent` |
| `removeLeastFrequent(n)` / `removeLeastFrequent()`（n=1） | `remove_least_frequent(n)`。`1` は明示的に渡す |
| `removeLeastFrequent(n, skip)` | `remove_least_frequent_skipping(n, &A)`。skip は件数ではなく値 |
| `removeMostFrequent(n)` / `removeMostFrequent()`（n=1） | `remove_most_frequent(n)` |
| `removeMostFrequent(n, skip)` | `remove_most_frequent_skipping(n, &A)` |
| `removeOverallLeastRecent(n)` | `remove_overall_least_recent(n)` |
| `removeOverallMostRecent(n)` | `remove_overall_most_recent(n)` |
| `removeOverallLeastRecentOutside(d)` | `remove_overall_least_recent_outside(Duration)` |
| `removeOverallMostRecentWithin(d)` | `remove_overall_most_recent_within(Duration)` |

## 等価性レビュー

### 独立レビュー（別系統のモデル）

- 実施者: 実装者とは別の、読み取り専用のレビュー担当。モデルは Codex（OpenAI GPT-6.2 Sol）。コーディネーターが実セッションの turn_context（2026-11-24T10:12:35.974Z）で `gpt-6.2-sol` を確かめた。第2工程の修正と TAKT のレビューが同じ claude-opus-5-5 だったため、`docs/decisions/takt-workflow-design.md` §2.6 に従って別系統のレビューを足した。
- 対象: PR #129 の固定 HEAD `cbe1edd34d116379360028973d4fc189fdad9636`。[移植テスト](https://github.com/j5ik2o/fraktor-ng-rs/blob/cbe1edd34d116379360028973d4fc189fdad9636/crates/fraktor-actor/tests/frequency_list_spec.rs)、[移植の実装](https://github.com/j5ik2o/fraktor-ng-rs/blob/cbe1edd34d116379360028973d4fc189fdad9636/crates/fraktor-actor/src/kernel/frequency_list/frequency_list_value.rs)。
- 原典: Pekko 固定 SHA `9a54d1294c3ac7821bbed3c940769c3d5027d1ff` の原典テスト、`FrequencyList.scala`、`DoubleLinkedList.scala`。
- 範囲: 原典と移植のテスト・実装の静的な照合。テストと台帳の再実行はレビュー担当の範囲外で、本書の実測を使う。全面的な API 互換性は対象外。
- 判定: 原典3ケースはすべて観測上等価。操作の取りこぼし、アサーションの弱体化、近似はない。`Accesses`・`Age`・`Priority` の型分けと、aging と skip の並びの回帰テストも原典と等価。除外0。
- 記述への指摘2件と修正状況（どちらも修正済み。コメント・rustdoc・本書の記述だけを変え、テストの操作・アサーションと実装は変えていない）:
  1. 移植テスト冒頭のコメントが、空の `check` は size だけを確かめると書いていた。原典の `check`（Spec L34–39）と移植（L77–80）は、空のときも両方向の順序を比べる。コメントをそれに合わせた。
  2. 型の rustdoc と本書が、更新した値は常に「元のバケットの直後」に入るように読めた。一般には今のバケットから前方へ探し、priority の小さい途中のバケットを通り過ぎる（原典 `FrequencyList.scala` L222–231、`DoubleLinkedList.scala` L79–90・L129–137）。「直後」は回帰テストの状況に限ると書き直した。

### 実装者自身のレビュー

実施者は第2工程の review と follow-up-review（claude-sdk / claude-opus-5-5）と、本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト・原典実装・`DoubleLinkedList` と、移植の実装・テストを突き合わせた。判定は「等価」。原典3ケースは観測上等価で、除外0。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| 新しい値は頻度1のバケットに入り、同じ頻度の中で最も新しい側に加わる | 実装 L193–200・L243–248 | ケース1の各 `check`、ケース3の通常・aging の初期 `check` |
| 保持している値の更新は頻度を1上げ、同じ頻度の中の最も新しい側へ移る。空になったバケットは消す | 実装 L234–252 | ケース1の `a`/`c` 更新後の `check` |
| 頻度の昇順と降順は、バケットの priority 順と、バケット内の更新順で決まる | 実装 L130–132・L267–269 | ケース1の全 `check`（`least_to_most_frequent`/`most_to_least_frequent`） |
| `removeLeastFrequent(n)` は頻度の低い側から n 件を削除して、その順で返す | 実装 L135–137・L254–257 | ケース1の `vec!["h", "i", "a"]`、ケース3の `vec!["x"]`・`vec!["x", "y"]`・`vec!["x", "y", "z"]` |
| `removeLeastFrequent(n, skip)` は値 `skip` を飛ばして n 件を削除する | 実装 L138–139 | ケース1の `remove_least_frequent_skipping(1, "j")` の `vec!["k"]`、`(2, "l")` の `vec!["j", "m"]` |
| `removeMostFrequent(n)` は頻度の高い側から n 件を削除して、その順で返す | 実装 L150–152・L254–257 | ケース1の `vec!["e", "c", "l"]`・`vec!["n"]`・`vec!["o", "r"]` |
| `removeMostFrequent(n, skip)` は値 `skip` を飛ばして n 件を削除する | 実装 L153–155 | ケース1の `remove_most_frequent_skipping(1, "o")` の `vec!["n"]`、`(2, "p")` の `vec!["o", "r"]` |
| 時計があるとき、更新ごとに時刻を刻み、全体の新しい側へ移す | 実装 L84–101 | ケース2の各 `check_recency` |
| `removeOverallLeastRecent(n)` は全体の古い側から削除してその順で返す | 実装 L170–173 | ケース2の `vec!["a"]` |
| `removeOverallMostRecent(n)` は全体の新しい側から削除してその順で返す | 実装 L175–178 | ケース2の `vec!["k"]` |
| `removeOverallLeastRecentOutside(d)` は `timestamp < now - d` の値を古い順に消す。境界の値は残る | 実装 L181–184 | ケース2の `vec!["c", "e", "h"]`、単体テスト `a_value_updated_exactly_at_the_boundary_stays_for_both_windows` |
| `removeOverallMostRecentWithin(d)` は `timestamp > now - d` の値を新しい順に消す。境界の値は残る | 実装 L186–190 | ケース2の `vec!["m", "l"]`、単体テスト `a_value_updated_exactly_at_the_boundary_stays_for_both_windows` |
| dynamic aging では、新しい値の priority は `1 + age`、更新した値の priority は `accesses + age` になり、バケットの位置は今の位置から進めて決める | 実装 L202–232 | ケース3の aging 側の各 `check` |
| dynamic aging では、最小頻度の削除の直前に、削除する値のバケットの priority を age にする | 実装 L262–265 | ケース3の aging 側の各削除後の `check`（age が 0→1→3→7→9 と変わる並び） |

**境界の扱い（第1工程から持ち越した確認事項）。** 原典の `removeOverallLeastRecentOutside` は `timestamp < now - d`（L183）、`removeOverallMostRecentWithin` は `timestamp > now - d`（L189）で、どちらも strict である。原典のテストデータには境界ちょうどの時刻に更新した値がないため、原典3ケースでは strict と non-strict を区別できない。移植では同じ向き（`<` と `>`, `recency_list_value.rs:134`／`:149` と同じ形）にし、境界ちょうどの値が両方向とも残ることを単体テスト `frequency_list_value_test.rs::a_value_updated_exactly_at_the_boundary_stays_for_both_windows` で直接確かめている。

**dynamic aging と skip の組み合わせ（PR #129 のレビュー指摘）。** 原典のバケットの並びは、dynamic aging のもとで常に priority の昇順になるとは限らない。skip 付きの削除で priority の高い値を先に消すと age が上がり、そのあと skip した値を消すと age が下がる。age が下がったあとに更新した値は、今のバケットより低い priority になることがある。原典の `increaseAdjustedFrequency`（実装 L222–231）が呼ぶ `findNextOrElseInsert`（`DoubleLinkedList.scala` L79–90・L129–137）は、今のバケットから前方へだけ探す。次のバケットの priority が新しい priority より小さいあいだはそれを通り過ぎ、次のバケットが等しければそこに入り、そうでなければ最後に通ったバケットの後ろに新しいバケットを挿入する。後ろへは戻らない。そのため値は、今のバケットより低い priority でも、今のバケットより後ろに入る。回帰テストの状況では今のバケットの後ろに別のバケットがないので、値は今のバケットの直後に入る。このため、次の最小頻度の削除は、priority がより高い前方の値から消す。移植は原典と同じ並びを保つ。型の rustdoc と `remove_least_frequent_skipping` の rustdoc にこの例外を書き、単体テスト `an_age_lowered_after_a_skipping_removal_keeps_pekkos_bucket_order` で確かめている。

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典の時計は `earlierTime` を自分の単位で計算する。移植ではリストが `Clock::monotonic()` の `Duration` から窓を引く。秒単位の時計では、境界は原典と同じ時刻になる。
- 時計がないときの `removeOverall*` は、原典の `UnsupportedOperationException` の代わりに `panic!("overall recency is not enabled")` する。
- 参照はハッシュマップではなく順序マップにした。そのため値に `Ord` と `Clone` が必要になり、値1件の配置・参照・削除は対数時間になる（原典は定数時間）。
- 配置とバケットの通し番号は `u64` で、更新・配置のたびに1つ進む。原典にはこの上限がない。2^64 回の配置に達するほどの極端な長寿命は、原典3ケースの範囲外の既知の差異とする。
- skip を値への参照で受け取り、`_skipping` という名前で分けた。原典は同じ `removeLeastFrequent`/`removeMostFrequent` のオーバーロードで、件数 `n` と値 `skip` を取る。
- 原典の `leastFrequent`、`mostFrequent`、`withOverallRecency.empty`（`NanoClock`）は、このノードの契約（テスト）が使わないため移植していない。#124・#126 と同じ判断である。
- 原典の `n == 1` の最適化経路（L135、L151）は写していない。結果は一般の経路と同じである。

## 製品編集前の実測

第1工程の統合後、台帳を絞った `7e68734e48b84196e1b41abdd941ee8f0339698a`（`5798643` からの製品差分0、テストの blob `9694b8d140c2266ae793654a0772b9aad08e9413`）で次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test frequency_list_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::frequency_list`（`tests/frequency_list_spec.rs:50`）の1件。宣言3件、実行0件、除外0件 |
| `rollup.py --goal G0issue128` | — | ノード1件、`{'red': 1}` |

## 実装後の実測

第2工程の統合後の `6469f09`（テストの blob は `9694b8d140c2266ae793654a0772b9aad08e9413` のまま）で次を実行した。独立レビューの指摘1で移植テスト冒頭のコメントだけを直したため、テストの blob は `05f39314114d9dc7a120103bd1bfb1066bda4f5e` に変わった。`9694b8d1` との差分はコメント3行で、テストの操作とアサーションは同じである。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test frequency_list_spec -- --test-threads=1` | 0 | 3 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib frequency_list` | 0 | 6 passed（境界1件、保持値1件、時計なしの panic 4件）。PR #129 のレビュー対応後は 7 passed（aging と skip の並び1件を追加） |
| `cargo test --workspace` | 0 | 55 スイート、459 passed / 0 failed / 0 ignored。レビュー対応後は 460 passed |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` | 0 | `clean: true`、151 files、違反0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |

台帳は `G0issue128` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs 閉性 OK、全対象ノード green。`G0issue128` は1件で `{'green': 1}`）。

## 再現手順

```text
cargo test -p fraktor-actor --test frequency_list_spec -- --test-threads=1
cargo test -p fraktor-actor --lib frequency_list
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue128
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue128
```
