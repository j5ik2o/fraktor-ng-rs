# ゴール G0issue195：actor の DeployerSpec の移植

## 目的

Pekko v2.0.0 の `actor-tests` の `DeployerSpec`（16件）を Rust へ移植し、対応する本体（`Deploy`、`Scope`、`Deployer` と型付きの入力 `DeploymentSection`）を `fraktor-actor` の kernel に足して、台帳の対象ノードを green にする。

設計は [docs/design/issue-195-deployer.md](../design/issue-195-deployer.md) にあり、独立レビュー（要修正、高1件・中7件・低5件）、再レビュー（要修正、中3件・低5件）、再々レビュー（残り2点）を通り、中の3点が原典どおりに直ったことを確かめて承認された。狭めと判断の11件は中将（2026-11-26）が推奨どおりに決めた。ただし Java の `split` の挙動（下記）は、元帥経由でオーナーの判断待ちである。

## 対象ノード

| id | Rust の統合テスト | 原典のケース数 |
|:---|:---|---:|
| `actor-tests/src/test/scala/org/apache/pekko/actor/DeployerSpec.scala` | `crates/fraktor-actor/tests/deployer_spec.rs` | 16 |

16件すべてを移植した。除外は0件である。

## 移植の忠実さの判断

- F1（ケース5）：原典は HOCON の値の型を Typesafe Config が検査し `ConfigException.WrongType` を投げることを見る。移植は `NumberOfInstances::NotANumber` という未解釈の値を解釈の段で誤りにする形に写した。誤りになるのは件数を読む種類（全プール）だけであり、数の範囲は原典の `getInt` に合わせた（`i32`、範囲外は `NotANumber` で渡す）。
- F3（ケース16）：原典は短い名前から JVM のクラス名への対応表を見る。移植は #192 の `ProviderSelection` と同じ形（`RouterSelection::from_setting` と `fqcn`、定数の FQCN）に写した。受ける FQCN は14件であり、NoRouter の FQCN は原典どおり生成の段で誤りになる。
- 原典より強い確認：ケース9〜12の件数、ケース11の `within`、ケース14の `within`（既定の5秒）は、原典の `assertRouting` が比べない値まで確かめている。テストのコメントに明記した。

## 判断待ちの項目

- Java の `split` の挙動（パスの分け方）：中将が写す側を推奨し、元帥経由でオーナーの判断待ち（2026-11-26）。本ゴールでは写す形で実装したが、承認済みではない。台帳にもテストにも承認済みとは書いていない。判断が狭める側に決まれば、分け方と期待値を直す。

## 既知の差

- 台帳の `needs.testkit` は `extends-pekko-spec`／`pekko-spec` のままである。原典が PekkoSpec から使うのはシステムの保持だけであり、移植は testkit を使わず `Deployer` の値を直接作って引く形にした。実態との差としてここに記録する。
- `PoolKind` に `Balancing`、`ConsistentHashing`、`TailChopping` を足した。#194 の `actor_configuration_verification_property_test.rs` の表にもこの3種を足した。規則は dispatcher の種類についてのものであり、原典の Spec にプールの種類ごとの期待値の違いは無いことを確かめた。consistent-hashing の振り分けは #269 の申し送りのままである。
