# ゴール G0issue126：classic SegmentedRecencyListSpec

[Issue #126](https://github.com/j5ik2o/fraktor-ng-rs/issues/126)（Orca 項目123）の対象は、Pekko v1.7.1 の classic `SegmentedRecencyListSpec`。原典は「SegmentedRecencyList must」の下に4ケースを持つ。3ケースは時計なしで段の昇格・降格と上限超過の削除を確かめ、1ケースは1秒ずつ進める制御時計で全段横断の期限切れ削除を確かめる。 [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/SegmentedRecencyListSpec.scala#L29-L157) [原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/util/SegmentedRecencyList.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/SegmentedRecencyListSpec.scala` | `crates/fraktor-actor/tests/segmented_recency_list_spec.rs` | 4 |

台帳は原典4件、移植4件、成功4件、除外0件。`needs` は空のままで、TestKit の機能は使わない。

## 実行経路

正規 TAKT の2工程で進める予定だった。ゴール `12612e6c-bba8-411d-b1cf-c4cc863d7126` を作り、第1工程のタスク `issue-126-1-segmentedrecencyli`（workKey `issue-126/01-source-red`、ワークフロー `builtins/ja/workflows/simple.yaml`、runtimeAssignment は既定の `opus-standard`）を共有 takt-mcp（f6c0d505）から投入した。run は `20261124-071620-issue-126-1-segmentedrecencyli-gwr0gl`。

| 工程 | step_start | provider / model |
|:---|:---|:---|
| plan | 2026-11-24T07:16:21.003Z | claude-sdk / claude-opus-5-5 |
| write_tests | 2026-11-24T07:20:35.470Z | opencode / dgx/deepseek-v4.1-flash |

write_tests の作業フェーズは 07:41:16Z に終わり、テスト1ファイルと製品編集前の実測を残した。続くレポートフェーズは 07:41:17Z に始まったあと、25分間ログ・レポート・作業ツリーに何も書かず、OpenCode への要求も出さなかった。前進がないと判断し、成果と run の記録を保全してから、そのタスクだけを `takt_stop_goal_task` で止めた（08:07:14Z に要求、08:07:17Z に実行プロセスが終了し、タスクは failed、run は aborted）。再投入はしていない。そのあとは同じ担当（Claude Opus 5.5）が直接実装した。TAKT が作ったテストファイルをそのまま使い、テストを先に置いて製品編集前の結果を採るという工程の順序は守った。第2工程は投入していない。

### 手順上の例外（PR #127 のレビューで指摘）

- 台帳の状態遷移の規約（`.takt/facets/knowledge/fraktor-porting-rules.md` L12–13）は `unported → red → green` の順を求める。このノードの台帳は、`2c7be2c` で `unported` から直接 `green` になった。製品編集前の compile-red は `b2f336b` で実測し本書に記録したが、台帳の `status: red` は書いていない。verify-red を通る TAKT の経路を途中で離れたためである。
- ゴール発行手順（`ledger/README.md` L28–33）は、発行前に対象ノードの `goal:` 行だけを変更してタグ付けすることを求める。`G0issue126` のタグは、`green` と同じ `2c7be2c` で初めて台帳に入った。TAKT ゴール `12612e6c-bba8-411d-b1cf-c4cc863d7126` の発行前に台帳のタグ付けをしていなかった。
- 既に push した履歴は書き換えず、上の2点を例外としてここに記録する。最終状態（`goal: G0issue126`、`status: green`、原典4・移植4・成功4・除外0）は規約どおりで、`check_needs.py` と `rollup.py` で確かめている。
- 等価性レビューの手順（`docs/decisions/takt-workflow-design.md` §2.6）は、実装者と別系統のモデルによるレビューを求める。下の「等価性レビュー」は実装者自身のレビューである。別系統のレビューは、修正の push 後に読み取り専用のレビューとして別に行い、その結果をここに追記する。

## 原典との照合

| 原典 | 移植 |
|:---|:---|
| Spec L24–29 `TestClock`（1 tick = 1秒、`earlierTime(d) = currentTime - d.toSeconds`） | テスト内の `TestClock`。カーネルの単調時計 `Clock` を実装し、秒単位の `Duration` を返す |
| Spec L34–41 `check`（全体の `size`、段ごとの `leastToMostRecentOf` と `sizeOf`） | `check`（`len`、`least_to_most_recent_of`、`len_of`） |
| Spec L46–74 2段 limits (2, 3) | `segmented_recency_list_must_track_recency_of_elements_across_2_segments` |
| Spec L76–107 4段 limits (4, 3, 2, 1) | `segmented_recency_list_must_track_recency_of_elements_across_4_segments` |
| Spec L109–129 limits (2, 8) と `updateLimits(1, 4)` | `segmented_recency_list_must_allow_protected_limits_to_be_updated` |
| Spec L131–156 limits (5, 5)、制御時計、`removeOverallLeastRecentOutside(2秒)` | `segmented_recency_list_must_remove_overall_least_recent_elements` |

各ケースの操作は原典と同じ順序で並べ、原典と同じ位置で `check` する。削除の戻り値は要素と順序まで比べる。

原典の `check` の先頭行 `expectedSegment.forall(recency.contains)` は結果を捨てており、何も検証していない。移植でも評価して結果を捨てる形にし、アサーションには変えていない。

## 実装

`fraktor_actor::kernel::segmented_recency_list::SegmentedRecencyList` を追加した。時計には既存の `kernel::Clock` を使う。原典の双方向リストとハッシュマップの組は、#124 の `RecencyList` と同じ考え方で置き換えた。段ごとと全段横断の順序は配置の通し番号をキーにした順序マップで持ち、値から位置を引く表も順序マップにした。そのため値には `Ord` と `Clone` が必要になる。配置・参照・削除の1回は対数時間で、`k` 件を降格または削除する操作は、原典の `O(k)` に対して `O(k log n)` になる。

段の上限は、生成時と `update_limits` の両方で専用の型 `SegmentLimits` に変換する。この型は、段が1つ以上あることを確かめ、合計を `usize::MAX` で飽和させて持つ。`[usize::MAX, 1]` のような上限でも合計が溢れず、リストはどの値も上限超過として扱わない。公開の引数は `&[usize]` のままにした。原典相当のテストが `empty(&[2, 3])` と `update_limits(&[1, 4])` の形で呼ぶためである。

| 原典 | 移植 |
|:---|:---|
| `SegmentedRecencyList.empty(limits)` | `SegmentedRecencyList::empty(&[usize])` |
| `new SegmentedRecencyList(initialLimits, OptionVal[Clock])` | `SegmentedRecencyList::new(&[usize], Option<Arc<dyn Clock>>)` |
| `size` / `sizeOf(level)` / `contains` | `len`（と `is_empty`）/ `len_of` / `contains` |
| `update` | `update`。値が既にあれば、保持している値を残して位置だけ動かす |
| `leastToMostRecentOf(level)` | `least_to_most_recent_of` |
| `removeLeastRecentOverLimit()` | `remove_least_recent_over_limit` |
| `removeOverallLeastRecentOutside(d)` | `remove_overall_least_recent_outside` |
| `updateLimits(newLimits)` | `update_limits` |

## 等価性レビュー

実施者は本 issue の担当（Claude Opus 5.5、Orca 項目123 の担当席）。根拠は、Pekko 固定 SHA の原典テストと原典実装、移植の実装とテストの4つを突き合わせた結果である。判定は「等価」。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| 新しい値は最下段の新しい側に加わる | 実装 L83–88・L141–148 | 各ケースの最初の `update` 後の `check` |
| 保持している値の更新は1段上へ昇格し、昇格先が上限を超えれば、その段の古い側から超過分を1段下へ降格する | 実装 L150–163・L136–139・L165–169 | 2段・4段ケースの各 `check` |
| 最上段の値の更新は、その段の新しい側へ移るだけ | 実装 L151–152 | 単体テスト `updating_a_value_in_the_highest_segment_moves_it_to_the_most_recent_end` |
| 上限超過の削除は、まず上の段から順に各段の超過分を降格し、全体の超過分を最下段の古い側から返す。超過がなければ空 | 実装 L108–115・L133–134 | 2段ケースの `vec!["a"]`・`vec!["e"]`・空・`vec!["a", "e"]`、4段ケースの空・`vec!["a", "b"]`・`vec!["c", "e"]`・`vec!["g", "b", "d"]` |
| `updateLimits` は次の削除から効く | 実装 L128–131 | limits (1, 4) への変更後に `vec!["c", "d", "e", "f", "g"]` が返り、`h` / `i, j, k, l` が残る |
| 時計があるとき、更新ごとに時刻を刻み、全段横断の順序の新しい側へ移す。`timestamp < now - d` の値を全段から古い順に消す。境界の時刻の値は残る | 実装 L122–126・L141–148・L159–162 | 時刻5で `a`〜`g` を返し、時刻3・4に更新した値が残る |
| 時計がないときの全段横断の削除は例外 | 実装 L123 | 単体テスト `removing_by_overall_recency_without_a_clock_panics` |
| 窓が時計の起点より前まで届くとき、何も消さない（負の時刻との比較） | 実装 L124–125 | 単体テスト `a_window_reaching_before_the_epoch_keeps_every_value` |
| 段の上限の合計は表現できる最大値で飽和する（原典の `Int` の合計に対応する、移植固有の境界） | 実装 L56・L128–131 | 単体テスト `limits_whose_total_exceeds_the_address_space_keep_every_value` |
| 同じキーと比較される別の値で更新しても、保持しているノードの値が残る | 実装 L84–85（既存ノードを `promote`） | 単体テスト `updating_a_held_value_keeps_the_value_first_added` |

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典の時計は `earlierTime` を自分の単位で計算する。移植ではリストが `Clock::monotonic()` の `Duration` から窓を引く。秒単位の時計では、境界は原典と同じ時刻になる。
- 時計がないときの全段横断の削除は、原典の `UnsupportedOperationException` の代わりに panic する。
- 段の数は生成時に決まる。原典は空の `limits` でも生成でき、最初の `update` で添字の例外になる。移植は生成時に panic する。`updateLimits` に段の数と違う長さを渡したとき、原典は短ければ後の削除で添字の例外になり、長ければ余りを合計上限にだけ足す。移植はどちらも呼んだ時点で panic する。どちらも原典の内部利用者が渡さない入力である。
- 原典の `remove`、`leastRecent`、公開の `removeLeastRecent`、`NanoClock` を使う `withOverallRecency.empty` は、このノードの契約に含まれないため移植していない。

## 製品編集前の実測

テストだけのコミット `b2f336b3a4ec665656befa863c9fa5455dde8b88`（起点 `155e8e4dde8eb1364fe8aad18df480ed6058be4e` からテスト1ファイルを追加したのみ、製品・台帳の差分0、テストの blob `b72b1dec40618788172de15ea500d09fb53ee734`）で次を実行した。TAKT の write_tests も同じ blob で同じ結果を記録している。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test segmented_recency_list_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::segmented_recency_list` の1件。宣言4件、実行0件、除外0件 |

## 実装後の実測

テストの blob は `b72b1dec40618788172de15ea500d09fb53ee734` のまま変えていない。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test segmented_recency_list_spec -- --test-threads=1` | 0 | 4 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib recency_list` | 0 | 10 passed（既存の RecencyList 4件と、新しい単体テスト6件。レビュー対応後） |
| `cargo test --workspace` | 0 | 54 スイート、レビュー対応後 450 passed / 0 failed / 0 ignored（対応前 449） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` | 0 | `clean: true`、147 files、違反0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |

台帳は `G0issue126` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs 閉性 OK、全対象ノード green）。

## 再現手順

```text
cargo test -p fraktor-actor --test segmented_recency_list_spec -- --test-threads=1
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue126
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue126
```
