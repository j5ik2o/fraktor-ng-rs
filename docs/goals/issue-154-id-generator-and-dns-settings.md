# ゴール G0issue154：classic の DNS 問い合わせ ID と DNS 設定の2スイート

[Issue #154](https://github.com/j5ik2o/fraktor-ng-rs/issues/154) と [Issue #155](https://github.com/j5ik2o/fraktor-ng-rs/issues/155)（Orca 項目230）の対象は、Pekko v1.7.1 の次の2つのテストである。`DnsSettings` が `IdGenerator` のポリシーを参照するので、2つのノードを1つのゴールにまとめた。

- `IdGeneratorSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/dns/IdGeneratorSpec.scala)。thread-local random と secure random の2ケース
- `DnsSettingsSpec`: [原典のテスト](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor-tests/src/test/scala/org/apache/pekko/io/dns/DnsSettingsSpec.scala)。name server、search domain、ndots、TTL、ID 生成ポリシーの10ケース
- 原典の実装: [IdGenerator.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/IdGenerator.scala)、[DnsSettings.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala)（`class DnsSettings`）、[CachePolicy.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/CachePolicy.scala)

範囲は、ID の生成ポリシーと、設定値の解釈だけである。DNS の送受信、名前解決、キャッシュ本体、resolver、HOCON などの設定ファイルの仕組み、OS の DNS 設定の読み取りと変更は対象外。OS を読む adaptor は [#156](https://github.com/j5ik2o/fraktor-ng-rs/issues/156) で追跡する。

## 対象ノード

| id | Rust の統合テスト | 原典 | 移植 | 成功 | 除外 |
|:---|:---|---:|---:|---:|---:|
| `actor-tests/src/test/scala/org/apache/pekko/io/dns/IdGeneratorSpec.scala` | `crates/fraktor-actor/tests/id_generator_spec.rs` | 2 | 2 | 2 | 0 |
| `actor-tests/src/test/scala/org/apache/pekko/io/dns/DnsSettingsSpec.scala` | `crates/fraktor-actor/tests/dns_settings_spec.rs` | 10 | 10 | 10 | 0 |

合計は原典12件・移植12件・成功12件・除外0件。原典件数は `" in {"` の数で数えた。`ignore` と `pending` はない。台帳の `needs.testkit`（`extends-pekko-spec`、`pekko-spec`）は変えていない。`check_needs.py` は G0b が提供元として閉性 OK を返す。

## 着手時の設計判断（調整担当の決定、2026-11-25）

原典12ケースを移植するには、既存の方針と原典の作りが合わない点が4つあった。着手前に選択肢と推奨を調整担当へ上げ、次のとおり決まった。

| 論点 | 決定 |
|:---|:---|
| D1 設定の入力形式。原典は HOCON の `Config` を受け取るが、fraktor には設定の仕組みがない（G0b の決定 E） | 原典の設定キーを写した型付きの入力値 `DnsSettingsConfig` を足す。HOCON のパーサーは作らない。ID ポリシーは文字列のまま受け取り、`DnsSettingsValues::new` で検査して、内部では型付きで持つ |
| D2 既存の `DnsSettings` はフィールドなしの型（#136 で原典の `object DnsSettings` に合わせた） | フィールドなしの型と `parse_nameserver_address` は変えない。リポジトリ内に使う箇所がなくても、値の構築とパターンは公開の約束である。原典の `class DnsSettings` の設定値は、別の型 `DnsSettingsValues` に持たせる |
| D3 ホストの既定値（`"default"`）。原典は JNDI と `/etc/resolv.conf` を読むが、kernel は std を使えない | kernel の port `HostDnsConfiguration` で注入する。テストは制御した値を渡す。実機の OS を読む adaptor は別の範囲にし、#156 を起票した |
| D4 乱数源。kernel は no_std の方針で、std にも暗号用の乱数の公開 API がない | ThreadLocalRandom と SecureRandom の2方式に絞る。乱数と OS のエントロピーは adaptor で扱い、依存 `getrandom` 0.3 は adaptor のコードだけが使う。SecureRandom の失敗はエラーで返し、別の方式へ切り替えない。EnhancedDoubleHashRandom は設定値の識別だけを持ち、生成は未対応として明示する |

## 実行経路

正規の TAKT で2工程を直列に実行した。工程は2つとも同じゴールに属する。

1. 開始点のブランチ `j5ik2o/issue-154-155-dns-settings-id` で、台帳の2ノードの `goal:` 行だけを `G0issue154` にした（`32d869f`、`ledger/README.md` のゴール発行手順）。`rollup.py --goal G0issue154` は2件 `unported`、`check_needs.py` は閉性 OK。
2. takt-mcp（`~/local/takt-custom/bin/takt-mcp`、serverInfo takt 0.76.1）を stdio で起動した。cwd は本体のチェックアウトである。既存のゴール全45件とタスク全133件に #154・#155 と一致するものがなく、稼働中・待機中のタスクもないことを確かめた。そのうえでゴール `629c5b51-1cea-4bc8-a357-1ee58bd268ed`（ブランチ `takt/20261124T1912-goal-629c5b51`、統合先 main）を作った。ワークフローは `simple` で、remediation は子のワークフロー review-remediation の呼び出しである。`spark-low` の実効設定は `dgx-flash`（opencode、`dgx/deepseek-v4.1-flash`）。
3. 第1工程（workKey `issue-154-155/01-source-red`）はテストの移植と red の実測である。公開 API の名前とシグネチャを投入文で固定した手順固定の作業なので、`runtimeAssignment` に `spark-low` を指定した。2026-11-24T19:13:16Z に開始し、19:23:55Z に完了した。差し戻しはなかった。成果 `3460283` を `takt_merge_goal_task` でゴールブランチへ統合した（`6e3356c`）。
4. 第2工程（workKey `issue-154-155/02-implement-green`）は実装、green、回帰の検証である。振る舞いの契約（評価の順序、ホストを呼ぶ条件、ID の変換式、失敗の扱い）を投入文で固定したので、こちらも `spark-low` を指定した。19:24:58Z に開始し、19:57:44Z に完了した。TAKT のレビューで1回差し戻された（禁止した `#[allow(clippy::too_many_arguments)]` が残っていた。外しても clippy は通る）。remediation で削除された。成果 `69cd1a3` をゴールブランチへ統合した（`765599e`）。

工程は直列で、並列実行はしていない。TAKT の同時実行上限は変えていない。どちらの工程も停滞せず、直接実装への切り替えはしていない。

| 段階 | コミット | 内容 |
|:---|:---|:---|
| タグ付け | `32d869f` | 2ノードの `goal: G0issue154`（台帳2行） |
| テストと台帳 red（第1工程） | `3460283` | 移植テスト2本、2ノードの status `red` と rust の欄。製品差分0 |
| 実装と台帳 green（第2工程） | `69cd1a3` | kernel の公開項目14個、adaptor の型2個、依存 `getrandom`、単体テスト、2ノードの green、統合テストへの import 1行 |
| 担当のレビュー修正 | `a41b70c`、`4f67deb` | 後述の「実装者自身のレビュー」と aislop の指摘 |
| ゴール文書 | `78de2a1` | 本書の初版 |
| PR #157 の指摘への対応 | `7eef22e`、`152b7a0` | 後述の「PR #157 のレビュー指摘への対応」 |
| 独立レビューの指摘への対応 | `50d22a2`、`af56ec8` | 後述の「独立レビュー（別系統のモデル）」 |

第1工程のテストには取りこぼしが1件あった。`id_generator_spec.rs` が trait `IdGenerator` を import しておらず、実装後も `next_id` を呼べなかった。第2工程の投入文で、この import 1行の追加だけを許可した。アサーションは変えていない。

## 原典との照合

### 型の対応

| 原典 | 移植 |
|:---|:---|
| `IdGenerator.Policy`（3値） | `kernel::io::dns::IdGeneratorPolicy`（`ThreadLocalRandom`、`SecureRandom`、`EnhancedDoubleHashRandom`） |
| `trait IdGenerator { def nextId(): Short }` | `kernel::io::dns::IdGenerator`（`next_id(&self) -> Result<i16, IdGenerationError>`、`Send + Sync`） |
| `IdGenerator(policy)` | `adaptor::RandomIdGenerator::new(policy) -> Result<_, UnsupportedIdGeneratorPolicyError>` |
| `object DnsSettings`（`parseNameserverAddress`） | 既存の `kernel::io::dns::DnsSettings`（フィールドなしの型。変更なし） |
| `class DnsSettings(system, c: Config)` | `kernel::io::dns::DnsSettingsValues::new(&DnsSettingsConfig, &impl HostDnsConfiguration) -> Result<_, DnsSettingsError>` |
| `Config` の DNS のキー | `DnsSettingsConfig`（`new` と `with_*`） |
| JNDI と `/etc/resolv.conf` の読み取り | `HostDnsConfiguration`（`name_servers()`、`resolv_conf()`） |
| `CachePolicy.Never`／`Forever`／`Ttl` | `CachePolicy`（`Never`、`Forever`、`Ttl(Ttl)`） |
| `Ttl.fromPositive(d)` | `Ttl::from_positive(Duration) -> Result<Ttl, NonPositiveTtlError>` |
| `NameServers`、`SearchDomains`、`NDots`、`PositiveCachePolicy`、`NegativeCachePolicy`、`IdGeneratorPolicy`、`ResolveTimeout` | `name_servers()`、`search_domains()`、`ndots()`、`positive_cache_policy()`、`negative_cache_policy()`、`id_generator_policy()`、`resolve_timeout()` |
| 例外（`IllegalArgumentException`、`IllegalStateException`） | `DnsSettingsError` の `Result` |

### 設定キーの対応

| 原典のキー | HOCON の値 | 移植の型付き入力 |
|:---|:---|:---|
| `nameservers` | `"default"`／文字列／文字列のリスト | `NameserversSetting::Default`／`Single(String)`／`List(Vec<String>)` |
| `resolve-timeout` | 期間 | `core::time::Duration` |
| `search-domains` | `"default"`／文字列／文字列のリスト | `SearchDomainsSetting::Default`／`Single(String)`／`List(Vec<String>)` |
| `ndots` | `"default"`／整数 | `NdotsSetting::Default`／`Value(i32)` |
| `positive-ttl`、`negative-ttl` | `forever`／`never`／期間 | `TtlSetting::Forever`／`Never`／`Duration(Duration)` |
| `id-generator-policy` | 文字列 | `String`（`DnsSettingsValues::new` で検査する） |

原典の `parseString(..).withFallback(defaultConfig)` は、書いたキーが優先する。移植は既定値の入力に `with_*` で上書きする。"use host search domains if set to default" だけは `defaultConfig.withFallback(parseString(..))` で、`defaultConfig` 側が優先する。`defaultConfig` は2つのキーを両方持つので、実際の設定は `defaultConfig` そのものになる。移植も既定値の入力をそのまま使い、そのことをテストのコメントに書いた。`10 d` は 864000 秒の `Duration` にした。

### ケース対応

| 原典のケース | Rust のテスト |
|:---|:---|
| "provide a thread-local-random" | `provide_a_thread_local_random` |
| "provide a secure-random" | `provide_a_secure_random` |
| "use host servers if set to default" | `use_host_servers_if_set_to_default` |
| "parse a single name server" | `parse_a_single_name_server` |
| "parse a list of name servers" | `parse_a_list_of_name_servers` |
| "use host search domains if set to default" | `use_host_search_domains_if_set_to_default` |
| "parse a single search domain" | `parse_a_single_search_domain` |
| "parse a single list of search domains" | `parse_a_single_list_of_search_domains` |
| "use host ndots if set to default" | `use_host_ndots_if_set_to_default` |
| "parse ndots" | `parse_ndots` |
| "parse ttl" | `parse_ttl` |
| "parse id-generator-policy" | `parse_id_generator_policy` |

IdGeneratorSpec の2ケースは、原典と同じく `next_id` を2回呼び、1回目が `Short.MaxValue` 以下、2回目が `Short.MinValue` 以上であることを確かめる。`i16` どうしの比較は clippy の `absurd_extreme_comparisons`（deny）になるので、`i32` にして比べた。乱数の値や分布には依存しない。DnsSettingsSpec の「default」の3ケースは、原典と同じく例外が出ない（`Ok` が返る）ことだけを確かめる。ホストの値は制御した `HostDnsConfiguration` から読み、実機の値には依存しない。

## 実装

- kernel の `io::dns` に14の公開項目を足した（`IdGeneratorPolicy`、`IdGenerator`、`IdGenerationError`、`HostDnsConfiguration`、`NameserversSetting`、`SearchDomainsSetting`、`NdotsSetting`、`TtlSetting`、`DnsSettingsConfig`、`Ttl`、`NonPositiveTtlError`、`CachePolicy`、`DnsSettingsError`、`DnsSettingsValues`。うち `IdGenerator` と `HostDnsConfiguration` は trait）。どれも `core` と `alloc` だけを使う。既存の `NameserverAddressError` には `core::error::Error` の実装だけを足した（`DnsSettingsError::InvalidNameserver` の `source` にするため）。
- `DnsSettingsValues::new` は、原典の `class DnsSettings` のフィールドの初期化と同じ順（nameservers、resolve-timeout、positive-ttl、negative-ttl、search-domains、ndots）で評価し、最初の失敗を返す。name server の文字列は、#136 の `DnsSettings::parse_nameserver_address` で読む。`"default"` の name server が `None` か空なら `UnableToDetermineDefaultNameservers`、明示の空のリストは `EmptyNameservers`、負の ndots は `NegativeNdots`、ゼロの TTL は `NonPositiveTtl` にする。各設定の値の変換は、設定の型の impl に置いた（`NameserversSetting::resolve`、`TtlSetting::cache_policy`、`SearchDomainsSetting::resolve`、`NdotsSetting::resolve`、`IdGeneratorPolicy::from_config_value`。どれも `pub(crate)`）。`DnsSettingsValues::new` は評価の順序と、ホストを読むかどうかだけを受け持つ。ホストの name server は `Default` のときだけ読み、resolv.conf は search-domains か ndots が `Default` のときだけ、1回の生成で最大1回読む（原典の `lazy val resolvConf` と同じ）。
- adaptor に `RandomIdGenerator` と `UnsupportedIdGeneratorPolicyError` を足した。ID は原典の `(nextInt(65536) + Short.MinValue).toShort` と同じく、0〜65535 の値 `x` から `x − 32768` にする。ThreadLocalRandom は、Java の `ThreadLocalRandom` と同じ SplitMix 系の状態（増分 `0x9E37_79B9_7F4A_7C15` と `mix32`）をスレッドごとに持ち、種は std の `RandomState` から作る。SecureRandom は、呼ぶたびに `getrandom::fill` で2バイトを読み、失敗は `IdGenerationError::EntropyUnavailable` で返す。どの乱数源を読むかは、非公開の表 `DrawSources`（ポリシーごとの乱数源）を通して決める。本番は OS と std の乱数源を使い、単体テストは値を固定した乱数源を渡す。
- 依存 `getrandom = "0.3"`（0.3.4）を `fraktor-actor` に足した。使うのは `adaptor/random_id_generator.rs` だけである。`Cargo.lock` には `cfg-if`、`libc`、`r-efi`、`wasip2`、`wit-bindgen` も入ったが、`libc` 以外は UEFI と WASI の対象でだけ使われる。

## 原典との差異

原典12ケースが観測する値は変わらない。

- 設定は HOCON ではなく型付きの入力値で受け取る（D1）。そのため、型の上で表せない入力がある。`ndots` に `"default"` 以外の文字列を書く、`nameservers` と `search-domains` に文字列以外の型を書く、`resolve-timeout` や TTL に負の期間を書く、といった、原典で `IllegalArgumentException` になる入力は、移植では作れない。
- 原典の `IdGeneratorPolicy` は `lazy val` で、最初に読んだときに検査する。移植は `DnsSettingsValues::new` の最後で検査する。そのため、不正なポリシーの文字列は、原典では生成に成功して読むときに失敗し、移植では生成時に失敗する。ほかの失敗があるときは、ほかの失敗が先に返る。
- 原典の `DnsSettings` は `ExtendedActorSystem` を受け取り、OS の設定の読み取り（反射）とログに使う。移植はホストの設定を `HostDnsConfiguration` で受け取り、アクターシステムは受け取らない。resolv.conf の解析失敗のログは、#156 の adaptor の担当になる。
- `IdGenerator.nextId()` は `Short` を返す。移植の `next_id` は `Result<i16, IdGenerationError>` を返し、SecureRandom の OS のエントロピーの失敗を値で返す。ThreadLocalRandom は失敗しない。
- `EnhancedDoubleHashRandom` は、設定値 `""` と `"enhanced-double-hash-random"` を識別するためだけにある。`RandomIdGenerator::new` はこのポリシーを `UnsupportedIdGeneratorPolicyError` で拒否し、別のポリシーには置き換えない（D4）。原典の `EnhancedDoubleHashGenerator` と、引数なしの `IdGenerator()`、任意の `java.util.Random` を受け取る `random(rand)` は移植していない。
- `CachePolicy.Ttl` の `never`、`effectivelyForever`、`toTll`、順序は移植していない。このゴールでは使わない。

## 等価性レビュー

### 実装者自身のレビュー

実施者は本 issue の担当（Claude Opus 5.5）。Pekko 固定 SHA の原典テスト2本と、`IdGenerator.scala`・`DnsSettings.scala`・`CachePolicy.scala` を、TAKT が生成した実装・テストと突き合わせた。TAKT の作業ツリーには原典の `pekko/` がなく、TAKT のレビューは投入文に書いた原典の要約を正本にしていた。原典との照合は担当が行った。判定は「等価」で、原典12ケースは観測上等価、除外0件である。

| 原典の振る舞い | 原典の根拠 | 移植で確かめているテスト |
|:---|:---|:---|
| `"default"` の name server は OS の値。取得失敗と空は `IllegalStateException` | `DnsSettings.scala` の `NameServers`、`failUnableToDetermineDefaultNameservers` | 統合テスト `use_host_servers_if_set_to_default`、単体テストの `None`・空・ホスト値の各ケース |
| 文字列1つは `parseNameserverAddress`、リストは各要素を読み、空は失敗 | `NameServers` の `STRING`・`LIST` の分岐 | 統合テストの2件、単体テストの不正な要素・空のリスト・`[::1]:5353` |
| search と ndots の `"default"` は resolv.conf の値、なければ `Nil` と 1 | `SearchDomains`、`NDots`、`lazy val resolvConf` | 統合テストの2件、単体テストの resolv.conf あり・なし・負の ndots、resolv.conf を1回だけ読むこと |
| 明示の ndots は負なら失敗 | `NDots` の `NUMBER` の分岐 | `parse_ndots`、単体テストの -1 と 0 |
| TTL は `forever`・`never`・正の期間、ゼロは失敗 | `getTtl`、`Ttl.fromPositive` | `parse_ttl`、単体テストのゼロ、`ttl_test.rs` |
| ポリシーの文字列の対応と、不正な文字列の失敗 | `IdGeneratorPolicy` の `match` | `parse_id_generator_policy`、単体テストの4つの文字列と不正な2つ |
| ID は `nextInt(65536) + Short.MinValue` | `IdGenerator.random` | 統合テスト2件、単体テストの変換（0、32768、65535 ほか） |
| ポリシーごとの生成器 | `IdGenerator.apply(policy)` | 単体テストの `policy()`、EnhancedDoubleHashRandom の拒否、別スレッドからの呼び出し、`Send + Sync` |

レビューで次を直した（`a41b70c`、`4f67deb`）。

| 指摘 | 対応 |
|:---|:---|
| `RandomIdGenerator::next_id` が、`new` で拒否済みの EnhancedDoubleHashRandom に対して、起きていない `EntropyUnavailable` を返す分岐を持っていた | 到達しない分岐として `unreachable!` にした |
| 単体テスト `the_secure_generator_varies_across_calls` が、64回の値が全部同じにならないことを確かめていた。乱数の値に依存するテストで、投入文と調整担当の決定に反する | 削除した |
| 定数 `THREAD_LOCAL_GAMMA` のコメントが「乗数」と書いていた（実際は増分） | 「増分」に直した |
| aislop が単体テストの区切りのコメント6件を指摘した | 削除した |

### 独立レビュー（別系統のモデル）

- 実施者: 実装者とは別系統の、読み取り専用のレビュー担当。モデルは Codex（OpenAI GPT-6.2 Sol、medium）で、調整担当が手配した。実装の2工程は DGX（`dgx/deepseek-v4.1-flash`）、自己レビューとレビュー対応は Claude Opus 5.5 なので、`docs/decisions/takt-workflow-design.md` §2.6 に従って別系統のレビューを足した。
- 対象: PR #157 の固定 HEAD `db279f3fd1f2abf811756c26a6c949a21da54cd8`。比較元は `b7b395495560591b0f84df0536eb4137fc11fb7e`。原典は Pekko 固定 SHA `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`。記録は [PR のコメント](https://github.com/j5ik2o/fraktor-ng-rs/pull/157#issuecomment-7143485210)にある。
- 判定: 指摘あり。原典12ケースの対応、評価の順序、既存の公開 API の不変、red の再実測（終了値101、E0432）、固定 HEAD での統合テストと workspace のテストは確認済みとされた。指摘は2件（中1、低1）。

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| 中: どの乱数源が選ばれたかをテストで区別できない。SecureRandom を thread-local の乱数源に誤配線しても、既存のテストは通る | 直す | `50d22a2`。`next_id` を、非公開の表 `DrawSources`（ポリシーごとの乱数源）を通す形にした。値を固定した別々の乱数源を渡す単体テストを3件足した。ThreadLocalRandom は thread-local の乱数源を読み、SecureRandom は secure の乱数源を読み、secure の乱数源の失敗は `EntropyUnavailable` のまま返って thread-local へ切り替わらないことを確かめる。SecureRandom を thread-local の乱数源に誤配線すると2件が失敗し、失敗時に thread-local へ切り替える変更を入れると1件が失敗することを確かめ、どちらも元に戻した。公開 API、kernel の no_std、原典2件のアサーションは変えていない |
| 低: `dns_settings_spec.rs` のコメントが、テストの既定値を「DNS の reference configuration を写したもの」と書いている。実際は spec 固有の fixture で、Pekko の `reference.conf` の既定値（`resolve-timeout = 5s`、`search-domains = default`、`ndots = default`、`id-generator-policy = enhanced-double-hash-random`）とは違う | 直す | `af56ec8`。コメントを spec の fixture の説明に直した。入力値、原典10ケース、アサーションは変えていない（差分はコメントだけ） |

## PR #157 のレビュー指摘への対応

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| Devin: TTL の変換が `TtlSetting` の値だけで決まるのに、型の外の自由関数にある（`fraktor-coding-rules.md` の rich-domain-model） | 直す | `7eef22e`。TTL に加えて、同じ形の nameservers、search-domains、ndots、ID ポリシーの変換も、それぞれの型の impl に移した。`DnsSettingsValues::new` には評価の順序とホストの読み取りだけを残した。振る舞いは変えておらず、既存の単体テスト143件と統合テスト12件はそのまま通る |
| Devin: 検索ドメインを明示し、ndots だけを既定にした組み合わせ（原典の "use host ndots if set to default"）で、ホストの ndots が返ることを確かめる単体テストがない | 直す | `152b7a0`。単体テスト `a_default_ndots_beside_explicit_search_domains_comes_from_the_host` を足した（search は明示の値、ndots はホストの 2、resolv.conf は1回だけ読む）。resolv.conf を読む条件から ndots を外すと、このテストが失敗することを確かめた。原典12件の統合テストとアサーションは変えていない |

Codex と CodeRabbit は指摘なしだった。

## 製品編集前の実測

第1工程の成果 `3460283753673e52865eb740aee0515182153107`（`32d869f` からの製品差分0。`git diff --stat 32d869f 3460283 -- 'crates/*/src' Cargo.toml Cargo.lock 'crates/*/Cargo.toml'` は空）で、担当が自分の作業ツリーで次を再実行した。テストの blob は、`id_generator_spec.rs` が `dc2b0e51`、`dns_settings_spec.rs` が `308ca154` である。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test id_generator_spec -- --test-threads=1` | 101 | compile-red。`E0432` が2件（`RandomIdGenerator`、`IdGeneratorPolicy` が未定義、L21）。宣言2件、実行0件、除外0件 |
| `cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1` | 101 | compile-red。`E0432` が1件（`CachePolicy` ほか10型が未定義、L33）。宣言10件、実行0件、除外0件 |
| `verify_red.py`（ノードごと。原典を `PEKKO_ORIGINAL_ROOT` に置いて実行） | 0 | 2ノードとも `expected_compile_error`、`excluded: 0`。台帳はすでに red で、書き換えなし |

already_green ではない。製品に ID 生成と設定値の型がなく、2本ともコンパイルに失敗した。第2工程で `id_generator_spec.rs` に import を1行足した後（blob `d5926600`）も、`6e3356c` の作業ツリーにそのファイルを置いて再実測し、`E0432` が3件（足した `IdGenerator` を含む）で compile-red になることを確かめた。`dns_settings_spec.rs` は、独立レビューの指摘でコメントだけを直した（blob `5fbb7fa6`）。これも `6e3356c` の作業ツリーに置いて再実測し、終了値101、`E0432` の1件で compile-red になることを確かめた。

## 実装後の実測

独立レビューの指摘に対応した後の HEAD（`af56ec8`）で、担当が自分の作業ツリーで次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test id_generator_spec -- --test-threads=1` | 0 | 2 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1` | 0 | 10 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 144 passed（PR の指摘への対応前は 143） |
| `cargo test -p fraktor-actor --lib random_id_generator` | 0 | 15 passed（独立レビューの対応前は 12） |
| `cargo test --workspace` | 0 | 71 スイート、772 passed / 0 failed / 0 ignored（PR の指摘への対応前は 768、独立レビューの対応前は 769） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0、抑制0 |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |
| `verify_green.py --crate fraktor-actor` | 0 | `all_pass: true` |

台帳は `G0issue154` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性は OK で、対象ノードはすべて green。`G0issue154` は2件で `{'green': 2}`）。

## 再現手順

```text
cargo test -p fraktor-actor --test id_generator_spec -- --test-threads=1
cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1
cargo test -p fraktor-actor --lib io::dns
cargo test -p fraktor-actor --lib random_id_generator
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue154
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue154
```

## 後続：#156 ホストの既定値を OS から読む adaptor

[Issue #156](https://github.com/j5ik2o/fraktor-ng-rs/issues/156)（Orca 項目236、[PR #158](https://github.com/j5ik2o/fraktor-ng-rs/pull/158)）は、上の設計判断 D3 で残した adaptor である。kernel の port `HostDnsConfiguration` を std で実装し、実機の OS から値を読む。原典のテストに対応しないので台帳のノードはなく、本ゴールの所属ノードと台帳は変えていない。記録は、D3 の続きとして本書に置く（`docs/goals/` の文書は1行目のゴール ID と台帳の所属を `goal_complete_gate_test.py` で照合するため、台帳のノードがない記録を単独の文書にしない）。

- 原典: [DnsSettings.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala)（`getDefaultNameServers`、`lazy val resolvConf`）と [ResolvConfParser.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/ResolvConfParser.scala)（`parseFile`）
- 原典が呼ぶ JDK の契約: [jdk21u の Unix 版 `sun.net.dns.ResolverConfigurationImpl`](https://github.com/openjdk/jdk21u/blob/master/src/java.base/unix/classes/sun/net/dns/ResolverConfigurationImpl.java)（`resolvconf("nameserver", 1, 5)`）と [`com.sun.jndi.dns.DnsContextFactory`](https://github.com/openjdk/jdk21u/blob/master/src/jdk.naming.dns/share/classes/com/sun/jndi/dns/DnsContextFactory.java)（`dns://` の URL に OS の name server を入れる処理）
- 範囲は、ホストの設定ファイルの読み取りだけ。名前解決、DNS の送受信、キャッシュ、resolver は対象外。OS の DNS 設定は読むだけで、書き換えない
- 実行経路: 小さな単独の後続なので、調整担当の指示で TAKT のキューには投入せず、Claude Opus 5.5 が隔離 worktree（ブランチ `j5ik2o/issue-156-host-dns`、起点 `f4f417d`）で直接実装した。着手時に、#156 と重複する issue と PR がないことを確かめた

### 原典の取得の意味

| 値 | 原典の経路 | 取得の意味 |
|:---|:---|:---|
| name server | `getDefaultNameServers`：JNDI の `dns://` → 失敗時は反射で `sun.net.dns.ResolverConfiguration` | Unix では、どちらの経路も JDK の `ResolverConfigurationImpl` が `/etc/resolv.conf` の `nameserver` 行を読んだ値になる。JNDI は各値を `dns://<値>` の URL にし、原典は URL の host と port（無ければ 53）から `InetSocketAddress` を作る。作れない値は捨てる |
| search、ndots | `lazy val resolvConf`：Windows なら `None`。`/etc/resolv.conf` が無ければ `None`。あれば `ResolvConfParser.parseFile`、失敗はエラーをログに出して `None` | 存在するファイルは、中身が空でも `ResolvConf(Nil, 1)` になる |

JDK の `resolvconf("nameserver", 1, 5)` は、`FileReader` と `BufferedReader.readLine` で1行ずつ読み、行頭が `nameserver` で直後が空白かタブの行だけを、最大5行まで数える。5行目を数えたら、それ以降は読まない。1行から読む値は最初の1つだけで、`#` か `;` で始まる値は捨てる。`.` を含まず `:` を含む値は、角括弧で囲んで IPv6 にする。`IOException` は読み取りのループの外で握りつぶすので、途中で読めなくなっても、それまでに読んだ値は返る（最初から開けなければ空のリスト）。JDK の `ResolverConfiguration.open()` はプロセスで1つのインスタンスで、読んだ name server を最後に読んでから5分間（`TIMEOUT = 300000` ミリ秒）キャッシュする。name server が1つもないと、JNDI は既定の `dns://` をそのまま返し、原典はその URL を捨てる。結果は空のリストになり、`"default"` の name server は `IllegalStateException`（"Unable to obtain default nameservers"）で失敗する。

原典の `asInetSocketAddress` は `new URI("dns://" + 値)` を作る。URI として不正な値は例外になり、原典はその値を捨てる。URL の authority は最初の `/`、`?`、`#` で終わり、その後ろは path、query、fragment として、`java.net.URI` が許す文字だけを持てる。そのため `nameserver 192.0.2.1#local` は、JDK が1つの値として残し、原典は URL の host `192.0.2.1` と port 53 を読む。一方、`192.0.2.1#a#b`（fragment に `#`）や `192.0.2.2#a|b`（許されない文字）は URI として不正なので捨てられる。authority の `@` より前は user information で、host には入らない。角括弧の IPv6 の後ろの `%` は scope ID で、`java.net.URI` は英数字と `_`、`.` を受け付け、`InetAddress` が数値か、インターフェース名として読む。角括弧なしの IPv6 に `#` が付くと、JDK が `#` ごと角括弧で囲むので、URL として読めずに捨てられる。

### 実装

adaptor に `SystemHostDnsConfiguration`（`crates/fraktor-actor/src/adaptor/system_host_dns_configuration.rs`）とエラー型 `UnsupportedHostDnsPlatformError` を足した。公開するのは、`/etc/resolv.conf` を読む `new() -> Result<Self, UnsupportedHostDnsPlatformError>` だけである。対象は `/etc/resolv.conf` に resolver の設定を置く OS（Linux、macOS、BSD など）に限る。Windows では `new()` が `UnsupportedHostDnsPlatformError` を返し、既定の name server を自動では取得しない（#160 で追跡）。Windows では、`nameservers` を `NameserversSetting::Single`・`List` で明示するか、独自の `HostDnsConfiguration` を `DnsSettingsValues::new` に渡す。この使い方は、公開 API の説明（型とエラー型の doc コメント）にも書いた。既存の `RandomIdGenerator::new` が未対応のポリシーを `UnsupportedIdGeneratorPolicyError` で返すのと同じ形で、既存の公開 API と port の契約は変えていない。テストで別のパスを読む `reading(path)` と、Windows かどうかを引数で受ける `for_platform(windows)` は `pub(crate)` にした。kernel の変更は、`HostDnsConfiguration` の doc コメント1行だけで（「実機を読む実装は範囲外」を「kernel の外に置く」に直した）、kernel は std を使わないまま。新しい依存はない。

| メソッド | 振る舞い |
|:---|:---|
| `name_servers()` | ファイルを1行ずつ読み、行ごとに UTF-8 として寛容に復号する（JDK の `FileReader` と同じく、不正なバイトは置き換える）。途中で I/O エラーになったら、そこで読むのをやめ、それまでの行を使う。5行目の `nameserver` 行の後は読まない。上の JDK の規則で `nameserver` の値を取り出し、`dns://` の URI として authority の終わりで切る。後ろの path、query、fragment が `java.net.URI` の許す文字でなければ捨てる。authority の user information を外し、host と port に分ける（port が無い、または `:` の後が空なら 53）。角括弧の IPv6 の scope ID は、`u32` に収まる数値なら保持し、インターフェース名や URI として不正なものは捨てる。host と port は #136 の `DnsSettings::parse_nameserver_address` で読み、読めない値は捨てる。ファイルを開けない、または使える値が1つもなければ `None` |
| `resolv_conf()` | ファイルを読み、UTF-8 として厳密に復号する（`Files.lines` と同じ）。#136 の `ResolvConfParser::parse_lines` で解析する。ファイルが無い、読めない、UTF-8 でない、解析に失敗した、のどれでも `None` |

行の区切りは、`BufferedReader.readLine` と同じく `\n`、`\r`、`\r\n` にした。`\r` と `\n` のそれぞれで区切ると空行が増えるが、どちらの読み手も空行を読み飛ばすので、結果は変わらない。ファイルは呼ぶたびに読み直す。`"default"` の解決と失敗の扱いは、既存の `DnsSettingsValues::new` に任せた（`None` と空のリストは、どちらも `UnableToDetermineDefaultNameservers` になる）。

### 原典・OS との差異

| 差異 | 理由 |
|:---|:---|
| Windows では既定の name server を自動で取得せず、`SystemHostDnsConfiguration::new()` が `UnsupportedHostDnsPlatformError` を返す。原典は Windows でも JDK 経由で（レジストリから）既定の name server を取得する。Windows で省くのは `resolvConf` だけである | 調整担当の決定（副元帥の回答 B）で、本 issue は resolv.conf の方式の対象 OS に限った。Windows の自動取得は [#160](https://github.com/j5ik2o/fraktor-ng-rs/issues/160) で追跡する。Windows では `nameservers` を明示するか、独自の `HostDnsConfiguration` を渡す。何も見つからない `None` ではなく、未対応と分かるエラーにした。原典のケースの除外は新設していない。#160 で Windows も IP Helper から取得するようにし、この差は解消した（下の「後続：#160」） |
| host 名の name server（例 `nameserver dns.example`）と、名前の scope ID つきの IPv6（例 `fe80::1%en0`）を捨てる。原典はどちらも `InetSocketAddress` にする。数値の scope ID（例 `fe80::1%2`）は原典と同じく保持する | #136 の `parse_nameserver_address` は IP リテラルだけを受け付け、名前解決をしない。名前解決は対象外。インターフェース名を番号に変えるには OS のインターフェース表が要る。数値の scope ID は、kernel のパーサーで読んだアドレスに adaptor で付ける（kernel と公開 API は変えない） |
| resolv.conf の読み取りと解析の失敗をログに出さない。原典は `log.error` に出してから `None` にする | port の `HostDnsConfiguration` は `ExtendedActorSystem` を受け取らず、出し先のアクターシステムがない（上の「原典との差異」のとおり）。値は原典と同じ `None` |
| 原典の name server は、JDK がプロセスで1つのキャッシュを持ち、最後に読んでから5分間は `/etc/resolv.conf` を読み直さない。そのため、ファイルを書き換えても5分以内に作った `DnsSettings` には古い name server が入る。本 adaptor は `name_servers()` を呼ぶたびに読み直すので、書き換えの後に作った `DnsSettingsValues` には新しい値が入る。search と ndots は、原典も生成ごとに読み直すので差はない | キャッシュはプロセスの状態で、テストで制御した入力の再現性を下げる。読み取り専用で、名前解決やキャッシュ本体は対象外。1回の `DnsSettingsValues::new` の中では name server を1回だけ読むので、生成1回の中の値は原典と変わらない |
| 途中で読めなくなったときの resolv.conf（search、ndots）は `None`、name server はそれまでに読んだ値 | 原典と同じ。`Files.lines` は途中の I/O エラーを `UncheckedIOException` にして `parseFile` が `Failure` を返し、JDK の `resolvconf` は途中までの値を返す |
| `options ndots:５` のような、ASCII 以外の十進数字の ndots を解析の失敗にする（search も既定値の空、ndots は 1 になる）。原典の `toInt` は受け付ける | 既存の `ResolvConfParser` の差で、[#159](https://github.com/j5ik2o/fraktor-ng-rs/issues/159) で追跡する。本 issue ではパーサーを変えない。#159 でパーサーを原典に合わせ、この差は解消した（[G0issue136](issue-136-dns-settings.md) の「後続：#159」） |
| `search corp.example # managed` の `#` と `managed` も search domain になる。FreeBSD と NetBSD のリゾルバーは、行の途中の `#` をコメントとして扱う | 原典の `ResolvConfParser` は行頭の `#` と `;` だけをコメントにし、#136 の `ResolvConfParser` も同じ（原典と一致させた既存のパーサー）。ホストの既定値は原典と同じ値を返すことを優先し、パーサーは変えない。この扱いはテストで固定した |

JDK が捨てる `#` か `;` で始まる値は、URI にする前に捨てる（その行も5行に数える）。当初は「authority の終わりで切れば host が IP リテラルにならない」として分岐を置かなかったが、`;user@192.0.2.1` は `;user` が URI の user information として読めるので誤りだった（PR #158 の CodeRabbit の指摘。後述）。`::ffff:192.0.2.9` のように `.` を含む角括弧なしの IPv6 は、URL の host として読めないので、原典と同じく捨てる。

### テスト

単体テスト40件（`system_host_dns_configuration_test.rs`）は、テストが作った一時ディレクトリのファイルか、テストの中のバイト列だけを読む。実機の `/etc/resolv.conf` は読まず、書き換えもしない。`new()` と `for_platform` のテストは、読むパスが `/etc/resolv.conf` であることと、Windows ではエラーになることを、値の比較で確かめるだけで、ファイルは開かない。一時ディレクトリは、ランダムな名前で排他的に作り（`DirBuilder::create`。既にあるパスは、誰が作ったものでも使わず、消さない）、Unix では所有者だけが入れる 0700 にする。Windows の分岐は `for_platform(true)` で、どの OS でも確かめる（CI は ubuntu のみ）。

| 観点 | テスト |
|:---|:---|
| port の既定 | IPv4 と角括弧なし IPv6 は 53。`:` の後が空でも 53。明示の port（IPv4、角括弧の IPv6）は保つ |
| JDK の行の規則 | ファイルの順序、1行に1値、`#`・`;` の値、最大5行（コメントの値の行も数える）、行頭の空白や `nameservers` の行は読まない、`\r\n` と `\r` の行末 |
| URL の authority | `192.0.2.1#local`、`192.0.2.2:5353#x`、`[2001:db8::1]#x`、`/` と `?` の後ろを切る。角括弧なし IPv6 の `#` は捨てる。`#local` つきの1行だけでも `"default"` の name server が決まる |
| 捨てる値 | host 名、名前の scope ID、`.` を含む角括弧なし IPv6、範囲外の port |
| scope ID | 数値の scope ID（`fe80::1%2`、`[fe80::2%3]:5353`）を保持する。空、URI に許されない文字、`u32` を超える値は捨てる |
| URI の妥当性 | `#a#b`、`#a\|b`、`?x^y`、`/a<b`、`#%zz` は捨て、`#local`、`/p;x?q=1#f?/`、`#%41`、`#café` は保つ。`user@192.0.2.1` は host を読み、不正な user information は捨てる |
| 途中の I/O エラー | 2行読んだ後に失敗する読み手で、2つの name server が返る。`\r` で終わった行の直後に失敗しても、その行は返り、終わっていない行は返らない（`\r\n` も同じ）。最初から失敗すると `None` |
| コメントで始まる値 | `;user@192.0.2.1` 型の値（`;` か `#` で始まる）は name server にならず、その行も5行に数える |
| 対象 OS | `for_platform(false)` は `/etc/resolv.conf` を読む。`for_platform(true)` は `UnsupportedHostDnsPlatformError`。`new()` は実行中の OS に従う |
| 空 | name server のない内容と空のファイルは、name server が `None`。空のファイルの resolv.conf は `ResolvConf([], 1)` |
| 読めない | ファイルが無い、ディレクトリ（存在するが読めない）は、どちらも `None`。不正な UTF-8 は、name server は読み、resolv.conf は `None`。`ndots:` の不正は `None` |
| search の行末 | `search corp.example # managed` は原典どおり3要素 |
| port の経由 | 既定値の `DnsSettingsConfig` で `DnsSettingsValues::new` を呼び、ファイルの name server・search・ndots が入ること。ファイルが無いと `UnableToDetermineDefaultNameservers`。解析できない resolv.conf では search が空、ndots が 1 |
| 読み直し | ファイルを書き換えると、次の呼び出しで新しい値が返る |
| 一時ディレクトリ | 2つの一時ディレクトリが別のパスで、既にあるパスへの作成は `AlreadyExists` で失敗し、Unix では 0700、片方の後始末がもう片方を消さない |

### 製品編集前の実測（red）

テストを先に書き、型の実装がない状態（コミット `7730264`。adaptor に空のモジュールだけを置き、再エクスポートはしない）で実行した。`cargo test -p fraktor-actor --lib system_host_dns_configuration` は終了値101の compile-red で、`E0432`（`super::SystemHostDnsConfiguration` が未定義、テストの L22）が1件だった。already_green ではない。aislop の指摘でテストの区切りのコメントを消した blob（`0ed04402`）でも、実装を外して同じ `E0432` と終了値101になることを確かめた。

実装後に変異を3つ入れた。最大行数を6にすると1件、既定の port を54にすると3件のテストが失敗した。独立レビューの指摘への対応（後述）では、先にテストを足した。`for_platform`、`read_name_servers`、`UnsupportedHostDnsPlatformError` がない状態では終了値101の compile-red（`E0432` が2件、`E0599` が3件）だった。読み取りの構造と Windows のエラーだけを入れた段階では、URI の妥当性、数値の scope ID、user information の3件が失敗した。そのうえで変異を4つ入れた。I/O エラーで止まらずに失敗させると3件、scope ID を付けないと1件、URI の後ろの検査を外すと1件、user information を外さないと1件のテストが失敗する。当初はコメントの値を捨てる分岐を消してもテストが失敗しなかったので分岐を削除したが、修正試行3で `;user@192.0.2.1` の反例が見つかり、テストを先に足して分岐を戻した（後述）。

### PR #158 のレビュー指摘への対応

| 指摘 | 分類 | 対応 |
|:---|:---|:---|
| CI: `goal_complete_gate_test.py` の `test_real_goal_documents_match_current_ledger` が、単独の文書 `docs/goals/issue-156-host-dns.md` の1行目を `# ゴール G0...：` として読めずに失敗した | 直す | 台帳のノードがない記録なので、単独の文書をやめ、D3 の続きとして本書のこの節に移した。テストの条件は変えていない |
| Devin（[r4686309417](https://github.com/j5ik2o/fraktor-ng-rs/pull/158#discussion_r4686309417)）: テストが、共有の一時領域の予測できるパスを、削除に失敗しても再利用する | 直す | 一時ディレクトリを、ランダムな名前で `DirBuilder::create` により排他的に作り、既にあれば別の名前で作り直す（既存のパスは使わず、消さない）。Unix では 0700 にした。この性質を確かめる単体テストを足した |
| Devin（[r4686309017](https://github.com/j5ik2o/fraktor-ng-rs/pull/158#discussion_r4686309017)）: `nameserver 192.0.2.1#local` が読み飛ばされる | 直す | JDK はこの値を1つの値として残し、原典は `dns://` の URL の host を読むので `192.0.2.1:53` になる（上の「原典の取得の意味」）。URL の authority の終わり（`/`、`?`、`#`）で切るようにした。先にテスト2件を足し、修正前に2件とも失敗することを確かめた |
| Devin（[r4686309234](https://github.com/j5ik2o/fraktor-ng-rs/pull/158#discussion_r4686309234)）: search の行末コメントの扱いが、Pekko と FreeBSD で違う | 直す（文書とテスト） | 原典と既存のパーサーは `#` 以降も search domain にする。原典との互換を優先してパーサーは変えず、OS との差を上の差異の表に書き、原典どおりの扱いをテストで固定した |

| 独立レビュー（GPT-6.2 Sol、固定 `b2104e0`、[REJECT の記録](https://github.com/j5ik2o/fraktor-ng-rs/pull/158#issuecomment-7144439886)）: 数値の scope ID つき IPv6 を捨てている | 直す | 原典の `new URI` と `InetAddress` は数値の scope ID を保持する。kernel のパーサーで読んだアドレスに adaptor で scope ID を付けた。名前の scope ID は捨てることを明記した。先に足したテストが修正前に失敗することを確かめた |
| 独立レビュー: URI として不正な nameserver を除外していない（`#` 以降を切るだけ） | 直す | `java.net.URI` の path、query、fragment、user information の文字の規則で検査し、不正な値は捨て、有効な行末コメントは保つ。先に足したテスト（不正な値の除外、user information）が修正前に失敗することを確かめた |
| 独立レビュー: 読み取り途中の I/O エラーで、取得済みの nameserver も捨てる | 直す | ファイルを1行ずつ読み、I/O エラーで止めて、それまでの値を使う（JDK の `resolvconf` の `IOException` の扱い）。制御した読み手でテストした。文書の「ファイルを読めないとき」の説明も直した |
| 独立レビュー: JDK のキャッシュとの差の説明が不正確 | 直す | プロセス内のキャッシュ（5分）と、呼ぶたびの読み直しで、生成をまたいで観測できる差があることに直した（上の差異の表） |
| 独立レビュー: 既存 `ResolvConfParser` の Unicode 十進数字の差 | 別件 | [#159](https://github.com/j5ik2o/fraktor-ng-rs/issues/159)。差異の表に、失敗時の扱いと一緒に記録した |
| 独立レビュー: Windows の既定 nameserver の取得が原典と違う | 直す（範囲の決定） | 副元帥の回答 B。本 issue は resolv.conf の方式の対象 OS に限り、Windows は `UnsupportedHostDnsPlatformError` で未対応と分かるようにして、テストで固定した。Windows の自動取得は [#160](https://github.com/j5ik2o/fraktor-ng-rs/issues/160) |

| CodeRabbit（[r4686751363](https://github.com/j5ik2o/fraktor-ng-rs/pull/158#discussion_r4686751363)）: `\r` で終わった行の直後に読み取りエラーになると、その行も捨てる | 直す | JDK の `BufferedReader.readLine` は `\r` を読んだ時点でその行を返し、`resolvconf` は後の `IOException` をループの外で握りつぶすので、原典は `192.0.2.1` を返す。本 adaptor は `\n` まで読みためていたので、行ごと失っていた。読み取りの段階で `\r` と `\n` のどちらでも行を終えるようにした。先に足したテスト（`\r` の後と `\r\n` の後の失敗）が修正前に失敗することを確かめた |
| CodeRabbit（[r4686751372](https://github.com/j5ik2o/fraktor-ng-rs/pull/158#discussion_r4686751372)）: `nameserver ;user@192.0.2.1` が name server になる | 直す | JDK の `resolvconf` は `#` か `;` で始まる値で行を終え、値を残さないので、原典はこの行から name server を取らない。本 adaptor は `;user` を URI の user information として外していた。URI にする前に `#` と `;` で始まる値を捨てるようにした（行は5行に数える）。先に足したテストが修正前に失敗することを確かめた |

CodeRabbit は当初は利用上限でレビューせず、`7849874` で上の2件を指摘した。Codex は `8671b47` で指摘なしだった。

### 実装後の実測

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --lib system_host_dns_configuration` | 0 | 40 passed |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 成功（DNS の既存の単体テスト） |
| `cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1` | 0 | 10 passed |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 0 | 成功 |
| `cargo test --workspace` | 0 | 失敗0（lib の単体テストは 400 → 440） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0、抑制0。`cfg-std-forbid`（kernel の std）と `port-adaptor-boundary` を含む |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |

台帳は `G0issue154` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性 OK、対象ノードはすべて green。`G0issue154` は `{'green': 2}`）。

```text
cargo test -p fraktor-actor --lib system_host_dns_configuration
cargo test --workspace
uv run --with pyyaml python3 ledger/tools/goal_complete_gate_test.py
```

## Pekko v2.0.0 との照合（2026-11-25 追記）

基準を Pekko v2.0.0（`9fbd2283b1cf701c90a1223bf621942c191db61f`）に切り替えた後に照合した。v1.7.1 は v2.0.0 の祖先ではないので、共通祖先 `0ca804c7af` から両方の系統を比べた。上の本文は v1.7.1 で照合したときの記録なので、そのまま残す。ここでは v2.0.0 の位置と判定だけを書く。台帳の `verified_against` は v2.0.0 にした（第3まとまり、`docs/baseline/pekko-v2.0.0-migration/batch-3.md`。第1・第2まとまりで扱ったノードは、それぞれの記録にある）。

- テストの変更は、`DnsSettingsSpec` が import の並びだけ、`IdGeneratorSpec` が import の書き方だけである。ケースと入力は同じである。
- 出典の更新：OS の設定を JDK から読む経路は、反射から method handle に変わった（`246fae4fb8`、`DnsSettings.scala` L174–175 の `publicLookup` と `lookup`。だめなら `privateLookupIn`）。失敗のメッセージには、modular な JDK では `java.base/sun.net.dns` へのアクセスが要る旨が足された（L165）。上の「反射」は、v2.0.0 では「JDK 内部の ResolverConfiguration（method handle）」と読む。
- 出典の更新：`random` の引数は `java.util.Random` から `java.util.random.RandomGenerator` に変わった（`f571b64f78` #3153、`IdGenerator.scala` L66）。`random(rand)` を移植しない判断は変わらない。
- 記録済みの差異（型付きの入力値、検査の時期、`next_id` の `Result`、`CachePolicy.Ttl` の一部、OS との差異）は維持する。

## 後続：#160 Windows の既定の name server を IP Helper から読む

[Issue #160](https://github.com/j5ik2o/fraktor-ng-rs/issues/160) は、上の「後続：#156」で Windows を未対応にした差を埋める。#156 と同じく原典のテストに対応しないので、台帳のノードはなく、本ゴールの所属ノードと台帳は変えていない。着手判断は元帥（2026-11-25）。TAKT は使わず、Claude Opus 5.5 がブランチ `j5ik2o/issue-160-windows-dns`（起点 `471d9d5`）で直接実装した。

- 原典: Pekko v2.0.0 の [DnsSettings.scala](https://github.com/apache/pekko/blob/9fbd2283b1cf701c90a1223bf621942c191db61f/actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala)（L109–112 の `resolvConf` は Windows で読まない。L199–269 の `getDefaultNameServers` は OS を問わず JNDI → JDK 内部の `ResolverConfiguration`）
- 原典が呼ぶ JDK の契約: [Windows 版 `sun.net.dns.ResolverConfigurationImpl`](https://github.com/openjdk/jdk/blob/master/src/java.base/windows/classes/sun/net/dns/ResolverConfigurationImpl.java)（`addressesToList`）と [その native 実装](https://github.com/openjdk/jdk/blob/master/src/java.base/windows/native/libnet/ResolverConfigurationImpl.c)（`getAdapters`、`loadConfig`、`strappend`）。JNDI の `dns://` は Unix と同じく、この name server を `dns://<値>` の URL にする
- 範囲は、name server の読み取りだけ。search の一覧（JDK は登録簿の `SearchList` とアダプターの `DnsSuffix` を読む）は、原典の `resolvConf` が Windows で `None` なので読まない。OS の設定は読むだけで、書き換えない

### JDK の Windows 版の取得の意味

`loadConfig` は `GetAdaptersAddresses(AF_UNSPEC, GAA_FLAG_SKIP_UNICAST | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_FRIENDLY_NAME, ...)` でアダプターの一覧を得る。バッファは 15360 バイトから始め、`ERROR_BUFFER_OVERFLOW` なら必要な大きさに 15360 を足して最大3回やり直す。`ERROR_NO_DATA` はアダプターなし（エラーではない）、ほかの失敗は例外になり、原典は name server を決められない。`OperStatus` が `IfOperStatusUp` のアダプターだけについて、DNS サーバーを順に読む。scope ID が 0 でない IPv6（link-local、site-local）は飛ばす。残りを `WSAAddressToStringA` で文字列にし（port が 0 なら port を書かない。0 でなければ書き、IPv6 は角括弧で囲む）、`strappend` でカンマ区切りの 1024 バイトのバッファに足す。入りきらない値は飛ばし、次の値を試す。Java 側の `addressesToList` は、`:` を含み `[` で始まらない値を角括弧で囲み、同じ値の2回目以降を捨てる。各値は、Unix と同じ `dns://` の URI として読まれる。

### 実装

| 部分 | 内容 |
|:---|:---|
| `SystemHostDnsConfiguration`（公開） | 中身を、読む先の列挙 `HostDnsSource`（非公開。`ResolvConf(PathBuf)` と `IpHelper`）にした。`new()` は Windows で `IpHelper`、それ以外で `/etc/resolv.conf` を読む。`resolv_conf()` は `IpHelper` で常に `None`。`name_servers()` は `IpHelper` で下の2つを組み合わせる |
| `WindowsAdapter`（`adaptor/windows_adapter.rs`、`pub(crate)`） | IP Helper の1アダプター（稼働中か、DNS サーバーの `SocketAddr` の列）。`WindowsAdapter::name_servers(Option<&[Self]>)` が上の JDK の規則（稼働中だけ、scope ID の除外、文字列化、1024 バイトの上限、角括弧、重複の除去）を純粋な関数として実装し、各値を #156 の `server_address`（`dns://` の URI と kernel のパーサー。adaptor の中で使えるよう `pub(super)` にした）で読む。`None`（読めなかった）と、使える値が1つもないときは `None` |
| `windows_ip_helper::read_adapters`（`adaptor/windows_ip_helper.rs`、Windows だけ） | OS を呼ぶ境界。`windows-sys` の `GetAdaptersAddresses` を JDK と同じフラグ、バッファの大きさ、やり直しの回数で呼び、連結リストを `WindowsAdapter` の列に写す。IPv4 と IPv6 以外の family は、`WSAAddressToStringA` が失敗して JDK が飛ばすので、写さない。Windows 以外では `None` を返す（呼ばれない） |

依存は `windows-sys` 0.61 を Windows の target だけに足した（`Win32_Foundation`、`Win32_NetworkManagement_IpHelper`、`Win32_NetworkManagement_Ndis`、`Win32_Networking_WinSock`）。Windows 以外のビルドは何も増えない。kernel は変えていない（std も依存も入れていない）。

候補の比較:

| 候補 | 判断 |
|:---|:---|
| `windows-sys` で `GetAdaptersAddresses` を呼ぶ | 採用。JDK と同じ API を同じフラグで呼べ、scope ID と port を含む `SOCKADDR` をそのまま読める。Microsoft が保守する生の束縛で、Windows の target にだけ入る |
| `ipconfig` クレート（hickory-resolver が使う） | 不採用。`Adapter::dns_servers()` が `IpAddr` しか返さず、JDK が使う scope ID と port が落ちる（0.3.4 の `src/adapter.rs` L97 で確認） |
| 登録簿（`Tcpip\Parameters\Interfaces` の `NameServer`、`DhcpNameServer`） | 不採用。現行の JDK の経路（`loadConfig` は登録簿から search の一覧だけを読む）と違い、稼働中かどうかの判定もない |
| `extern "system"` を手で書く | 不採用。`IP_ADAPTER_ADDRESSES_LH` の配置を手で写すと誤りやすい。依存が1つ減るだけ |

公開 API は変えていない。`new()` の `Result<Self, UnsupportedHostDnsPlatformError>` とエラー型は残した（Windows で `Err` を扱っていた呼び出し元がそのままコンパイルできる）。どの OS でも `Err` は返らなくなったので、エラー型の `pub(crate)` の生成関数は消し、doc コメントにその旨を書いた。エラー型を将来どうするか（非推奨にする、`new()` を `Result` でなくす）は、公開 API を壊す判断なので、本 issue では行わない。

### 原典・JDK との差異

| 差異 | 理由 |
|:---|:---|
| JDK は name server と search の一覧を、最後に読んでから120秒か、アドレスの変化の通知まで、プロセスでキャッシュする。本 adaptor は `name_servers()` を呼ぶたびに IP Helper を読む | #156 の resolv.conf と同じ扱い（キャッシュはプロセスの状態で、制御した入力の再現性を下げる）。1回の `DnsSettingsValues::new` の中では1回だけ読む |
| アドレスの文字列化は `WSAAddressToStringA` ではなく Rust の `Display` で行う | 値は直後に読み直すので、結果の `SocketAddr` は変わらない。1024 バイトの上限に数える長さは、正規形（RFC 5952）の IPv6 なら Windows と同じ |
| IP Helper が失敗したときの OS のエラーを出さない | #156 と同じく、port がアクターシステムを受け取らない。値は原典と同じく name server なし（`UnableToDetermineDefaultNameservers`） |

### テスト

純粋な規則は `windows_adapter_test.rs` の単体テスト9件で、テストが作ったアダプターの列を渡して確かめる（どの OS でも走る）。

| 観点 | テスト |
|:---|:---|
| 取得 | 稼働中のアダプターの DNS サーバーが、アダプターと一覧の順に、port 0 なら 53 で返る |
| 稼働中だけ | 稼働中でないアダプターの DNS サーバーは飛ばす |
| scope ID | scope ID のある IPv6 は飛ばし、ないものは残す |
| 重複 | 同じ値の2回目以降は捨てる（アダプターをまたいでも） |
| port | 0 でない port は IPv6 では残る。IPv4 は `[192.0.2.1:5353]` になり URI として不正なので、原典と同じく捨てる |
| IPv4 射影 | `::ffff:192.0.2.1` は `192.0.2.1:53`（`InetAddress` と kernel のパーサーの `to_canonical`） |
| 上限 | 39文字の IPv6 は25個まで入り、26個目からは飛ばし、後の短い IPv4 は入る |
| 空 | アダプターなし（`ERROR_NO_DATA`）、DNS サーバーなし、稼働中のものなし、はどれも `None` |
| 失敗 | アダプターを読めない（`GetAdaptersAddresses` の失敗）は `None` |

`system_host_dns_configuration_test.rs` は、`for_platform(true)` が IP Helper を読む構成で `resolv_conf()` が `None` になること、`new()` が `Ok(for_platform(cfg!(windows)))` であることに直した（#156 の `windows_is_reported_as_unsupported` は置き換えた）。実機の Windows では `#[cfg(windows)]` のテストが、IP Helper が答えること（`read_adapters()` が `Some`）と、返る name server に scope ID がないことを確かめる。実機の値そのものはネットワークによるので比べない。

CI に Windows のジョブ（`windows-latest`）を足し、`cargo test -p fraktor-actor --lib adaptor::system_host_dns_configuration` と `cargo clippy -p fraktor-actor --all-targets -- -D warnings` を走らせる。ジョブは `ci.yml` の中にあるので、`Main PR CI` の `Fraktor CI Gate`（`needs.ci.result`）の対象になる。Windows のジョブで見つかった既存の問題（テストの補助関数の `unused_mut`。`mode` を Unix でだけ呼ぶため）も直した。

### 製品編集前の実測（red）

テストだけを足したコミット `a0c3c30`（`WindowsAdapter` の型がなく、`for_platform` は `Result` のまま）で、`cargo test -p fraktor-actor --lib system_host_dns_configuration` は終了値101の compile-red だった（`E0432` が1件：`super::WindowsAdapter`。`E0599` が2件：`ip_helper` と `Result` の `resolv_conf`。`E0308` が2件）。already_green ではない。

実装後に変異を7つ入れ、どれも `windows_adapter` のテストが失敗した：稼働中の絞り込みを外す（2件）、scope ID の除外を外す、重複の除去を外す、1024 バイトの上限を外す、入りきらない値で止める（飛ばさずに）、port 0 でも port を書く、`[` で始まる値も角括弧で囲む（各1件）。

### 実装後の実測

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --lib system_host_dns_configuration` | 0 | 52 passed（`windows_adapter` の9件を含む。`#[cfg(windows)]` の1件は macOS では数えない） |
| `cargo clippy -p fraktor-actor --all-targets -- -D warnings`（macOS） | 0 | 警告なし |
| `cargo clippy --target x86_64-pc-windows-msvc -p fraktor-actor --all-targets -- -D warnings`（macOS からの検査） | 0 | 警告なし。`windows-sys` の型とフィールドの名前がコンパイルで合うことも確かめた |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0、抑制0。新しい2つのモジュールは `adaptor` の直下に置いた（`system_host_dns_configuration` の子にすると、`module-wiring` が葉でないモジュールからの再エクスポートとして拒む） |

Windows の実機での実行は、CI の Windows ジョブで確かめる。手元の macOS では、OS の呼び出しの部分（`windows_ip_helper::read_adapters` の Windows 版）は検査だけで、実行していない。
