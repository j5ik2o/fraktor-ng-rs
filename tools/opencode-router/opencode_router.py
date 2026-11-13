#!/usr/bin/env python3
"""DGX の混雑に応じて opencode の dgx 接続先を Ollama Cloud へ切り替えるラッパー。

TAKT は opencode を `TAKT_OPENCODE_PATH` から起動し、まず `--version` を呼び、次に
`serve --hostname=127.0.0.1 --port=N` を起動する。TAKT は子の PID に SIGTERM/SIGKILL を
送って止めるため、このラッパーは本物に exec で置き換わる。

起動のたびに DGX vLLM の /metrics から `vllm:num_requests_waiting` を読み、対象モデル
`deepseek-v4.1-flash` の系列の値がしきい値を超えていれば、`OPENCODE_CONFIG_CONTENT` の
`provider.dgx` だけを Ollama Cloud の接続先へ差し替えてから exec する。値がしきい値以下か、
判定に失敗したときは、TAKT が渡した設定をそのまま使う（fail-open）。モデルの指定は変えない。

振り分けるのは DeepSeek V4.1 Flash 用のサーバーだけである。content の `model` が
`dgx/deepseek-v4.1-flash` か `dgx/deepseek-v4.1-flash-high` のときに限り、/metrics の判定、
設定の差し替え、記録を行う。それ以外のモデル（`ollama/*` など）や content のない起動は、
記録もせずに本物へそのまま exec する。

`--version` は metrics を読まず、記録もせず、そのまま本物に exec する。

振り分けの結果は 1 行 1 JSON で記録へ追記する。秘密の値（環境変数、設定の本文、例外の
全文）は記録にも標準出力にも出さない。標準出力は TAKT が `server listening` を待つため空に保つ。

環境変数（既定値）:
  OPENCODE_ROUTER_METRICS_URL      http://10.0.1.60:8000/metrics
  OPENCODE_ROUTER_THRESHOLD        0（整数。負の値も可。超えたら Cloud）
  OPENCODE_ROUTER_METRICS_TIMEOUT  1.5（秒）
  OPENCODE_ROUTER_LOG              ${XDG_STATE_HOME:-$HOME/.local/state}/opencode-router/route.log
  OPENCODE_ROUTER_REAL_OPENCODE    $HOME/.nix-profile/bin/opencode
"""
import json
import math
import os
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone

METRICS_URL_ENV = "OPENCODE_ROUTER_METRICS_URL"
THRESHOLD_ENV = "OPENCODE_ROUTER_THRESHOLD"
METRICS_TIMEOUT_ENV = "OPENCODE_ROUTER_METRICS_TIMEOUT"
LOG_ENV = "OPENCODE_ROUTER_LOG"
REAL_OPENCODE_ENV = "OPENCODE_ROUTER_REAL_OPENCODE"

DEFAULT_METRICS_URL = "http://10.0.1.60:8000/metrics"
DEFAULT_THRESHOLD = 0
DEFAULT_METRICS_TIMEOUT_SECONDS = 1.5

# /metrics は小さな prometheus 本文だけを返す。読み過ぎないための取得上限。
MAX_METRICS_BYTES = 1 << 20

TARGET_MODEL = "deepseek-v4.1-flash"
WAITING_METRIC = "vllm:num_requests_waiting"
MODEL_LABEL = 'model_name="' + TARGET_MODEL + '"'

CLOUD_BASE_URL = "http://localhost:11434/v1"
CLOUD_MODEL_ID = "deepseek-v4.1-flash:cloud"

# 振り分けの対象は dgx provider の下のモデルだけ（どちらも DeepSeek V4.1 Flash）。
# TAKT は content の model を `${providerID}/${modelID}` の形で入れる（providerModel.js の整え方）。
DGX_PROVIDER_ID = "dgx"
DGX_MODEL_KEYS = (TARGET_MODEL, f"{TARGET_MODEL}-high")
ROUTED_MODELS = frozenset(f"{DGX_PROVIDER_ID}/{key}" for key in DGX_MODEL_KEYS)

REASON_BELOW = "below_or_equal_threshold"
REASON_ABOVE = "above_threshold"
REASON_CONNECT_ERROR = "connect_error"
REASON_TIMEOUT = "timeout"
REASON_FORMAT_ERROR = "format_error"
REASON_INVALID_THRESHOLD = "invalid_threshold"
REASON_INVALID_CONFIG = "invalid_config_content"


def real_opencode_path() -> str:
    configured = os.environ.get(REAL_OPENCODE_ENV)
    if configured:
        return configured
    return os.path.join(os.path.expanduser("~"), ".nix-profile", "bin", "opencode")


def parse_threshold(raw: str | None) -> int | None:
    if raw is None:
        return DEFAULT_THRESHOLD
    try:
        return int(raw)
    except ValueError:
        return None


def load_config_content(raw: str | None) -> dict | None:
    """content を読む。無い・壊れている・オブジェクトでないときは None（対象外か不正の区別は呼び出し側）。"""
    if raw is None:
        return None
    try:
        content = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(content, dict):
        return None
    return content


def is_routed_model(content: dict | None) -> bool:
    if content is None:
        return False
    model = content.get("model")
    return isinstance(model, str) and model in ROUTED_MODELS


def parse_waiting(body: str) -> tuple[str | None, float | None]:
    """対象モデルの waiting 系列を合計する。当てはまる系列が無い、または値が不正なら format_error。"""
    total = 0.0
    found = False
    for raw_line in body.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        name, brace, rest = line.partition("{")
        if not brace or name.strip() != WAITING_METRIC:
            continue
        labels, close, value_text = rest.partition("}")
        if not close or MODEL_LABEL not in labels:
            continue
        try:
            value = float(value_text.strip())
        except ValueError:
            return REASON_FORMAT_ERROR, None
        if not math.isfinite(value):
            return REASON_FORMAT_ERROR, None
        total += value
        found = True
    if not found:
        return REASON_FORMAT_ERROR, None
    return None, total


def probe_waiting(url: str, timeout: float) -> tuple[str | None, float | None]:
    try:
        with urllib.request.urlopen(url, timeout=timeout) as response:
            body = response.read(MAX_METRICS_BYTES).decode("utf-8", "replace")
    except urllib.error.URLError as error:
        if isinstance(error.reason, TimeoutError):
            return REASON_TIMEOUT, None
        return REASON_CONNECT_ERROR, None
    except TimeoutError:
        return REASON_TIMEOUT, None
    except OSError:
        return REASON_CONNECT_ERROR, None
    return parse_waiting(body)


def decide() -> tuple[str, float | None, object, str]:
    """route / waiting / threshold / reason を 1 か所で決める。記録と env はこの結果だけから作る。"""
    raw_threshold = os.environ.get(THRESHOLD_ENV)
    threshold = parse_threshold(raw_threshold)
    if threshold is None:
        return "dgx", None, raw_threshold, REASON_INVALID_THRESHOLD

    url = os.environ.get(METRICS_URL_ENV, DEFAULT_METRICS_URL)
    timeout = float(os.environ.get(METRICS_TIMEOUT_ENV, str(DEFAULT_METRICS_TIMEOUT_SECONDS)))
    reason, waiting = probe_waiting(url, timeout)
    if reason is not None:
        return "dgx", None, threshold, reason
    if waiting > threshold:
        return "cloud", waiting, threshold, REASON_ABOVE
    return "dgx", waiting, threshold, REASON_BELOW


def dgx_cloud_definition() -> dict:
    # 利用者の opencode.json は実行時に読まない（秘密を含むため）。dgx の定義はここに写しで持つ。
    models = {
        TARGET_MODEL: {
            "name": "DeepSeek V4.1 Flash 1017 (DGX Spark x4, vLLM)",
            "tools": True,
            "limit": {"context": 1048576, "output": 32768},
            "id": CLOUD_MODEL_ID,
        },
        f"{TARGET_MODEL}-high": {
            "id": CLOUD_MODEL_ID,
            "name": "DeepSeek V4.1 Flash 1017 High (DGX Spark x4, vLLM)",
            "tools": True,
            "limit": {"context": 1048576, "output": 32768},
            "options": {"reasoningEffort": "high"},
        },
    }
    return {
        "npm": "@ai-sdk/openai-compatible",
        "name": "DGX Spark x4 (vLLM, routed to Ollama Cloud)",
        "options": {"baseURL": CLOUD_BASE_URL},
        "models": models,
    }


def build_env(route: str, content: dict) -> tuple[dict, str | None]:
    """cloud のとき content に provider.dgx を足した env を返す。失敗時は書き換え前の写しを返す。"""
    env = dict(os.environ)
    if route != "cloud":
        return env, None

    provider = content.get("provider")
    if provider is None:
        provider = {}
    if not isinstance(provider, dict):
        return env, REASON_INVALID_CONFIG

    routed = {**content, "provider": {**provider, "dgx": dgx_cloud_definition()}}
    env["OPENCODE_CONFIG_CONTENT"] = json.dumps(routed)
    return env, None


def route_log_path() -> str:
    configured = os.environ.get(LOG_ENV)
    if configured:
        return configured
    state_home = os.environ.get("XDG_STATE_HOME") or os.path.join(os.path.expanduser("~"), ".local", "state")
    return os.path.join(state_home, "opencode-router", "route.log")


def append_record(route: str, waiting: float | None, threshold: object, reason: str) -> None:
    entry = {
        "time": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "waiting": waiting,
        "threshold": threshold,
        "route": route,
        "reason": reason,
    }
    path = route_log_path()
    try:
        parent = os.path.dirname(path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry) + "\n")
    except OSError:
        # 記録は補助情報であり、失敗しても起動を止めない。TAKT は stdout を待つので stderr にだけ出す。
        sys.stderr.write("opencode-router: could not append the route log\n")


def main(argv: list[str]) -> None:
    real = real_opencode_path()
    if real == os.path.abspath(argv[0]):
        sys.stderr.write("opencode-router: refusing to exec itself; set OPENCODE_ROUTER_REAL_OPENCODE\n")
        raise SystemExit(1)

    if argv[1:] == ["--version"]:
        os.execv(real, [real, "--version"])

    content = load_config_content(os.environ.get("OPENCODE_CONFIG_CONTENT"))
    if not is_routed_model(content):
        # 対象外のモデル（ollama/* など）と content のない起動は、判定も記録もせずそのまま渡す。
        os.execve(real, [real, *argv[1:]], dict(os.environ))

    route, waiting, threshold, reason = decide()
    env, override_reason = build_env(route, content)
    if override_reason is not None:
        route = "dgx"
        reason = override_reason
    append_record(route, waiting, threshold, reason)
    os.execve(real, [real, *argv[1:]], env)


if __name__ == "__main__":
    main(sys.argv)
