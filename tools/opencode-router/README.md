# opencode-router

TAKT が `TAKT_OPENCODE_PATH` から起動する opencode の前に置く、薄いラッパーです。

起動のたびに DGX vLLM の `/metrics` を読み、対象モデル `deepseek-v4.1-flash` の
`vllm:num_requests_waiting` がしきい値を超えていれば、provider `dgx` の接続先だけを
Ollama Cloud（ローカルの ollama プロキシ `http://localhost:11434/v1`）へ差し替えて、
本物の opencode に exec で置き換わります。モデルの指定 `dgx/deepseek-v4.1-flash` は変えません。
しきい値以下、または判定に失敗したときは、TAKT が渡した設定のまま DGX で起動します（fail-open）。

振り分けるのは DeepSeek V4.1 Flash 用のサーバーだけです。TAKT が設定に入れる `model` が
`dgx/deepseek-v4.1-flash` か `dgx/deepseek-v4.1-flash-high` のときに限り判定と記録を行い、
それ以外のモデル（`ollama/*` など）や `model` のない起動は、記録もせずにそのまま渡します。

## TAKT の起動時の設定

```sh
export TAKT_OPENCODE_PATH="$PWD/tools/opencode-router/opencode_router.py"
export TAKT_OPENCODE_VERSION=v2
takt ...
```

- `TAKT_OPENCODE_VERSION=v2` は、本物の opencode が 2.x（手元は 2.2.5）であるためです。
  TAKT は `--version` の出力の major とこの値が一致することを求めます。
- `.takt/runtime.yaml` と `.takt/config.yaml` は変更しません。この 2 つの環境変数を
  takt の起動時に設定して指定します。
- ラッパーは本物を `$HOME/.nix-profile/bin/opencode` から探します。別の場所にある場合は
  `OPENCODE_ROUTER_REAL_OPENCODE` で指定してください。

## 差し替えの方式とその根拠

opencode 2.2.5 は、設定を `OPENCODE_CONFIG`（ファイル）と `OPENCODE_CONFIG_CONTENT`
（JSON 文字列。仮想の文書として読む）から読みます。TAKT の `serve` は設定を
`OPENCODE_CONFIG_CONTENT` に入れて起動するので、このラッパーはその JSON を読み、
`provider.dgx` の鍵だけを Ollama Cloud の定義（`options.baseURL` と、2 つのモデルの `id` を
`deepseek-v4.1-flash:cloud` にしたもの）で置き換えて、同じ環境変数に入れ直してから exec します。

- 一時ファイルを作らないので、exec の後に後片付けが残りません。
- 利用者の `~/.config/opencode/opencode.json`（秘密を含む）は読みません。dgx の定義は
  ラッパー内の定数に持っています（`limit` と `tools` はその設定と同じ値）。
- `model`、`agents`、`plugins`、`permissions`、`provider` のほかの鍵は、そのまま残ります。
- baseURL は provider 単位なので、`deepseek-v4.1-flash` と `deepseek-v4.1-flash-high` の
  両方の `id` を Cloud のものに変えます。high の `options.reasoningEffort: "high"` は残します。

## しきい値と記録先の変え方

| 項目 | 環境変数 | 既定値 |
|------|----------|--------|
| metrics の URL | `OPENCODE_ROUTER_METRICS_URL` | `http://10.0.1.60:8000/metrics` |
| しきい値 | `OPENCODE_ROUTER_THRESHOLD` | `0`（整数。負の値も可。`waiting > しきい値` で Cloud） |
| 時間制限（秒） | `OPENCODE_ROUTER_METRICS_TIMEOUT` | `1.5`（正の数。数値でない・0 以下は `invalid_timeout` で DGX） |
| 記録先 | `OPENCODE_ROUTER_LOG` | `${XDG_STATE_HOME:-$HOME/.local/state}/opencode-router/route.log` |
| 本物の opencode | `OPENCODE_ROUTER_REAL_OPENCODE` | `$HOME/.nix-profile/bin/opencode` |

しきい値の既定値を `0` にした根拠は、`vllm:num_requests_waiting` が「実行枠が埋まって
順番を待っている要求の数」であり、1 件でも待ちがあれば新しい要求はすぐには処理されない
ためです。2026-11-13 の実測は waiting 0 / running 1 でした。整数でない値を設定した場合は
DGX で起動し、記録の reason を `invalid_threshold` にします。時間制限に数値でない値や 0 以下の値を
設定した場合も DGX で起動し、reason を `invalid_timeout` にします。`/metrics` の URL が
空文字列やスキームの無い値で取得できない場合も、DGX で起動し reason を `connect_error` にします。

記録は 1 行 1 JSON で、`time`（ISO8601 UTC）、`waiting`、`threshold`、`route`
（`dgx` か `cloud`）、`reason` を持ちます。reason は `below_or_equal_threshold`、
`above_threshold`、`connect_error`、`timeout`、`format_error`、`invalid_threshold`、
`invalid_timeout`、`invalid_config_content` のいずれかです。秘密の値（`OPENCODE_PASSWORD`、設定の本文、
API キー）は記録しません。記録に失敗しても起動は止めません。

## 実測の手順

手で起動するときは、ラッパーが対象モデルを見分けられるように `OPENCODE_CONFIG_CONTENT` に
`model` を入れます。これを省くと対象外の起動になるため、判定も記録もされません。

```sh
export OPENCODE_CONFIG_CONTENT='{"model":"dgx/deepseek-v4.1-flash"}'
```

### 空いているとき（DGX のまま起動する）

```sh
cd .   # リポジトリの根
tools/opencode-router/opencode_router.py --version     # 本物と同じ "opencode v2.2.5" が出る
OPENCODE_CONFIG_CONTENT='{"model":"dgx/deepseek-v4.1-flash"}' \
OPENCODE_ROUTER_LOG=/tmp/route-idle.log \
  tools/opencode-router/opencode_router.py serve --hostname=127.0.0.1 --port=4096 &
sleep 2 && curl -s http://127.0.0.1:4096/models | head -c 200; kill %1
cat /tmp/route-idle.log                                 # route は "dgx"
```

`--version` を本物と比べるときは `diff <(tools/opencode-router/opencode_router.py --version) \
<(opencode --version)` を使います。TAKT の正規表現
`^(?:opencode v)?(\d+)\.\d+\.\d+(?:[-+][\w.-]+)?$` に合い、major が 2 であることも確かめます。

### 混んでいる状態を作るとき（Cloud へ切り替える）

DGX を実際に混ませなくても、しきい値を 0 未満にすると必ず Cloud 側になります。

```sh
OPENCODE_CONFIG_CONTENT='{"model":"dgx/deepseek-v4.1-flash"}' \
OPENCODE_ROUTER_THRESHOLD=-1 OPENCODE_ROUTER_LOG=/tmp/route-busy.log \
  tools/opencode-router/opencode_router.py serve --hostname=127.0.0.1 --port=4097 &
sleep 2 && kill %1
cat /tmp/route-busy.log                                 # route は "cloud"、reason は "above_threshold"
```

### 対象外のモデルがそのまま渡ることの確認

```sh
OPENCODE_CONFIG_CONTENT='{"model":"ollama/glm-5.3:cloud"}' \
OPENCODE_ROUTER_THRESHOLD=-1 OPENCODE_ROUTER_LOG=/tmp/route-other.log \
  tools/opencode-router/opencode_router.py serve --hostname=127.0.0.1 --port=4098 &
sleep 2 && kill %1
test ! -e /tmp/route-other.log && echo "no routing record (as expected)"
```

しきい値を `0` のまま実際の混雑で試すときは、DGX で `vllm` の待ち行列ができるだけの要求を
流してから、同じコマンドを実行し、記録の `waiting` が `0` を超えていることを確かめます。

## 制約

- 判定はサーバの起動時に一度だけ行います。起動後に混雑が変わっても切り替え直しません
  （TAKT はサーバをモデルなどを鍵に共有し、その寿命の間は起動時の設定のまま使うため）。
- provider `dgx` の下のモデルは、`deepseek-v4.1-flash` と `deepseek-v4.1-flash-high` の
  両方が Cloud 側のモデル ID になります（どちらも DeepSeek V4.1 Flash）。
- 振り分けの対象は DeepSeek V4.1 Flash だけです。`ollama/*` や別の provider で直接指定した
  モデルの経路は変えず、`model` が対象の 2 つ以外の起動では `/metrics` を読まず記録もしません。
  `model` を含まない起動（手動の起動など）も同じく対象外です。

