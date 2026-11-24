# ゴール G0issue124：classic RecencyListSpec

[Issue #124](https://github.com/j5ik2o/fraktor-ng-rs/issues/124)（Orca 項目114）の対象は、Pekko v1.7.1 の classic `RecencyListSpec`。原典は、1秒ずつ進める制御時計で1ケース「RecencyList must track recency of elements」を実行する。 [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/util/RecencyListSpec.scala#L21-L128) [原典の実装](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/util/RecencyList.scala)

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/util/RecencyListSpec.scala` | `crates/fraktor-actor/tests/recency_list_spec.rs` | 1 |

台帳は原典1件、移植1件、成功1件、除外0件。`needs` は空のままで、TestKit の機能は使わない。SegmentedRecencyList は別ノードとして扱い、ここでは触れない。

## 実行経路

正規 TAKT の2工程で進める予定だったが、第1工程のタスク `issue-124-1-recencylistspec-no` は開始前に失敗した。エラーは `Workflow bundle could not resolve call "simple/remediation"` で、run ディレクトリもステップログも作られていない。daemon が読む `~/.takt/config.yaml` が `enable_builtin_workflows: false` のため、builtin の `review-remediation` を解決できなかった。共有設定は変更していない。担当の許可に従い、同じ担当が直接実装に切り替えた。そのあとも、テストを先に置いて製品編集前の結果を採るという工程の順序は守った。失敗したタスクは保全し、再投入していない。

## 原典との照合

| 原典 | 移植 |
|:---|:---|
| L24–29 `TestClock`（1 tick = 1秒、`earlierTime(d) = currentTime - d.toSeconds`） | テスト内の `TestClock`。カーネルの単調時計 `Clock` を実装し、秒単位の `Duration` を返す |
| L34–39 `check`（`size`、古い順、新しい順） | `check`（`len`、`least_to_most_recent`、`most_to_least_recent`） |
| L43–128 時刻1〜20の操作列 | 同じ順序の同じ操作。各段で同じ `check` を行い、削除の戻り値は要素と順序を比較する |

原典との差異は次の3点で、観測する値は変わらない。

- 原典の時計は `earlierTime` を自分の単位で計算する。移植ではリストが `Clock::monotonic()` の `Duration` から窓を引く。秒単位の時計では、境界は原典と同じ時刻になる。窓が時計の起点より前まで届く場合、古い側の削除は何も消さず、新しい側の削除はすべて消す。原典の負の時刻の比較と同じ結果になり、この点は単体テストで確かめた。
- Rust には既定引数がないため、`removeLeastRecent(n, skip)` と `removeMostRecent(n, skip)` は `_skipping` の名前で分けた。
- 原典の `check` の先頭行 `expectedLeastToMostRecent.forall(recencyList.contains)` は結果を捨てており、何も検証していない。移植でも評価して結果を捨てる形にし、アサーションには変えていない。

時間窓は、時刻12の `remove_least_recent_outside(3秒)` が `h`、`j` の順に返し、時刻9の `k` が残る。時刻14の `remove_most_recent_within(3秒)` は `r`、`k`、`q`、`p` の順に返し、時刻11の `n`、`o` が残る。どちらの方向でも、境界の時刻そのものの要素は残る。

## 実装

`fraktor_actor::kernel::recency_list::RecencyList` を追加した。時計には既存の `kernel::Clock` をそのまま使い、新しい時計の型は作っていない。カーネルにはハッシュマップがないため、原典の双方向リストとハッシュマップの組を、更新の通し番号をキーにした順序マップと、値から番号を引く順序マップの組に置き換えた。そのため値には `Ord` と `Clone` が必要で、各操作は対数時間になる。原典の `leastRecent` / `mostRecent`、`NanoClock` を使う `empty` は、このノードの契約に含まれないため移植していない。

## 等価性レビュー

実施者は本 issue の担当（Claude Opus 5.5、Orca 項目114 の担当席）で、PR #125 のレビュー指摘への対応として行った。根拠は、Pekko 固定 SHA の原典テストと原典実装、修正後のテストの3つを突き合わせた結果である。判定は「等価」。製品表面の差異は、上の「原典との照合」に挙げた3点だけである。

| 原典の観測 | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| 空の状態で件数0、両方向の列挙が空 | Spec L47 | `recency_list_spec.rs` の最初の `check` |
| 新規の更新は新しい側の端に加わる。同じ時刻内では更新した順に並ぶ | Spec L49–56・L61–63・L105–107、実装 L65–76 | 時刻1・2・4・15の `check` |
| 登録済みの値を更新すると、新しい側の端へ移る。保持していた値そのものは残り、時刻と位置だけが変わる | Spec L57–60・L65–68・L73–76・L81–84・L97–100、実装 L66–69（既存ノードの `timestamp` と `moveToBack` だけを更新） | 時刻3・5・7・9・13の `check`。値の保持は単体テスト `updating_a_held_value_keeps_the_value_first_added` |
| 個別削除。保持していない値の削除は何もしない | Spec L69–72、実装 L79–84 | 時刻6の `check`。保持していない値は単体テスト `removing_a_value_the_list_does_not_hold_changes_nothing` |
| 件数指定の削除は、端から順に返す。件数が残数を超えれば全部を返す | Spec L77–80・L85–88・L117–120・L125–128、実装 L102–104・L114–116 | 時刻8・10・18・20の戻り値と `check` |
| skip つきの削除は、端から skip 件を飛ばして返す | Spec L109–116、実装 L106–107・L118–119 | 時刻16・17の戻り値と `check` |
| `removeLeastRecentOutside` は `timestamp < now - d` の値を古い側から消す。境界の時刻の値は残る | Spec L93–96、実装 L126–129 | 時刻12で `h`、`j` を返し、時刻9の `k` が残る |
| `removeMostRecentWithin` は `timestamp > now - d` の値を新しい側から消す。境界の時刻の値は残る | Spec L101–104、実装 L131–134 | 時刻14で `r`、`k`、`q`、`p` を返し、時刻11の `n`、`o` が残る |
| 窓が時計の起点より前まで届くとき、古い側の削除は何も消さず、新しい側の削除はすべて消す（負の時刻の比較） | 実装 L127・L132 | 単体テスト `a_window_reaching_before_the_epoch_*` の2件 |

PR #125 のレビューで、同じキーと比較される別の値で更新すると、保持していた値が置き換わる差異が見つかった。修正前に単体テストをコミット `6704618faad2deac3ac28f7ed8bead5fb6d5a364` へ入れて再現した（`cargo test -p fraktor-actor --lib recency_list -- --test-threads=1` が終了値101、`left: ["other", "new"]`、`right: ["other", "old"]`）。修正後は、保持している値を残して時刻と位置だけを更新する。原典相当テストは文字列だけを使うため、この差異を観測できなかった。テストファイルは変えていない。

## 製品編集前の実測

テストだけのコミット `ecbb11499621a0e1c35feac400d105cdc3ce697b`（起点 `d5d518abb1b3bd0b9f3b9e2c7488e254f3c64a43` からテスト1ファイルを追加したのみ、製品・台帳の差分0、テストの blob `6beaa7a764e55e882ac794285b74aec3d952e8ec`）で次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test recency_list_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved import fraktor_actor::kernel::recency_list` の1件。宣言1件、実行0件、除外0件 |

## 実装後の実測

テストの blob は `6beaa7a764e55e882ac794285b74aec3d952e8ec` のまま変えていない。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test recency_list_spec -- --test-threads=1` | 0 | 1 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib recency_list -- --test-threads=1` | 0 | 単体テスト4件成功（レビュー修正後） |
| `cargo test --workspace` | 0 | 53 スイート、レビュー修正後 440 passed / 0 failed / 0 ignored（修正前 439） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| `cargo run --quiet --release --locked --manifest-path tools/syn-check/Cargo.toml -- --format json crates/fraktor-actor` | 0 | `clean: true`、143 files、違反0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |

台帳は `G0issue124` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs 閉性 OK、全対象ノード green）。

## 再現手順

```text
cargo test -p fraktor-actor --test recency_list_spec -- --test-threads=1
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue124
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue124
```
