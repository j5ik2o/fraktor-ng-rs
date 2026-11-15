# ゴール G0f：EventBus と EventStream の完成（EventBusSpec と EventStreamSpec）

状態: 定義済み・未発行（2026-11-14、窓口が準備した）。開始は 2026-11-15 にオーナーが承認した。着手は前提の G0e の完了後である。
根拠は、G0e のゴール文書（`docs/goals/g0e-become-and-supervision.md`）が「汎用 EventBusSpec と EventStreamSpec の移植」を G0e でやらないことに挙げ、後続のゴール G0f の見通しとしたことである。
G0e の実施記録を前提とする。

kind: foundation（基盤ゴール。同時に 1 本だけ走らせる）
requires: [G0e]（G0e の成果が main に入り、`rollup.py --goal G0a` から `--goal G0e` がすべて exit 0 になること。2026-11-15 時点で G0e は Ollama Cloud へ再開した run が実行中である）

## 原典プロトコール依存分析（2026-11-14、窓口が原典 Pekko v1.7.1 のスペック本体を読んで導出）

- EventBusSpec（概数 16）は、抽象スイートと 3 つの具体バスが 1 ファイルにあるスペックである。抽象スイートは 13 ケースで、subscribe と unsubscribe の成否、同じ購読者の二重登録と二重解除の拒否、複数の購読者、購読者がいない状態での publish、単一の購読者への単発と複数回の配送、意図した購読者だけへの配送、解除した購読者への非配送を検証する。具体バスは次の 3 つである。
  - LookupEventBus（LookupClassification）：イベントを 1 つの分類子へ対応づけ、分類子をキーに購読者を引く。
  - ScanningEventBus（ScanningClassification）：分類子を順序づけて走査し、述語（matches）で照合する。
  - ActorEventBus（ActorClassifier と ManagedActorClassification）：分類子は ActorRef で、配送は tell である。追加の 3 ケースを持ち、管理対象の購読者が終了したときの自動解除、分類子側の actor が死んでも購読を維持すること、すべての購読を解除した時点で watch を外すことを、system.eventStream の Debug ログの文面（「actor … has terminated, unsubscribing it from …」「unregistered watch of … in …」）まで含めて検証する。
- EventStreamSpec（17 ケース）は、EventStream（LoggingBus。EventBus に SubchannelClassification を重ねたもの）を検証する。中心は、クラスと trait の階層によるサブチャンネルの意味論である。親クラスや trait への購読が子の型のイベントへ届くことと、subscribe と unsubscribe と publish の各時点で分類の索引を更新することを、テスト自身が定義する階層（A、B2、B3、C と、T、AT、ATT、BT、BTT、CC、CCATBT）で検証する。ほかに、ログレベルの管理（setLogLevel と各レベルの配送の確認）、UnhandledMessage の Debug ログ（「unhandled message from …」）、TerminatedUnsubscriber の意味論（終了した購読者の自動解除、すでに終了した actor の購読の即時解除、二重初期化の拒否、購読解除時の unwatch、Debug ログの文面）、null の購読者の拒否がある。
- Rust への写像の要点であり、このゴールの設計の中心は、分類子のモデルである。Scala の実行時クラス階層（Class の isAssignableFrom と trait の継承）は Rust には無い。分類子は型で表し、上位型の関係は明示的に登録するモデルにする。テスト自身が階層を定義するため、テスト側が上位型の関係を宣言する形が原典の意味論に最も近い。実装の最初の段で、分類子とサブチャンネルのモデルの設計案を示してから実装へ進むこと。
- 移植済みの土台は、Logging の値型（`crates/fraktor-actor/src/kernel/actor/Logging/` の log_event、log_level、event_sink）、EventFilter と TestEventListener（G0d、testkit 側）、watch と Terminated（G0c）、TestProbe（G0c）、fishForMessage（G0c）である。EventBus と EventStream の本体、LoggingBus、サブチャンネルの索引は main にまだ無い（2026-11-14 に origin/main 4359cef で確認した）。
- G0f で新しく作るものは、EventBus の分類の仕組み一式（LookupClassification、ScanningClassification、SubchannelClassification、ActorEventBus と ManagedActorClassification）、EventStream（LoggingBus、ログレベルの管理、既定ロガーの起動、UnhandledMessage の扱い）、TerminatedUnsubscriber（終了の監視と自動解除と Debug ログ）、ActorSystem の eventStream の接合、testkit の within である。
- within について。EventStreamSpec が使う testkit の within は未実装である（origin/main の crates 全体に該当の実装は無いことを 2026-11-14 に確認した）。このゴールで `crates/fraktor-testkit` に実装し、`ledger/capabilities.yaml` の within の provided_by を G0f にする（G0c の expect-no-message、G0d の event-filter と同じ扱い）。
- 設定について。原典は設定テキストでロガーとログレベルと debug フラグを指定するが、設定体系（HOCON 相当）の再現範囲は未決定の設計判断であるため、このゴールでは設定テキストの解釈は作らない。EventStream の debug は原典でもコンストラクタの引数で切り替える面があり、プログラム側の指定で受ける。システムの既定ロガーの構成もプログラム側の指定に留める。この扱いは移植の忠実さの判断として、プルリクエスト本文に記録する。
- null の扱いについて（2026-11-15、オーナーが決定した）。EventStreamSpec の 2 ケースは、null の購読者を IllegalArgumentException で拒否することを検証する。Rust では null を None へ対応づける。購読者の入力は Option で表し、None の購読者は原典の IllegalArgumentException に相当する形で拒否する。2 ケースは None を渡して拒否を検証する形で移植し、除外しない。Option を受ける面の具体的な形は、実装の最初の段の設計案で示す。
- 台帳の depends_on_impl.modules は対象 2 ノードとも空である（seed 未抽出）。G0e の再開時の扱いと同じく、run の中で補完する。
- 処理順は EventBusSpec を先、EventStreamSpec を後とする。EventStream は EventBus の分類の仕組みの上に LoggingBus とサブチャンネルを重ねるため、依存の形で順序が決まる。

## 仕事は 1 段

1. **移植**：対象 2 ノードを Pekko v1.7.1 から移植して green にする（ワークフロー `fraktor-porting-ng`）

## 対象ノード（明示列挙、2 件）

| 処理順 | id | kind | ケース数（概数） | needs.testkit |
|---|---|---|---|---|
| 1 | `actor-tests/src/test/scala/org/apache/pekko/event/EventBusSpec.scala` | test | 16 | extends-pekko-spec, pekko-spec, test-actor, expect-msg, expect-no-message, test-probe, expect-terminated, fish-for-message |
| 2 | `actor-tests/src/test/scala/org/apache/pekko/event/EventStreamSpec.scala` | test | 17 | extends-pekko-spec, pekko-spec, test-actor, expect-msg, expect-no-message, test-probe, expect-terminated, within, await-assert, fish-for-message |

ノードの増減は人間だけが行う。ケース数は台帳の概数であり、実数は run が数えて台帳へ記録する（G0d の進め方と同じ）。EventBusSpec は抽象スイートを 3 つのバスで共有するファイル構成であり、数え方は run が実数で確定させる。

## 置き場所

- EventBus と EventStream と TerminatedUnsubscriber の実装は `crates/fraktor-actor/src` に置く
- テストは 2 ノードとも `crates/fraktor-actor/tests/` に置く（1 ノード 1 ファイル。actor-tests は主たる検証対象の actor のテストに置くという G0d の規則に従う）
- within は `crates/fraktor-testkit/src` に置く

## write_scope

- `crates/fraktor-actor/**`
- `crates/fraktor-testkit/**`
- ワークスペース直下の `Cargo.toml` と `Cargo.lock`
- `ledger/nodes.yaml`。対象 2 ノードの欄だけ
- `ledger/capabilities.yaml`。within の provided_by の欄だけ

範囲外の変更が要るときは、止めて報告する。

## 完了条件（機械判定できる形）

- AC1: `cargo test -p fraktor-actor` が通る
- AC2: `cargo test -p fraktor-testkit` が通る（G0a から G0e の退行がない）
- AC3: `uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0f` が 0 を返す
- AC4: 同じ rollup を `--goal G0a` から `--goal G0e` で実行し、すべて 0 を返す（退行なし）
- AC5: `uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0f` が 0 を返す
- AC6: 発行前処理のコミットより後の台帳の差分が、対象 2 ノードの欄と capabilities.yaml の within の欄に限られる
- AC7: 2 ノードの `cases.total` が実数で記録され、`rust.crate` と `rust.test_file` も記録されている
- AC8: `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all --check` が通る
- AC9: 成果はドラフト PR と 6 項目の完了報告で提出し、そこで止まる。次のゴールには着手しない

## やらないこと

- 設定体系（HOCON 相当）の再現と、設定テキストからのロガーやログレベルの読み取り
- typed の EventStreamSpec（actor-typed-tests の別ノード）の移植
- mailbox の上限の既定値とあふれ時の意味論（未決定の設計判断。ActorMailboxSpec はその決定後に別のゴールで行う）
- 台帳スキーマの変更。ノードを do_not_port にすること、ケースの除外、kind の変更（null の 2 ケースは上記のオーナー決定により None への対応づけで移植するため、除外の対象ではない）
- `ledger/tools/` の変更
- ドメインプリミティブ対応（プルリクエスト #56 はクローズ済み。対応は保留であり、このゴールでは扱わない）
- 旧 fraktor-rs からのコードの持ち込み

## provider

基盤ゴールなので強いモデルを使う（論点 19）。実効並行は 1 本。assignment は発行時の既定に従う。2026-11-15 時点の主チェックアウトの実効は interim-cloud-flash（Ollama Cloud の deepseek-v4.1-flash:cloud、作業ツリーで未コミット）である。

## 発行時の台帳前処理（窓口がゴール文書のプルリクエストとあわせて行う）

1. 対象 2 ノードの `goal: null` を `goal: G0f` に書き換える（台帳の差分は 2 行だけ）
2. `ledger/capabilities.yaml` の within の `provided_by: null` を `provided_by: G0f` に書き換える（差分は 1 行だけ）
3. 2026-11-14 に origin/main（4359cef）相当の台帳の一時コピーで確認済みである。前処理を行うと `rollup.py --goal G0f` は「ノード 2 件、unported 2」で exit 1 になり、`check_needs.py --goal G0f` は exit 0 になる。前処理なしの check_needs は、within が未充足で exit 1 になることも確認した。あわせて `--goal G0a` から `--goal G0d` の rollup はすべて exit 0 で、`--goal G0e` は実行中のため exit 1 である

## 前提と残るリスク

- run の作業ツリーへの Pekko 原典の配置は恒久策が入っている（プルリクエスト #28。`tools/pekko-source/prepare_pekko_source.sh` が作業ツリー直下の `pekko/` に v1.7.1〔commit 9a54d12〕を用意する）
- 対象は 2 ノード 33 ケース（概数）で、G0e（13 ケース）より大きい。イベント系の基盤をほぼ新規に作るゴールであり、分類子モデルの設計が規模を左右する
- EventStreamSpec の null の 2 ケースの扱いは決定済みである（上記のとおり、None への対応づけで移植する）

## run の後に人間が確認するコマンド

```sh
cargo test -p fraktor-actor
cargo test -p fraktor-testkit
for g in G0a G0b G0c G0d G0e G0f; do uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal $g; done
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0f
```
