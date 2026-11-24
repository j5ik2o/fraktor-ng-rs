# Issue #156：DNS 設定のホスト既定値を OS から読む adaptor

[Issue #156](https://github.com/j5ik2o/fraktor-ng-rs/issues/156)（Orca 項目236）は、[G0issue154](issue-154-id-generator-and-dns-settings.md) の設計判断 D3 で残した adaptor である。#154・#155 は、DNS 設定の値 `"default"` が使うホストの既定値（name server、search、ndots）を kernel の port `HostDnsConfiguration` から受け取る形にした。本 issue は、その port を std で実装し、実機の OS から値を読む。

- 原典: Pekko v1.7.1（固定 SHA `9a54d1294c3ac7821bbed3c940769c3d5027d1ff`）の [DnsSettings.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/DnsSettings.scala)（`getDefaultNameServers`、`lazy val resolvConf`）と [ResolvConfParser.scala](https://github.com/apache/pekko/blob/9a54d1294c3ac7821bbed3c940769c3d5027d1ff/actor/src/main/scala/org/apache/pekko/io/dns/internal/ResolvConfParser.scala)（`parseFile`）
- 原典が呼ぶ JDK の契約: [jdk21u の Unix 版 `sun.net.dns.ResolverConfigurationImpl`](https://github.com/openjdk/jdk21u/blob/master/src/java.base/unix/classes/sun/net/dns/ResolverConfigurationImpl.java)（`resolvconf("nameserver", 1, 5)`）と [`com.sun.jndi.dns.DnsContextFactory`](https://github.com/openjdk/jdk21u/blob/master/src/jdk.naming.dns/share/classes/com/sun/jndi/dns/DnsContextFactory.java)（`dns://` の URL に OS の name server を入れる処理）

範囲は、ホストの設定ファイルを読み取ることだけである。名前解決、DNS の送受信、キャッシュ、resolver は対象外。OS の DNS 設定は読むだけで、書き換えない。台帳のノードはない（原典のテストに対応しない adaptor のため）。

## 実行経路

小さな単独の後続なので、調整担当の指示で共有の TAKT キューには投入せず、Claude Opus 5.5 が隔離 worktree（ブランチ `j5ik2o/issue-156-host-dns`、起点は main の `f4f417d`）で直接実装した。着手時に、#156 と重複する issue と PR がないことを確かめた。

## 原典の取得の意味

| 値 | 原典の経路 | 取得の意味 |
|:---|:---|:---|
| name server | `getDefaultNameServers`：JNDI の `dns://` → 失敗時は反射で `sun.net.dns.ResolverConfiguration` | Unix では、どちらの経路も JDK の `ResolverConfigurationImpl` が `/etc/resolv.conf` の `nameserver` 行を読んだ値になる。JNDI は各値を `dns://<値>` の URL にし、原典は URL の host と port（無ければ 53）から `InetSocketAddress` を作る。作れない値は捨てる |
| search、ndots | `lazy val resolvConf`：Windows なら `None`。`/etc/resolv.conf` が無ければ `None`。あれば `ResolvConfParser.parseFile`、失敗はエラーをログに出して `None` | 存在するファイルは、中身が空でも `ResolvConf(Nil, 1)` になる |

JDK の `resolvconf("nameserver", 1, 5)` の規則は次のとおり。行頭が `nameserver` で、直後が空白かタブの行だけを数え、最大5行まで読む。1行から読む値は最初の1つだけで、`#` か `;` で始まる値は捨てる。`.` を含まず `:` を含む値は、角括弧で囲んで IPv6 にする。ファイルを読めないときは、`IOException` を握りつぶして空のリストを返す。name server が1つもないと、JNDI は既定の `dns://` をそのまま返す。原典はその URL を捨てるので、結果は空のリストになり、`"default"` の name server は `IllegalStateException`（"Unable to obtain default nameservers"）で失敗する。

## 実装

adaptor に `SystemHostDnsConfiguration`（`crates/fraktor-actor/src/adaptor/system_host_dns_configuration.rs`）を足した。kernel の `HostDnsConfiguration` を実装する。公開するのは、`/etc/resolv.conf` を読む `new()` と `Default` だけである。テストで使う、別のパスを読む `reading(path)` は `pub(crate)` にした。kernel の変更は、`HostDnsConfiguration` の doc コメント1行だけである（「実機を読む実装は範囲外」を「kernel の外に置く」に直した）。kernel は std を使わないまま。

| メソッド | 振る舞い |
|:---|:---|
| `name_servers()` | ファイルを読んで UTF-8 として寛容に復号し（JDK の `FileReader` と同じく、不正なバイトは置き換える）、上の JDK の規則で `nameserver` の値を取り出す。各値を `dns://` の URL として host と port に分ける（port が無い、または `:` の後が空なら 53）。その host と port を、#136 の `DnsSettings::parse_nameserver_address` で読む。読めない値は捨てる。ファイルを読めない、または使える値が1つもなければ `None` |
| `resolv_conf()` | ファイルを読み、UTF-8 として厳密に復号する（`Files.lines` と同じ）。#136 の `ResolvConfParser::parse_lines` で解析する。ファイルが無い、読めない、UTF-8 でない、解析に失敗した、のどれでも `None` |

行の区切りは、`BufferedReader.readLine` と同じく `\n`、`\r`、`\r\n` にした。`\r` と `\n` のそれぞれで区切ると空行が増えるが、どちらの読み手も空行を読み飛ばすので、結果は変わらない。ファイルは呼ぶたびに読み直す。Windows では、2つのメソッドとも `None` を返す。

既存の境界は次のとおり再利用した。name server の文字列は `DnsSettings::parse_nameserver_address`、resolv.conf は `ResolvConfParser::parse_lines`、`"default"` の解決と失敗の扱いは `DnsSettingsValues::new` に任せた（`None` と空のリストは、どちらも `UnableToDetermineDefaultNameservers` になる）。新しい依存は足していない。

## 原典との差異

| 差異 | 理由 |
|:---|:---|
| Windows では name server も `None` にした。原典の JDK は、Windows ではレジストリから読む | 本 issue は `/etc/resolv.conf` の読み取りが範囲。Windows では `nameservers` を明示すればよい。`"default"` は `UnableToDetermineDefaultNameservers` で失敗する |
| host 名の name server（例 `nameserver dns.example`）と、IPv6 のゾーン ID つき（例 `fe80::1%en0`）を捨てる。原典はどちらも `InetSocketAddress` にする | #136 の `parse_nameserver_address` は IP リテラルだけを受け付け、名前解決をしない（G0issue154 と #136 の決定）。名前解決は対象外。ゾーン名をインターフェース番号に変えるには OS の名前引きが要る |
| resolv.conf の読み取りと解析の失敗をログに出さない。原典は `log.error` に出してから `None` にする | port の `HostDnsConfiguration` は `ExtendedActorSystem` を受け取らず、出し先のアクターシステムがない（G0issue154 の差異の記録どおり）。値は原典と同じ `None` |
| JDK の `ResolverConfigurationImpl` は、読んだ値を5分間キャッシュする。本 adaptor は呼ぶたびに読み直す | 原典の `DnsSettings` は、生成ごとに name server を1回、resolv.conf を最大1回読む。`DnsSettingsValues::new` も同じなので、1回の生成の中で観測できる値は変わらない |

JDK が捨てる `#` か `;` で始まる値は、IP リテラルにならないので、本 adaptor では `parse_nameserver_address` が捨てる。そのための分岐は置いていない（分岐を消しても、全テストの結果が変わらないことを確かめた）。`::ffff:192.0.2.9` のように `.` を含む角括弧なしの IPv6 は、URL の host として読めないので、原典と同じく捨てる。

## テスト

単体テスト25件（`system_host_dns_configuration_test.rs`）は、どれも一時ディレクトリに書いたファイルだけを読む。実機の `/etc/resolv.conf` は読まず、書き換えもしない。`new()` のテストは、読むパスが `/etc/resolv.conf` であることを値の比較で確かめるだけで、ファイルは開かない。Windows ではテストのモジュールごと外す（CI は ubuntu のみ）。

| 観点 | テスト |
|:---|:---|
| port の既定 | IPv4 と角括弧なし IPv6 は 53。`:` の後が空でも 53。明示の port（IPv4、角括弧の IPv6）は保つ |
| JDK の行の規則 | ファイルの順序、1行に1値、`#`・`;` の値、最大5行（コメントの値の行も数える）、行頭の空白や `nameservers` の行は読まない、`\r\n` と `\r` の行末 |
| 捨てる値 | host 名、ゾーン ID、`.` を含む角括弧なし IPv6、範囲外の port |
| 空 | name server のない内容と空のファイルは name server が `None`。空のファイルの resolv.conf は `ResolvConf([], 1)` |
| 読めない | ファイルが無い、ディレクトリ（存在するが読めない）は、どちらも `None`。不正な UTF-8 は、name server は読み、resolv.conf は `None`。`ndots:` の不正は `None` |
| port の経由 | 既定値の `DnsSettingsConfig` で `DnsSettingsValues::new` を呼び、ファイルの name server・search・ndots が入ること。ファイルが無いと `UnableToDetermineDefaultNameservers`。解析できない resolv.conf では search が空、ndots が 1 |
| 読み直し | ファイルを書き換えると、次の呼び出しで新しい値が返る |

### 製品編集前の実測（red）

テストを先に書き、型の実装がない状態（コミット `7730264`。adaptor に空のモジュールだけを置き、再エクスポートはしない）で実行した。

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --lib system_host_dns_configuration` | 101 | compile-red。`E0432`（`super::SystemHostDnsConfiguration` が未定義、テストの L22）が1件 |

already_green ではない。adaptor に `HostDnsConfiguration` の実装がなかった。テストの区切りのコメントを aislop の指摘で消した後の最終の blob（`0ed04402`）でも、実装を外して同じ `E0432` と終了値101になることを確かめた。

実装後に、変異を3つ入れて確かめた。最大行数を6にすると1件、既定の port を54にすると3件のテストが失敗する。コメントの値を捨てる分岐を消してもテストは失敗しなかったので、上に書いたとおり分岐ごと削除した。

### 実装後の実測

| コマンド | 終了値 | 結果 |
|:---|---:|:---|
| `cargo test -p fraktor-actor --lib system_host_dns_configuration` | 0 | 25 passed |
| `cargo test -p fraktor-actor --lib io::dns` | 0 | 成功（DNS の既存の単体テスト） |
| `cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1` | 0 | 10 passed |
| `cargo test -p fraktor-actor --test resolv_conf_parser_spec -- --test-threads=1` | 0 | 成功 |
| `cargo test --workspace` | 0 | 960 passed / 0 failed（lib の単体テストは 400 → 425） |
| `cargo fmt --check` | 0 | 差分なし |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 警告なし |
| syn-check（`crates/fraktor-actor`、`crates/fraktor-testkit`） | 0 | 両方 `clean: true`、違反0、抑制0。`cfg-std-forbid`（kernel の std）と `port-adaptor-boundary` を含む |
| `npm ci` → `npm run check:aislop` | 0 | warn 以上の指摘は受理済み一覧と一致 |
| `goal_complete_gate_test.py`、`verify_red_test.py`、`verify_green_test.py`、`verify_green_test_aislop.py`、`verify_green_test_crate.py` | 0 | 成功 |

台帳は `G0issue154` と `G0a`〜`G0f` の各々で `check_needs.py` と `rollup.py` を実行し、14コマンドすべて終了値0だった（needs の閉性 OK、対象ノードはすべて green。`G0issue154` は `{'green': 2}`）。台帳は変えていない。

## 再現手順

```text
cargo test -p fraktor-actor --lib system_host_dns_configuration
cargo test -p fraktor-actor --lib io::dns
cargo test -p fraktor-actor --test dns_settings_spec -- --test-threads=1
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
uv run --with pyyaml python3 ledger/tools/check_needs.py --ledger ledger/nodes.yaml --capabilities ledger/capabilities.yaml --goal G0issue154
uv run --with pyyaml python3 ledger/tools/rollup.py --ledger ledger/nodes.yaml --goal G0issue154
```
