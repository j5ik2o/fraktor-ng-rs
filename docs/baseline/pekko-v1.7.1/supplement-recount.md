# Pekko v1.7.1 事実層の補遺（再計測）

この補遺は、凍結スナップショットに無い値を足す。凍結済みのファイル（`ledger.yaml`、`summary.md`、
`module-deps.dot`、`brief.md`、`cover-memo.md`、`PROVENANCE.md`）は書き換えていない。
baseline の値と食い違う点も、ここに書くだけで、元のファイルは直さない。

- 作成日は 2026-11-11 である
- 根拠は `docs/decisions/fable-graph-model-proposal.md` の承認論点 18 である
- この補遺は元の配布物（`fable-input-fraktor-restart.zip`）に含まれない。`PROVENANCE.md` の
  ファイル対応表の対象外であり、来歴はこのファイルの「来歴」の節が持つ

## この補遺の値は近似である

ここにある値は、すべて正規表現による機械的な数え直しである。コンパイルも意味解析もしていない。
LLM がコードを読んで数えた値は無い。

- 能力の有無は存在判定である。「そのファイルに、正規表現に一致する文字列が 1 回以上あるか」
  だけを見る
- コメント、文字列リテラル、import 文の中の一致も数える
- 継承した基底クラスやヘルパを経由した利用は見えない。ファイルが能力を使っていても、
  そのファイルに文字列が無ければ「使っていない」と数える
- 判定の単位はファイルである。どのテストケースがその能力を使うかは分からない
- `taggedAs` つきのケースは、ケース名の文字列、`taggedAs`、`in` が同じ行にある書き方だけを
  数える。ほかの書き方は拾えないので、ケース数は下限である
- 「abstract class か trait だけを宣言する」は、ファイル内の宣言の形だけで判定した
- 層の分類は、上の存在判定の上に立つ。層なしと数えたファイルが、実際には ActorSystem を
  使う可能性が残る

したがって、ここにある値は triage の下ごしらえである。ノードの状態、種別、ケース単位の除外を
決める根拠にはならない。それらは triage と人間の判断で決める。

## 手法

- 対象は、baseline の `ledger.yaml` が挙げるテストクラスファイル 1,262 件である
- baseline の TestKit 使用頻度は、モジュールのテストソース全体を数えている。この補遺は
  テストクラスファイルだけを数える。対象が違うので、同じ名前の能力でも値は一致しない。
  たとえば `TestProbe` は baseline で 1,756 回、この補遺で 1,717 回である
- ケース数は、まず baseline と同じ 3 種の書き方（`"..." in`、`test("`、`@Test`）で数え直し、
  baseline の値を再現できることを確かめた。そのうえで `"..." taggedAs ... in` を別に数えた
- 能力の正規表現は、2 章の表にすべて載せた
- baseline の TestKit 一覧（11 項目）との対応は次のとおりである。`expectMsg`、`expectMsgPF`、
  `expectMsgType`、`expectMsgAnyOf` の 4 項目は、この補遺では `expectMsg*` にまとめた。
  `expectMsg*` は `expectMsgAllOf` や `expectMsgClass` も含む。`testActorRef` は `TestActorRef` に、
  `TestKit(identifier)` は `TestKit` に対応する

## 再現方法

```sh
git clone --depth 1 --branch v1.7.1 https://github.com/apache/pekko.git <checkout>
python3 ledger/tools/recount_supplement.py \
  --baseline docs/baseline/pekko-v1.7.1/ledger.yaml --pekko <checkout> --markdown
```

「以下は生成物」の行より下は、このコマンドの出力そのままである。次のコマンドが何も出さなければ、
生成物と一致している。

```sh
diff <(sed '1,/^<!-- 以下は生成物 -->$/d' docs/baseline/pekko-v1.7.1/supplement-recount.md) \
     <(python3 ledger/tools/recount_supplement.py \
         --baseline docs/baseline/pekko-v1.7.1/ledger.yaml --pekko <checkout> --markdown)
```

`--baseline` は JSON も受ける。PyYAML が無い環境での変換方法は、スクリプトの docstring にある。

## 来歴

| 項目 | 値 |
|---|---|
| Pekko | `https://github.com/apache/pekko.git` のタグ `v1.7.1`、コミット `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`（shallow clone） |
| baseline の台帳 | `ledger.yaml`、SHA-256 `f0ab69cb1c7093eeb369cb6a299719f63f09cdf560bf918f8a5a8afad0a401bd`（`PROVENANCE.md` の値と一致） |
| スクリプト | `ledger/tools/recount_supplement.py`（この補遺と同じコミットの版） |
| 実行日 | 2026-11-11 |
| 実行環境 | macOS、Python 3.15.2 |
| 作成者 | Fable（外部モデル）。承認論点 18 に基づく人間の指示で作成した |
| 独立した検証 | この補遺の作成時点では受けていない |

<!-- 以下は生成物 -->
## 要点

- 台帳のケース数は、1,262 ファイル中 1,262 ファイルで再現できた。
- 台帳は `taggedAs` つきのケースを数えていない。該当は 35 ファイル、314 ケースである。台帳の合計 7,561 に足すと 7,875 になる。
- 台帳でケース数が 0 の 184 ファイルのうち、14 ファイルは `taggedAs` つきのケースだけを持つ（198 ケース）。
- `EventFilter` 系は baseline の TestKit 一覧に無い。116 ファイルに 471 箇所ある。
- typed の `expectMessage` 系は baseline の TestKit 一覧に無い。155 ファイルに 1,745 箇所ある。
- stream の testkit 系は baseline の TestKit 一覧に無い。194 ファイルに 1,996 箇所ある。

## 1. ケース数

### モジュール別

`taggedAs` つきのケースを持つモジュールだけを挙げる。合計の行は全モジュールの値である。

| モジュール | 台帳のケース数 | `taggedAs` つき | 和 |
|---|---|---|---|
| `actor-tests` | 1,363 | 152 | 1,515 |
| `stream-tests` | 2,526 | 72 | 2,598 |
| `remote-tests` | 3 | 31 | 34 |
| `cluster` | 319 | 28 | 347 |
| `remote` | 439 | 10 | 449 |
| `persistence` | 186 | 9 | 195 |
| `actor-typed-tests` | 597 | 8 | 605 |
| `cluster-metrics` | 54 | 2 | 56 |
| `cluster-sharding` | 190 | 1 | 191 |
| `testkit` | 81 | 1 | 82 |
| 合計 | 7,561 | 314 | 7,875 |

### タグ別

1 つのケースに複数のタグが付く場合は、タグごとに数える。

| タグ | ケース数 |
|---|---|
| TimingTest | 271 |
| LongRunningTest | 43 |

### `taggedAs` つきのケースを持つファイル

| ファイル | 台帳のケース数 | `taggedAs` つき |
|---|---|---|
| `actor-tests/src/test/scala/org/apache/pekko/pattern/CircuitBreakerSpec.scala` | 0 | 63 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/RestartSpec.scala` | 0 | 38 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/SchedulerSpec.scala` | 2 | 37 |
| `remote-tests/src/test/scala/org/apache/pekko/remote/testconductor/BarrierSpec.scala` | 0 | 31 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/FlowGroupedWithinSpec.scala` | 0 | 16 |
| `cluster/src/test/scala/org/apache/pekko/cluster/JoinConfigCompatCheckerSpec.scala` | 0 | 15 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/ReceiveTimeoutSpec.scala` | 2 | 11 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMTimingSpec.scala` | 0 | 10 |
| `actor-tests/src/test/scala/org/apache/pekko/util/BoundedBlockingQueueSpec.scala` | 52 | 10 |
| `cluster/src/test/scala/org/apache/pekko/cluster/MixedProtocolClusterSpec.scala` | 0 | 9 |
| `actor-typed-tests/src/test/scala/org/apache/pekko/actor/typed/TimerSpec.scala` | 5 | 8 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/TimerSpec.scala` | 3 | 7 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/TickSourceSpec.scala` | 1 | 7 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/SupervisorHierarchySpec.scala` | 0 | 6 |
| `persistence/src/test/scala/org/apache/pekko/persistence/AtLeastOnceDeliverySpec.scala` | 1 | 6 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/FlowDelaySpec.scala` | 14 | 6 |
| `actor-tests/src/test/scala/org/apache/pekko/pattern/CircuitBreakerMTSpec.scala` | 0 | 4 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/TypedActorSpec.scala` | 25 | 3 |
| `cluster/src/test/scala/org/apache/pekko/cluster/testkit/AutoDownSpec.scala` | 6 | 3 |
| `persistence/src/test/scala/org/apache/pekko/persistence/fsm/PersistentFSMSpec.scala` | 9 | 3 |
| `remote/src/test/scala/org/apache/pekko/remote/artery/RemoteWatcherSpec.scala` | 1 | 3 |
| `remote/src/test/scala/org/apache/pekko/remote/classic/RemoteWatcherSpec.scala` | 1 | 3 |
| `remote/src/test/scala/org/apache/pekko/remote/classic/transport/ThrottlerTransportAdapterSpec.scala` | 0 | 2 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/impl/fusing/InterpreterStressSpec.scala` | 3 | 2 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/FSMActorSpec.scala` | 8 | 1 |
| `cluster-metrics/src/test/scala/org/apache/pekko/cluster/metrics/EWMASpec.scala` | 6 | 1 |
| `cluster-metrics/src/test/scala/org/apache/pekko/cluster/metrics/MetricsCollectorSpec.scala` | 4 | 1 |
| `cluster-sharding/src/test/scala/org/apache/pekko/cluster/sharding/JoinConfigCompatCheckShardingSpec.scala` | 0 | 1 |
| `cluster/src/test/scala/org/apache/pekko/cluster/ShutdownAfterJoinSeedNodesSpec.scala` | 0 | 1 |
| `remote/src/test/scala/org/apache/pekko/remote/classic/transport/PekkoProtocolStressTest.scala` | 0 | 1 |
| `remote/src/test/scala/org/apache/pekko/remote/classic/transport/SystemMessageDeliveryStressTest.scala` | 0 | 1 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/FlowFoldAsyncSpec.scala` | 20 | 1 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/FlowGroupedWeightedSpec.scala` | 8 | 1 |
| `stream-tests/src/test/scala/org/apache/pekko/stream/scaladsl/FlowThrottleSpec.scala` | 22 | 1 |
| `testkit/src/test/scala/org/apache/pekko/testkit/TestTimeSpec.scala` | 2 | 1 |

### 台帳でケース数が 0 のファイルの内訳

| 内訳 | ファイル数 |
|---|---|
| `taggedAs` つきのケースだけを持つ | 14 |
| abstract class か trait だけを宣言する | 13 |
| その他 | 157 |
| 合計 | 184 |

abstract class か trait だけを宣言するファイルは次のとおりである。`fixture` の候補だが、種別を変えるには人間の承認が要る。

- `cluster-sharding-typed/src/test/scala/docs/org/apache/pekko/cluster/sharding/typed/ShardingCompileOnlySpec.scala`
- `docs/src/test/java/jdocs/persistence/PersistenceMultiDocTest.java`
- `docs/src/test/scala/docs/CompileOnlySpec.scala`
- `docs/src/test/scala/docs/persistence/PersistenceMultiDocSpec.scala`
- `docs/src/test/scala/docs/stream/cookbook/RecipeSpec.scala`
- `persistence-typed-tests/src/test/scala/docs/org/apache/pekko/persistence/typed/ReplicatedEventSourcingCompileOnlySpec.scala`
- `persistence-typed-tests/src/test/scala/org/apache/pekko/persistence/typed/ReplicationBaseSpec.scala`
- `remote-tests/src/test/scala/org/apache/pekko/remote/testkit/STMultiNodeSpec.scala`
- `remote/src/test/scala/org/apache/pekko/remote/NetworkFailureSpec.scala`
- `remote/src/test/scala/org/apache/pekko/remote/artery/ArteryMultiNodeSpec.scala`
- `stream-testkit/src/test/scala/org/apache/pekko/stream/testkit/StreamSpec.scala`
- `testkit/src/test/scala/org/apache/pekko/testkit/AbstractSpec.scala`
- `testkit/src/test/scala/org/apache/pekko/testkit/PekkoSpec.scala`

## 2. TestKit の能力

### 能力ごとの出現

ファイル数は、その正規表現に 1 回以上一致したファイルの数である。「baseline の一覧」は、baseline の TestKit 一覧（11 項目）にその能力があるかどうかを示す。

| 分類 | 能力 | 正規表現 | ファイル数 | 出現回数 | baseline の一覧 |
|---|---|---|---|---|---|
| 基底 fixture | extends PekkoSpec | `extends\s+PekkoSpec\b` | 340 | 359 | なし |
| 基底 fixture | extends StreamSpec | `extends\s+StreamSpec\b` | 186 | 187 | なし |
| 基底 fixture | ActorTestKit 系 | `\b(ScalaTestWithActorTestKit\|ActorTestKit)\b` | 186 | 431 | なし |
| classic | PekkoSpec | `\bPekkoSpec\b` | 393 | 779 | なし |
| classic | TestKit | `\bTestKit(Base)?\b` | 113 | 557 | あり |
| classic | testActor | `\btestActor\b` | 157 | 805 | なし |
| classic | ImplicitSender | `\bImplicitSender\b` | 171 | 297 | なし |
| classic | DefaultTimeout | `\bDefaultTimeout\b` | 71 | 126 | なし |
| classic | expectMsg* | `\bexpectMsg\w*\b` | 310 | 3,235 | あり |
| classic | expectNoMessage | `\bexpectNoMessage\b` | 249 | 973 | あり |
| classic | TestProbe | `\bTestProbe\b` | 280 | 1,717 | あり |
| classic | expectTerminated | `\bexpectTerminated\b` | 130 | 348 | なし |
| classic | lastSender | `\blastSender\b` | 29 | 110 | なし |
| classic | EventFilter | `\b(EventFilter\|filterEvents\|filterException)\b` | 116 | 471 | なし |
| classic | TestLatch | `\bTestLatch\b` | 43 | 211 | あり |
| classic | within | `\bwithin\(` | 58 | 182 | あり |
| classic | awaitAssert | `\bawaitAssert\b` | 86 | 278 | なし |
| classic | awaitCond | `\bawaitCond\b` | 39 | 124 | なし |
| classic | fishForMessage | `\bfishForMessage\b` | 18 | 34 | なし |
| classic | receiveWhile | `\breceiveWhile\b` | 16 | 23 | なし |
| classic | receiveN | `\breceiveN\b` | 33 | 82 | なし |
| classic | receiveOne | `\breceiveOne\b` | 4 | 4 | なし |
| classic | ignoreMsg | `\bignoreMsg\b` | 6 | 9 | なし |
| classic | TestActorRef | `\bTestActorRef\b` | 9 | 87 | あり |
| classic | TestFSMRef | `\bTestFSMRef\b` | 3 | 16 | なし |
| classic | CallingThreadDispatcher | `\bCallingThreadDispatcher\b` | 7 | 16 | なし |
| classic | TestBarrier | `\bTestBarrier\b` | 1 | 1 | あり |
| classic | setAutoPilot | `\bsetAutoPilot\b` | 3 | 3 | なし |
| typed | expectMessage 系 (typed) | `\bexpectMessage(Type\|Class)?\b` | 155 | 1,745 | なし |
| typed | receiveMessage 系 (typed) | `\breceiveMessages?\b` | 131 | 736 | なし |
| typed | BehaviorTestKit | `\bBehaviorTestKit\b` | 11 | 127 | なし |
| typed | TestInbox | `\bTestInbox\b` | 17 | 116 | なし |
| typed | LoggingTestKit 系 | `\b(LoggingTestKit\|LoggingEventFilter)\b` | 40 | 276 | なし |
| typed | LogCapturing | `\bLogCapturing\b` | 212 | 435 | なし |
| typed | ManualTime | `\bManualTime\b` | 2 | 8 | なし |
| stream | stream testkit (TestSubscriber/TestPublisher/TestSink/TestSource) | `\b(TestSubscriber\|TestPublisher\|TestSink\|TestSource)\b` | 194 | 1,996 | なし |
| その他 | Java 直列化 (JavaSerializer/ObjectOutputStream/ObjectInputStream) | `\b(JavaSerializer\|ObjectOutputStream\|ObjectInputStream)\b` | 10 | 63 | なし |

### モジュール別のファイル数

値は、その能力に 1 回以上一致したファイルの数である。

| モジュール | ファイル数 | PekkoSpec 継承 | StreamSpec 継承 | ActorTestKit 系 | EventFilter 系 | testActor | expectTerminated | expectMessage 系 | stream testkit |
|---|---|---|---|---|---|---|---|---|---|
| `actor-testkit-typed` | 23 | 0 | 0 | 13 | 0 | 0 | 2 | 15 | 0 |
| `actor-tests` | 177 | 114 | 0 | 0 | 41 | 43 | 25 | 0 | 0 |
| `actor-typed-tests` | 117 | 2 | 0 | 65 | 0 | 1 | 23 | 49 | 0 |
| `cluster` | 38 | 21 | 0 | 0 | 2 | 4 | 2 | 0 | 0 |
| `cluster-metrics` | 9 | 7 | 0 | 0 | 0 | 1 | 0 | 0 | 0 |
| `cluster-sharding` | 36 | 28 | 0 | 0 | 2 | 3 | 8 | 0 | 0 |
| `cluster-sharding-typed` | 35 | 0 | 0 | 15 | 0 | 0 | 1 | 11 | 0 |
| `cluster-tools` | 13 | 9 | 0 | 0 | 0 | 4 | 0 | 0 | 0 |
| `cluster-typed` | 25 | 1 | 0 | 14 | 0 | 0 | 1 | 10 | 0 |
| `coordination` | 3 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0 |
| `discovery` | 9 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `distributed-data` | 23 | 3 | 0 | 0 | 0 | 2 | 1 | 0 | 0 |
| `docs` | 173 | 49 | 0 | 11 | 6 | 13 | 12 | 7 | 8 |
| `osgi` | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `persistence` | 42 | 12 | 0 | 0 | 12 | 21 | 12 | 0 | 0 |
| `persistence-query` | 7 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 3 |
| `persistence-shared` | 3 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 0 |
| `persistence-tck` | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `persistence-testkit` | 25 | 0 | 0 | 10 | 4 | 8 | 7 | 8 | 4 |
| `persistence-typed` | 22 | 0 | 0 | 12 | 0 | 0 | 9 | 10 | 0 |
| `persistence-typed-tests` | 50 | 0 | 0 | 42 | 0 | 0 | 14 | 41 | 0 |
| `pki` | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `remote` | 107 | 62 | 0 | 0 | 24 | 23 | 6 | 0 | 9 |
| `remote-tests` | 5 | 2 | 0 | 0 | 1 | 2 | 0 | 0 | 0 |
| `serialization-jackson` | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `slf4j` | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `stream-testkit` | 4 | 3 | 0 | 0 | 1 | 0 | 0 | 0 | 3 |
| `stream-tests` | 254 | 9 | 186 | 0 | 18 | 25 | 6 | 0 | 164 |
| `stream-tests-tck` | 24 | 0 | 0 | 0 | 1 | 0 | 0 | 0 | 0 |
| `stream-typed` | 7 | 0 | 0 | 4 | 0 | 0 | 0 | 4 | 2 |
| `stream-typed-tests` | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| `testkit` | 15 | 7 | 0 | 0 | 3 | 6 | 1 | 0 | 0 |

## 3. テスト基盤の層

層は classic の TestKit の能力だけで定義する。層の番号は、ファイルが参照する能力のうち最も高い層である。

- 層なしは、ActorSystem、TestKit、TestProbe、ActorTestKit、StreamSpec などを 1 つも参照しないファイルである。
- 層 1 の能力は、`PekkoSpec`、`TestKit`、`testActor`、`ImplicitSender`、`DefaultTimeout`、`expectMsg*`、`expectNoMessage`、`TestProbe`、`expectTerminated`、`lastSender` である。
- 層 2 の能力は、`EventFilter`、`TestLatch`、`within`、`awaitAssert`、`awaitCond`、`fishForMessage`、`receiveWhile`、`receiveN`、`receiveOne`、`ignoreMsg` である。
- 層 3 の能力は、`TestActorRef`、`TestFSMRef`、`CallingThreadDispatcher`、`TestBarrier`、`setAutoPilot` である。

| 対象 | ファイル数 | 層なし | 層 1 | 層 2 | 層 3 |
|---|---|---|---|---|---|
| actor-tests | 177 | 47（405） | 64（377） | 58（484） | 8（97） |
| actor-tests のパッケージ actor | 49 | 4（37） | 17（82） | 26（232） | 2（13） |
| actor-tests のパッケージ util | 32 | 26（289） | 6（38） | 0（0） | 0（0） |
| testkit | 15 | 2（9） | 6（12） | 3（15） | 4（45） |

括弧の中は台帳のケース数である。docs 以外の全体では、層なしは 244 ファイル（1,221 ケース）である。

## 4. Java 直列化を参照するファイル

| ファイル | 出現回数 |
|---|---|
| `actor-tests/src/test/scala/org/apache/pekko/actor/ActorRefSpec.scala` | 11 |
| `actor-tests/src/test/scala/org/apache/pekko/actor/TypedActorSpec.scala` | 10 |
| `actor-tests/src/test/scala/org/apache/pekko/serialization/SerializationSetupSpec.scala` | 5 |
| `actor-tests/src/test/scala/org/apache/pekko/serialization/SerializeSpec.scala` | 14 |
| `actor-tests/src/test/scala/org/apache/pekko/util/ByteStringSpec.scala` | 5 |
| `actor-typed-tests/src/test/scala/org/apache/pekko/actor/typed/internal/ActorRefSerializationSpec.scala` | 3 |
| `cluster-metrics/src/test/scala/org/apache/pekko/cluster/metrics/protobuf/NumberInputStreamSpec.scala` | 8 |
| `remote/src/test/scala/org/apache/pekko/remote/serialization/AllowJavaSerializationOffSpec.scala` | 5 |
| `remote/src/test/scala/org/apache/pekko/remote/serialization/DaemonMsgCreateSerializerAllowJavaSerializationSpec.scala` | 1 |
| `remote/src/test/scala/org/apache/pekko/remote/serialization/MiscMessageSerializerSpec.scala` | 1 |

