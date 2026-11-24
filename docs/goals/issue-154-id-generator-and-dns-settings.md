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
- adaptor に `RandomIdGenerator` と `UnsupportedIdGeneratorPolicyError` を足した。ID は原典の `(nextInt(65536) + Short.MinValue).toShort` と同じく、0〜65535 の値 `x` から `x − 32768` にする。ThreadLocalRandom は、Java の `ThreadLocalRandom` と同じ SplitMix 系の状態（増分 `0x9E37_79B9_7F4A_7C15` と `mix32`）をスレッドごとに持ち、種は std の `RandomState` から作る。SecureRandom は、呼ぶたびに `getrandom::fill` で2バイトを読み、失敗は `IdGenerationError::EntropyUnavailable` で返す。
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

PR 作成後、既出の指摘に対応した HEAD を固定して行う（`docs/decisions/takt-workflow-design.md` §2.6）。結果はここに追記する。

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

already_green ではない。製品に ID 生成と設定値の型がなく、2本ともコンパイルに失敗した。第2工程で `id_generator_spec.rs` に import を1行足した後（blob `d5926600`）も、`6e3356c` の作業ツリーにそのファイルを置いて再実測し、`E0432` が3件（足した `IdGenerator` を含む）で compile-red になることを確かめた。`dns_settings_spec.rs` は第2工程以降も変えていない。

## 実装後の実測

PR #157 の指摘に対応した後の HEAD（`152b7a0`）で、担当が自分の作業ツリーで次を実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --test id_generator_spec -- --test-threads=1` | 0 | 2 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1` | 0 | 10 passed / 0 failed / 0 ignored |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 144 passed（PR の指摘への対応前は 143） |
| `cargo test -p fraktor-actor --lib random_id_generator` | 0 | 12 passed |
| `cargo test --workspace` | 0 | 71 スイート、769 passed / 0 failed / 0 ignored（対応前は 768） |
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
