# ゴール G0issue142：classic の DNS ワイヤ形式の2スイート

[Issue #142](https://github.com/j5ik2o/fraktor-ng-rs/issues/142) と [Issue #143](https://github.com/j5ik2o/fraktor-ng-rs/issues/143)（Orca 項目220）の対象は、Pekko v1.7.1 の次の2つのテストである。どちらもメモリ上の DNS メッセージを読み、ドメイン名のデコードを共有する。2つのノードを1つのゴールにまとめた。

- `DomainNameSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/DomainNameSpec.scala)。通常の名前、後方圧縮参照、自己参照、参照ループ、予約ラベル種別、途中で終わる名前の6ケース
- `MessageSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/MessageSpec.scala)。TC フラグ付きで質問の直後に切れた応答の1ケース
- 原典の実装: [DomainName.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/DomainName.scala)、[DnsMessage.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/DnsMessage.scala)（`MessageFlags`、`Message`）、[Question.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/Question.scala)、[RecordTypeSerializer.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/RecordTypeSerializer.scala)、[RecordClassSerializer.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/RecordClassSerializer.scala)、[RecordType.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/RecordType.scala)、[RecordClass.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/RecordClass.scala)、[DnsResourceRecords.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/DnsResourceRecords.scala)（`ResourceRecord.parse`）

Issue #143 の「原典実装」のリンク先 `internal/Message.scala` は、固定 SHA に存在しない。`Message` と `MessageFlags` は `internal/DnsMessage.scala` にあるので、そちらを原典とした。

範囲はメモリ上のバイト列の読み取りだけである。ネットワーク、ソケット、resolver、TCP への切替、メッセージの書き出し（`write`）、レコードデータの型ごとのデコード（`ARecord` など）は対象外。

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/DomainNameSpec.scala` | `crates/fraktor-actor/tests/domain_name_spec.rs` | 6 | 6 | 6 | 0 |
| `actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/MessageSpec.scala` | `crates/fraktor-actor/tests/message_spec.rs` | 1 | 1 | 1 | 0 |

合計は原典7件・移植7件・成功7件・除外0件。`needs` はどちらも空で、TestKit の機能は使わない。原典件数は `" in {"` の数で数えた。`ignore` と `pending` はない。

## 実行経路

TAKT を使わず、担当（Claude Opus 5.5）が直接実装した。TAKT にはゴールもタスクも投入していない（workKey・タスク名・実行 ID はない）。

- 着手時に、共有 TAKT（`7e95bcf16`）のゴール統合に回帰の疑いがあり、ゴール統合を伴う新規投入、ゴールの再開・再試行を保留する通知が出た（2026-11-25）。
- 担当から投入0件を報告し、調整担当の指示で直接経路に切り替えた。理由は、保留の解除時期が未定であることと、`takt-operations` の「最初から直接指示にする仕事」（1件で完結し受け入れ条件がはっきりしている、Opus 1名で半日以内）に当たることである。
- 保留が解除されても、同じ仕事を TAKT へ再投入しない。

工程は「テスト先行・red の実測」「実装・green・回帰」の2段で、どちらも同じ担当が直列に行った。TAKT の同時実行上限は変えていない。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `be01a47` | 2ノードの `goal: G0issue142`（台帳2行） |
| テストと台帳 red | `f8c4d03` | 移植テスト2本、2ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green | `e2aa776` | kernel の `io::dns` に8型、単体テスト53件（レビュー対応で2型・6件を追加）、2ノードの green。統合テストは不変 |
| ゴール文書 | 本書のコミット | 本書 |

## 原典との照合

### API の対応

| 原典 | 移植 |
|:---|:---|
| `DomainName.parse(it: ByteIterator, msg: ByteString): String` | `DomainName::parse(&mut WireReader) -> Result<DomainName, WireFormatError>`（`as_str()` で文字列） |
| `bytes.iterator`、`bytes.iterator.drop(13)` | `WireReader::new(&bytes)`、`WireReader::starting_at(&bytes, 13)` |
| `Message.parse(msg: ByteString): Message` | `Message::parse(&[u8]) -> Result<Message, WireFormatError>` |
| `msg.id`（`Short`）、`msg.flags.isTruncated` | `id() -> u16`、`flags().is_truncated()` |
| `msg.questions`、`answerRecs`、`authorityRecs`、`additionalRecs` | `questions()`、`answer_records()`、`authority_records()`、`additional_records()`（スライス） |
| `Question(name, qType, qClass)` | `Question`（`name()`、`record_type()`、`record_class()`） |
| `RecordType.AAAA`、`RecordClass.IN` | `RecordType::AAAA`、`RecordClass::IN` |
| 例外 `IllegalArgumentException`、`NoSuchElementException`、`RuntimeException` | `WireFormatError` の列挙子 |

原典の `DomainName`、`Message`、`Question` は `@InternalApi` で、`private[pekko]` か `private[internal]` である。移植では統合テストから呼ぶために公開した。`Question::parse`、`ResourceRecord::parse`、`RecordType::parse`、`RecordClass::parse` と、`WireReader` の読み取りメソッドは `pub(crate)` のままにした。追加だけで、既存の公開 API は変えていない。

### ケース対応

| 原典のケース | Rust のテスト | 原典の期待 | 移植の期待 |
|:---|:---|:---|:---|
| "parse a plain name" | `parse_a_plain_name` | `"www.example.com"` | `as_str() == "www.example.com"` |
| "follow a backwards compression pointer" | `follow_a_backwards_compression_pointer` | `"www.example.com"` | 同じ |
| "reject a pointer that points at itself" | `reject_a_pointer_that_points_at_itself` | `IllegalArgumentException` | `Err(TooManyCompressionPointers)` |
| "reject a pointer cycle" | `reject_a_pointer_cycle` | `IllegalArgumentException` | `Err(TooManyCompressionPointers)` |
| "reject reserved label types" | `reject_reserved_label_types` | `IllegalArgumentException` | `Err(ReservedLabelType(1))` |
| "fail rather than loop on a truncated name" | `fail_rather_than_loop_on_a_truncated_name` | `NoSuchElementException` | `Err(Truncated)` |
| "parse a response that is truncated mid-message" | `parse_a_response_that_is_truncated_mid_message` | id 4、TC、質問1件 `many.bzzt.net` AAAA IN、3セクション空 | 同じ（質問は3つの欄を比べる） |

入力のバイト列は原典と同じである。`label` と `pointer` の補助は原典の式どおりに組み、MessageSpec の入力は原典の符号付きバイト31個を `i8` の配列で書いて、ビットを変えずに `u8` にした。原典の例外は種類しか確かめないが、移植は原典の例外メッセージに対応する列挙子まで確かめる。

## 実装

kernel の `io::dns` モジュールに足した。どれも `core` と `alloc` だけを使う（syn-check の `cfg-std-forbid` は違反0）。

- `WireReader`：原典が受け渡す読み取り位置（`ByteIterator`）と、圧縮参照の参照先になるメッセージ全体（`ByteString`）を1つにした型。値はビッグエンディアンで読む。メッセージの終わりを越える読み取りは `Truncated` で、位置は終わりに残る（原典の `getShort`・`getInt` は残りのバイトを消費してから例外を投げる）。
- `DomainName::parse`：原典と同じ順に処理する。長さ0で終わり、上位2ビットが `11` なら14ビットのオフセットへ飛び、`01`・`10` は `ReservedLabelType` にする。圧縮参照は16回まで辿り、17回目で `TooManyCompressionPointers` にする。ラベルを足すたびに長さを数え、255文字を超えたら `NameTooLong` にする。読み取り位置は、最初の圧縮参照の直後か、名前の終わりの0の直後に進む（原典の `it` は最初の参照の後で別のイテレータに替わる）。メッセージの終わりを越えるラベルは、原典の `take`・`drop` と同じく終わりまで読み、次の長さの読み取りで `Truncated` になる。
- ラベルの各バイトは、Scala の `Byte.toChar` と同じく符号拡張して文字にする（`0xE9` は `U+FFE9`）。
- `RecordType`：原典が登録する22種の定数と、`from_code`（原典の `RecordType.apply`）。読み取りは `RecordTypeSerializer.parse` と同じく、登録のないコードを `UnknownRecordType` にする。
- `RecordClass`：原典の5つの定数。読み取りは `RecordClassSerializer.parse` と同じく `IN`・`CS`・`CH`・`WILDCARD` だけを受け付け、`HS`（4）を含む他のコードを `UnknownRecordClass` にする。
- `Question`：名前、型、クラスの順に読む。
- `ResourceRecord`：原典の `ResourceRecord.parse` が読む枠（名前、型コード、クラスコード、TTL、データ長、データ）を読む。データは型によらずバイト列のまま持つ。TTL は0なら `Duration::ZERO`（原典の `Ttl.never`）、負なら `NegativeTtl`（原典の `Ttl.fromPositive` の例外）。
- `MessageFlags`：QR、AA、TC、RD、RA の各ビット。
- `Message::parse`：ヘッダーの6つの16ビット値を読み、4つのセクションを順に読む。TC ビットがあれば読めない項目を落として次の項目を同じ位置から読み、なければ最初に読めなかった項目の誤りを返す（原典の `Try` と暗黙の flatten）。ヘッダーが12バイトに満たなければ `Truncated`。

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2本と上記の原典実装を、移植の実装・テストと突き合わせた。判定は「等価」で、原典7ケースは観測上等価、除外0件である。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| ラベルを `.` でつなぎ、長さ0で終わる | `DomainName.scala` L54–57、L72–75 | `parse_a_plain_name`、`a_zero_length_byte_alone_is_the_empty_root_name`、`the_reader_moves_past_the_terminating_zero_byte` |
| 圧縮参照は14ビットのオフセットへ飛び、読み取り位置は最初の参照の後 | L59–66 | `follow_a_backwards_compression_pointer`、`the_reader_moves_past_the_first_compression_pointer_only`、`a_pointer_may_be_the_whole_name`、`a_pointer_offset_uses_the_low_six_bits_of_its_first_byte` |
| 参照は16回まで、17回目で例外 | L32、L61–64 | 自己参照・ループの2ケース、`sixteen_compression_pointers_are_followed`、`a_seventeenth_compression_pointer_is_rejected` |
| `0x40`・`0x80` のラベル種別は例外 | L67–70 | `reject_reserved_label_types`、`the_0x80_label_type_is_reserved_too` |
| 255文字を超える名前は例外 | L33、L76–78 | `a_name_of_255_characters_is_accepted`、`a_name_of_256_characters_is_rejected` |
| メッセージの終わりで `NoSuchElementException` | `ByteIterator.next` | `fail_rather_than_loop_on_a_truncated_name`、`a_label_that_runs_past_the_end_is_truncated`、`a_pointer_without_its_second_byte_is_truncated`、`a_pointer_past_the_end_of_the_message_is_truncated` |
| バイトは `Byte.toChar` で文字にする | L74 | `a_byte_from_0x80_up_becomes_a_sign_extended_character` |
| TC があれば読めない項目を落とし、なければ最初の失敗を投げる | `DnsMessage.scala` L156–179 | `parse_a_response_that_is_truncated_mid_message`、`a_response_with_the_tc_bit_leaves_out_the_missing_records`、`a_response_with_the_tc_bit_reads_on_after_a_rejected_question`、`a_response_without_the_tc_bit_fails_on_a_missing_record`、`a_response_without_the_tc_bit_fails_on_a_rejected_question` |
| ヘッダーの不足は TC によらず例外 | L147–154（`Try` の外） | `a_message_cut_inside_its_header_is_truncated` |
| セクション数は `Short`。`0x8000` 以上は負で0件 | L151–167 | `a_section_count_from_0x8000_up_reads_no_entries` |
| 型コードは登録済みだけ、クラスは `HS` を除く4種 | `RecordTypeSerializer.scala`、`RecordClassSerializer.scala` | `record_type` の7件、`record_class` の4件、`question` の4件 |
| レコードの枠、TTL の0と負、データ長の切り詰めと負の長さ | `DnsResourceRecords.scala` `ResourceRecord.parse` | `resource_record` の7件 |

原典との差異は次のとおりで、原典テストが観測する値は変わらない。

- 原典の `ResourceRecord.parse` は、`A`・`CNAME`・`AAAA`・`SRV` のデータを型つきのレコードにデコードし、他の型は `UnknownRecord` としてバイト列のまま持つ。移植は全ての型をバイト列のまま持つ。レコードデータのデコードは issue の対象外である。そのため、TC のない応答で `A` のデータが4バイトに満たないとき、原典は例外を投げるが、移植は受け付ける。
- 原典は失敗を例外で伝える。移植は `Result` で返し、原典の例外メッセージに対応する列挙子を持つ。
- 原典の `Message.id` は `Short`、移植は `u16`。同じ16ビットを持つ。id は照合に使うだけの不透明な値で、このゴールには問い合わせとの照合がないため、専用型にせず `u16` のままにした（`fraktor-coding-rules.md` の domain-primitive の狭い例外。理由は `message.rs` のフィールドにも書いた）。
- `ResourceRecord` の型コードとクラスコードは、専用型 `RecordTypeCode`・`RecordClassCode` で持つ。原典の `UnknownRecord` と同じく定数のないコードも持てるよう、どちらも任意の16ビット値を包む。`RecordTypeCode::record_type()` は登録済みの型があれば返す（PR #144 の Devin の指摘で、`u16` から変えた）。
- 名前の長さの上限は、原典の `MaxNameLength` と同じく、組み立てた `.` 区切りの文字列の文字数で255を超えたら失敗にする。RFC 1035 の255オクテットの上限（各ラベルの長さバイトと終端の0を数える）とは別で、255文字の名前はワイヤ上で257オクテットだが受け付ける（PR #144 の Devin の指摘で、コードのコメントでも区別した）。
- 原典の `RecordType` と `RecordClass` は公開コンストラクタで任意のコードと名前を作れる。移植は定数だけを公開する。
- メッセージの書き出し（`Message.write`、`Question.write`、`DomainName.write`、`MessageFlags.apply`）、`opCode`・`responseCode`、`Question.isSame` は移植していない。問い合わせの送信と resolver の側にあり、対象外である。

### 独立レビュー（別系統のモデル）

PR 作成後、既出の指摘に対応した HEAD を固定して行う（`docs/decisions/takt-workflow-design.md` §2.6）。結果はここに追記する。

## 製品編集前の実測

`f8c4d036cb700b6788fc846fabae2c78e076dc28`（`be01a47` からの製品差分0。`git diff --stat be01a47 f8c4d03 -- 'crates/*/src'` は空）で、担当が次を実行した。テストの blob は、`domain_name_spec.rs` が `6c65f68e`、`message_spec.rs` が `43eb392a` である。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test domain_name_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved imports DomainName, WireFormatError, WireReader`（L35）。宣言6件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test message_spec -- --test-threads=1` | 101 | compile-red。`E0432 unresolved imports Message, RecordClass, RecordType`（L23）。宣言1件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 2ノードとも `expected_compile_error`、`excluded: 0`、`red_recorded: true` |

already_green ではない。製品に DNS メッセージを読む型がなく、2本ともコンパイルに失敗した。実装のコミットでは統合テストを変えていない（`git diff f8c4d03 e2aa776 -- crates/fraktor-actor/tests` は空）。

## 実装後の実測

`e2aa776` で、担当が次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test domain_name_spec -- --test-threads=1` | 0 | 6 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test message_spec -- --test-threads=1` | 0 | 1 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 90 passed（既存37、新規53） |
| `cargo test --workspace` | 0 | 66 スイート、662 passed / 0 failed / 0 ignored |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py` | 0 | 成功 |

PR #144 の Devin の指摘への対応後の HEAD でも、同じコマンドを再実行して同じ結果を確かめた（`--lib io::dns` は 96 passed、`cargo test --workspace` は 668 passed）。

台帳は `G0issue142` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK で、対象ノードはすべて green。`G0issue142` は2件で `{'green': 2}`）。

## PR #144 のレビュー指摘への対応

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| Devin: Pekko 互換の文字数上限は DNS のワイヤ長上限とは別 | 直す | `domain_name.rs` の定数のコメントと rustdoc、`WireFormatError::NameTooLong` の説明、境界テストのコメントで、原典の文字数の上限と RFC 1035 のオクテットの上限を区別した。振る舞いは原典どおりで変えていない |
| Devin: レコードの型コードとクラスコードを型で区別できない | 直す | 専用型 `RecordTypeCode`・`RecordClassCode` を足し、`ResourceRecord` の欄と取得メソッドをその型にした。単体テスト6件を足した |
| Codex | 指摘なし | — |
| CodeRabbit | 対象なし | 利用上限のため、レビューは行われなかった |

## 再現手順

```text
cargo test -p fraktor-actor --test domain_name_spec -- --test-threads=1
cargo test -p fraktor-actor --test message_spec -- --test-threads=1
cargo test -p fraktor-actor --lib io::dns
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue142
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue142
```
