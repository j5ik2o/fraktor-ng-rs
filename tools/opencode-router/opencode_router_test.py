#!/usr/bin/env python3
"""opencode_router.py の振り分けを、固定の応答で確かめる。

使い方:
    python3 tools/opencode-router/opencode_router_test.py

/metrics の応答はローカルの小さな HTTP サーバで与え、本物の opencode の代わりに
「受け取った引数・環境・標準入力を書き出す偽コマンド」を OPENCODE_ROUTER_REAL_OPENCODE で
指す。次のケースを確かめる。

  1. 空いている                       waiting 0、しきい値 0 で DGX のまま起動する
  2. 混んでいる                       waiting 3、しきい値 0 で dgx の接続先を Cloud に向ける
  3. しきい値を変える                 -1 なら waiting 0 でも Cloud、5 なら waiting 3 で DGX。
                                      未設定なら既定値で判定し、整数でなければ DGX
  4. 判定に失敗した                   接続できない・時間切れ・形式が違う、いずれも DGX
  5. --version                        本物と同じ出力・終了コード。metrics を読まず、記録もしない
  6. 引数・標準入出力・終了コード・PID  そのまま本物に渡り、exec でプロセスが置き換わる
  7. 記録                            1 行ずつ追記され、環境変数で場所を変えられる。失敗しても起動する
  8. 秘密                             OPENCODE_PASSWORD も content の apiKey も記録に出さない
"""
import json
import os
import re
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import traceback
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROUTER = Path(__file__).resolve().parent / "opencode_router.py"

# 欠陥のある実装は /metrics を待ち続けたり、偽 opencode を起動したまま戻らなかったりし得る。
TEST_TIMEOUT_SECONDS = 30

TARGET_MODEL = "deepseek-v4.1-flash"
CLOUD_MODEL_ID = "deepseek-v4.1-flash:cloud"
CLOUD_BASE_URL = "http://localhost:11434/v1"
DGX_BASE_URL = "http://10.0.1.60:8000/v1"
REAL_VERSION = "opencode v2.2.5"
FAKE_EXIT_CODE = 7

# TAKT の runtime.js が --version の出力に使う正規表現（runtime.js:24）。
TAKT_VERSION_PATTERN = re.compile(r"^(?:opencode v)?(\d+)\.\d+\.\d+(?:[-+][\w.-]+)?$")


class TestTimeout(Exception):
    pass


def _on_timeout(signum, frame):
    raise TestTimeout()


def waiting_line(value: str, model: str = TARGET_MODEL) -> str:
    return f'vllm:num_requests_waiting{{engine="0",model_name="{model}"}} {value}'


IDLE_METRICS = "\n".join(
    [
        "# HELP vllm:num_requests_waiting Number of waiting requests",
        "# TYPE vllm:num_requests_waiting gauge",
        waiting_line("0.0"),
        f'vllm:num_requests_running{{engine="0",model_name="{TARGET_MODEL}"}} 1.0',
        "",
    ]
)

BUSY_METRICS = "\n".join(
    [
        waiting_line("3.0"),
        f'vllm:num_requests_running{{engine="0",model_name="{TARGET_MODEL}"}} 16.0',
        "",
    ]
)

# 対象の系列が無く、コメント行・別モデル・名前が前方一致するだけのメトリクスだけを返す。
NO_TARGET_METRICS = "\n".join(
    [
        "# HELP vllm:num_requests_waiting 5",
        'vllm:num_requests_waiting{engine="0",model_name="other-model"} 9.0',
        'vllm:num_requests_waiting_by_reason{engine="0",model_name="deepseek-v4.1-flash"} 7.0',
        "",
    ]
)

# 対象の系列はあるが値が数値でない。
NON_NUMERIC_METRICS = waiting_line("abc") + "\n"


def takt_config_content() -> dict:
    """TAKT の v2 設定に似せた最小の content（provider は opencode の鍵だけ）。"""
    return {
        "model": f"dgx/{TARGET_MODEL}",
        "agents": {"build": {"model": f"dgx/{TARGET_MODEL}"}},
        "plugins": ["/tmp/example-plugin.js"],
        "permissions": {"external_directory": "deny"},
        "provider": {"opencode": {"options": {"apiKey": "sk-test"}}},
    }


class MetricsServer:
    """固定の本文を返す /metrics（ポート 0 で開き、テストごとに分ける）。"""

    def __init__(self, body: str | None, delay: float = 0.0) -> None:
        self.body = body
        self.delay = delay
        self.requests = 0
        server = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                server.requests += 1
                if server.delay:
                    time.sleep(server.delay)
                payload = (server.body or "").encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/plain; version=0.0.4")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def log_message(self, format, *args) -> None:
                pass

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    @property
    def url(self) -> str:
        host, port = self.httpd.server_address[:2]
        return f"http://{host}:{port}/metrics"

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *exc):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)
        return False


def unused_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


FAKE_OPENCODE_SOURCE = """\
import json, os, sys
out = os.environ["FAKE_OPENCODE_OUT"]
record = {
    "argv": sys.argv[1:],
    "pid": os.getpid(),
    "config_content": os.environ.get("OPENCODE_CONFIG_CONTENT"),
    "stdin": sys.stdin.read() if not sys.stdin.isatty() else "",
}
with open(out, "w", encoding="utf-8") as handle:
    json.dump(record, handle)
# 実際の opencode の C ラッパーは argv0 に本物のパスを渡すので、その次の要素で判定する。
if record["argv"][1:] == ["--version"]:
    sys.stdout.write(os.environ.get("FAKE_OPENCODE_VERSION", "") )
    sys.exit(int(os.environ.get("FAKE_OPENCODE_VERSION_EXIT", "0")))
sys.stdout.write(record["stdin"])
sys.exit(int(os.environ.get("FAKE_OPENCODE_EXIT", "0")))
"""

# 本物の /Users/j5ik2o/.nix-profile/bin/opencode と同じ形の C ラッパー（argv0 を引き継いで
# 別のファイルを実行する）。ここでは Python で代わりに、argv0 をそのまま引数として渡す。
REAL_SHIM_SOURCE = """\
#!{python}
import os, sys
target = {fake!r}
os.execv(sys.executable, [sys.executable, target, sys.argv[0], *sys.argv[1:]])
"""


class RouterCase:
    """1 回のラッパー起動に使う、一時ディレクトリ・偽 opencode・環境をまとめる。"""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.fake = root / "fake-opencode.py"
        self.fake.write_text(FAKE_OPENCODE_SOURCE, encoding="utf-8")
        self.real = root / "real-opencode"
        self.real.write_text(REAL_SHIM_SOURCE.format(python=sys.executable, fake=str(self.fake)), encoding="utf-8")
        self.real.chmod(0o755)
        self.output = root / "fake-output.json"
        self.log = root / "route.log"

    def env(self, metrics_url: str, overrides: dict[str, str | None] | None = None) -> dict:
        values = {key: value for key, value in os.environ.items() if not key.startswith("OPENCODE_")}
        values.update(
            {
                "OPENCODE_ROUTER_REAL_OPENCODE": str(self.real),
                "OPENCODE_ROUTER_METRICS_URL": metrics_url,
                "OPENCODE_ROUTER_LOG": str(self.log),
                "FAKE_OPENCODE_OUT": str(self.output),
                "FAKE_OPENCODE_VERSION": REAL_VERSION + "\n",
                "FAKE_OPENCODE_EXIT": str(FAKE_EXIT_CODE),
            }
        )
        for key, value in (overrides or {}).items():
            if value is None:
                values.pop(key, None)
            else:
                values[key] = value
        return values

    def run(self, env: dict, argv: list[str], stdin: str = "") -> subprocess.CompletedProcess:
        completed = subprocess.run(
            [sys.executable, str(ROUTER), *argv],
            input=stdin,
            capture_output=True,
            text=True,
            env=env,
            timeout=TEST_TIMEOUT_SECONDS,
        )
        return completed

    def fake_record(self) -> dict:
        assert self.output.exists(), "the fake opencode was never executed"
        return json.loads(self.output.read_text(encoding="utf-8"))

    def records(self) -> list[dict]:
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text(encoding="utf-8").splitlines() if line.strip()]


class PopenCase(RouterCase):
    """PID を観測するため、stdin を閉じずに起動する。"""

    def popen(self, env: dict, argv: list[str]) -> subprocess.Popen:
        return subprocess.Popen(
            [sys.executable, str(ROUTER), *argv],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )


def cloud_enabled_provider(content: dict) -> dict:
    provider = content.get("provider", {})
    dgx = provider.get("dgx")
    assert isinstance(dgx, dict), f"provider.dgx is missing from the content passed to opencode: {content}"
    assert dgx.get("options", {}).get("baseURL") == CLOUD_BASE_URL, dgx
    models = dgx.get("models", {})
    for name in (TARGET_MODEL, f"{TARGET_MODEL}-high"):
        assert models.get(name, {}).get("id") == CLOUD_MODEL_ID, f"{name} was not moved to Cloud: {models}"
    return dgx


def test_idle_keeps_dgx() -> None:
    serve_argv = ["serve", "--hostname=127.0.0.1", "--port=4321"]
    with tempfile.TemporaryDirectory() as directory, MetricsServer(IDLE_METRICS) as server:
        case = RouterCase(Path(directory))
        original = takt_config_content()
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(original)})
        completed = case.run(env, serve_argv)

        assert completed.returncode == FAKE_EXIT_CODE, completed.stderr
        record = case.fake_record()
        passed = json.loads(record["config_content"])
        assert passed == original, f"idle routing must not touch the content, got {passed}"
        assert record["argv"][1:] == serve_argv, record["argv"]

        (entry,) = case.records()
        assert entry["route"] == "dgx", entry
        assert entry["waiting"] == 0, entry
        assert entry["threshold"] == 0, entry
        assert entry["reason"] == "below_or_equal_threshold", entry


def test_busy_moves_dgx_to_cloud() -> None:
    with tempfile.TemporaryDirectory() as directory, MetricsServer(BUSY_METRICS) as server:
        case = RouterCase(Path(directory))
        original = takt_config_content()
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(original)})
        completed = case.run(env, ["serve", "--hostname=127.0.0.1", "--port=4321"])

        assert completed.returncode == FAKE_EXIT_CODE, completed.stderr
        record = case.fake_record()
        passed = json.loads(record["config_content"])

        dgx = cloud_enabled_provider(passed)
        assert dgx.get("options", {}).get("apiKey") is None, "the apiKey must not be copied into the override"
        assert passed["model"] == original["model"], passed["model"]
        for key in ("agents", "plugins", "permissions"):
            assert passed[key] == original[key], f"{key} must survive the override"
        assert passed["provider"]["opencode"] == original["provider"]["opencode"], passed["provider"]
        assert "ollama" not in passed["provider"], "only provider.dgx may be rewritten"

        (entry,) = case.records()
        assert entry["route"] == "cloud", entry
        assert entry["waiting"] == 3, entry
        assert entry["reason"] == "above_threshold", entry


def test_busy_without_provider_key_still_moves_dgx_to_cloud() -> None:
    # TAKT の v2 content は provider を持たない（apiKey があるときだけ providers.opencode が入る）。
    # 実運用の形では provider が無いので、その場合も dgx を足して Cloud へ向くことを確かめる。
    with tempfile.TemporaryDirectory() as directory, MetricsServer(BUSY_METRICS) as server:
        case = RouterCase(Path(directory))
        original = {
            "model": f"dgx/{TARGET_MODEL}",
            "agents": {"build": {"model": f"dgx/{TARGET_MODEL}"}},
            "plugins": [],
            "permissions": {"external_directory": "deny"},
        }
        assert "provider" not in original
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(original)})
        completed = case.run(env, ["serve", "--port=1"])

        assert completed.returncode == FAKE_EXIT_CODE, completed.stderr
        passed = json.loads(case.fake_record()["config_content"])
        cloud_enabled_provider(passed)
        assert sorted(passed["provider"]) == ["dgx"], passed["provider"]
        for key in ("model", "agents", "plugins", "permissions"):
            assert passed[key] == original[key], f"{key} must survive the override"

        (entry,) = case.records()
        assert (entry["route"], entry["reason"]) == ("cloud", "above_threshold"), entry


def test_threshold_boundary_and_override() -> None:
    with tempfile.TemporaryDirectory() as directory:
        case = RouterCase(Path(directory))
        original = json.dumps(takt_config_content())

        with MetricsServer(IDLE_METRICS) as server:
            env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": original, "OPENCODE_ROUTER_THRESHOLD": "-1"})
            case.run(env, ["serve", "--port=1"])
            cloud_enabled_provider(json.loads(case.fake_record()["config_content"]))
            (entry,) = case.records()
            assert (entry["route"], entry["reason"], entry["threshold"]) == ("cloud", "above_threshold", -1), entry

        case.log.unlink()
        with MetricsServer(BUSY_METRICS) as server:
            env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": original, "OPENCODE_ROUTER_THRESHOLD": "5"})
            case.run(env, ["serve", "--port=1"])
            assert json.loads(case.fake_record()["config_content"]) == json.loads(original)
            (entry,) = case.records()
            assert (entry["route"], entry["reason"], entry["threshold"]) == ("dgx", "below_or_equal_threshold", 5), entry

        case.log.unlink()
        with MetricsServer(BUSY_METRICS) as server:
            env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": original, "OPENCODE_ROUTER_THRESHOLD": "3"})
            case.run(env, ["serve", "--port=1"])
            assert json.loads(case.fake_record()["config_content"]) == json.loads(original), "waiting == threshold must stay on DGX"
            (entry,) = case.records()
            assert (entry["route"], entry["reason"]) == ("dgx", "below_or_equal_threshold"), entry

        case.log.unlink()
        with MetricsServer(BUSY_METRICS) as server:
            env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": original, "OPENCODE_ROUTER_THRESHOLD": "lots"})
            completed = case.run(env, ["serve", "--port=1"])
            assert completed.returncode == FAKE_EXIT_CODE, completed.stderr
            assert json.loads(case.fake_record()["config_content"]) == json.loads(original)
            (entry,) = case.records()
            assert (entry["route"], entry["reason"]) == ("dgx", "invalid_threshold"), entry


def test_failures_fall_open_to_dgx() -> None:
    cases = [
        ("connect_error", f"http://127.0.0.1:{unused_port()}/metrics", None, {}),
        ("timeout", None, 2.0, {"OPENCODE_ROUTER_METRICS_TIMEOUT": "0.3"}),
        ("format_error", None, 0.0, {}),
    ]
    for reason, url, delay, overrides in cases:
        with tempfile.TemporaryDirectory() as directory:
            body = NO_TARGET_METRICS if reason == "format_error" else BUSY_METRICS
            server = MetricsServer(body, delay=delay or 0.0)
            with server:
                case = RouterCase(Path(directory))
                original = takt_config_content()
                metrics_url = url or server.url
                env = case.env(metrics_url, {"OPENCODE_CONFIG_CONTENT": json.dumps(original), **overrides})
                completed = case.run(env, ["serve", "--port=1"])

            assert completed.returncode == FAKE_EXIT_CODE, f"{reason}: {completed.stderr}"
            assert json.loads(case.fake_record()["config_content"]) == original, reason
            (entry,) = case.records()
            assert entry["route"] == "dgx", f"{reason}: {entry}"
            assert entry["reason"] == reason, f"{reason}: {entry}"
            assert entry["waiting"] is None, f"{reason}: {entry}"

    with tempfile.TemporaryDirectory() as directory, MetricsServer(NON_NUMERIC_METRICS) as server:
        case = RouterCase(Path(directory))
        original = takt_config_content()
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(original)})
        case.run(env, ["serve", "--port=1"])
        assert json.loads(case.fake_record()["config_content"]) == original
        (entry,) = case.records()
        assert (entry["route"], entry["reason"]) == ("dgx", "format_error"), entry


def test_version_is_passed_through_without_probe() -> None:
    with tempfile.TemporaryDirectory() as directory, MetricsServer(BUSY_METRICS) as server:
        case = RouterCase(Path(directory))
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(takt_config_content())})
        completed = case.run(env, ["--version"])

        assert completed.returncode == 0, completed.stderr
        assert completed.stdout == REAL_VERSION + "\n", repr(completed.stdout)
        assert TAKT_VERSION_PATTERN.match(completed.stdout.strip()), "TAKT would reject this version string"
        assert TAKT_VERSION_PATTERN.match(completed.stdout.strip()).group(1) == "2"

        record = case.fake_record()
        assert record["argv"][1:] == ["--version"], record["argv"]
        assert server.requests == 0, "the router must not probe /metrics for --version"
        assert case.records() == [], "the router must not record a routing decision for --version"


def test_argv_stdio_exit_code_and_exec_are_preserved() -> None:
    with tempfile.TemporaryDirectory() as directory, MetricsServer(IDLE_METRICS) as server:
        case = PopenCase(Path(directory))
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(takt_config_content())})
        argv = ["serve", "--hostname=127.0.0.1", "--port=1234"]
        process = case.popen(env, argv)
        stdout, stderr = process.communicate(input="hello from stdin", timeout=TEST_TIMEOUT_SECONDS)

        assert process.returncode == FAKE_EXIT_CODE, stderr
        assert stdout == "hello from stdin", f"stdout must be the fake opencode's, got {stdout!r}"
        record = case.fake_record()
        assert record["pid"] == process.pid, "the router must exec, not spawn a child"
        assert record["argv"][1:] == argv, record["argv"]
        assert record["stdin"] == "hello from stdin", record["stdin"]


def test_records_append_and_location_is_configurable() -> None:
    with tempfile.TemporaryDirectory() as directory, MetricsServer(IDLE_METRICS) as server:
        case = RouterCase(Path(directory))
        custom_log = Path(directory) / "nested" / "custom.log"
        env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(takt_config_content()), "OPENCODE_ROUTER_LOG": str(custom_log)})
        case.run(env, ["serve", "--port=1"])
        case.run(env, ["serve", "--port=1"])

        assert not case.log.exists(), "the default log location must not be used when OPENCODE_ROUTER_LOG is set"
        lines = [json.loads(line) for line in custom_log.read_text(encoding="utf-8").splitlines() if line.strip()]
        assert len(lines) == 2, f"two launches must append two lines, got {lines}"
        for entry in lines:
            assert set(entry) == {"time", "waiting", "threshold", "route", "reason"}, entry
            assert entry["time"], entry
            assert entry["route"] == "dgx", entry


def test_unwritable_log_does_not_block_startup() -> None:
    with tempfile.TemporaryDirectory() as directory, MetricsServer(IDLE_METRICS) as server:
        case = RouterCase(Path(directory))
        blocker = Path(directory) / "blocker"
        blocker.write_text("not a directory", encoding="utf-8")
        env = case.env(
            server.url,
            {"OPENCODE_CONFIG_CONTENT": json.dumps(takt_config_content()), "OPENCODE_ROUTER_LOG": str(blocker / "route.log")},
        )
        completed = case.run(env, ["serve", "--port=1"])

        assert completed.returncode == FAKE_EXIT_CODE, "a failing log must not stop the launch"
        assert completed.stdout == "", "the router must keep stdout clean for TAKT's 'server listening' line"
        assert case.fake_record()["argv"][1:] == ["serve", "--port=1"]


def test_secrets_never_reach_the_log() -> None:
    with tempfile.TemporaryDirectory() as directory, MetricsServer(BUSY_METRICS) as server:
        case = RouterCase(Path(directory))
        content = takt_config_content()
        content["provider"]["opencode"]["options"]["apiKey"] = "sk-test-secret-value"
        env = case.env(
            server.url,
            {
                "OPENCODE_CONFIG_CONTENT": json.dumps(content),
                "OPENCODE_PASSWORD": "router-password-secret",
            },
        )
        completed = case.run(env, ["serve", "--port=1"])

        assert completed.returncode == FAKE_EXIT_CODE, completed.stderr
        text = case.log.read_text(encoding="utf-8")
        for secret in ("sk-test-secret-value", "router-password-secret"):
            assert secret not in text, f"{secret} leaked into the route log"
        for entry in case.records():
            assert all(secret not in json.dumps(entry) for secret in ("sk-test", "router-password")), entry
        assert "router-password-secret" not in completed.stdout + completed.stderr


def test_non_routed_models_are_passed_through_without_probe() -> None:
    # SCN-RES-1-N1/N2: 名前に deepseek-v4.1-flash を含んでも、Cloud のモデルや別系統は対象外。
    # SCN-RES-1-N3: content のない起動も対象外。いずれも /metrics を読まず、記録もしない。
    cases = [
        ("ollama-flash-cloud", {**takt_config_content(), "model": "ollama/deepseek-v4.1-flash:cloud"}),
        ("ollama-strong", {**takt_config_content(), "model": "ollama/glm-5.3:cloud"}),
    ]
    for label, content in cases:
        content.pop("provider", None)
        with tempfile.TemporaryDirectory() as directory, MetricsServer(BUSY_METRICS) as server:
            case = RouterCase(Path(directory))
            env = case.env(server.url, {"OPENCODE_CONFIG_CONTENT": json.dumps(content), "OPENCODE_ROUTER_THRESHOLD": "0"})
            completed = case.run(env, ["serve", "--port=1"])

            assert completed.returncode == FAKE_EXIT_CODE, f"{label}: {completed.stderr}"
            assert json.loads(case.fake_record()["config_content"]) == content, f"{label}: content must be untouched"
            assert case.fake_record()["argv"][1:] == ["serve", "--port=1"], label
            assert server.requests == 0, f"{label}: must not probe /metrics"
            assert case.records() == [], f"{label}: must not record"

    with tempfile.TemporaryDirectory() as directory, MetricsServer(BUSY_METRICS) as server:
        case = RouterCase(Path(directory))
        env = case.env(server.url, {"OPENCODE_ROUTER_THRESHOLD": "-1"})
        env.pop("OPENCODE_CONFIG_CONTENT", None)
        completed = case.run(env, ["serve", "--port=1"])

        assert completed.returncode == FAKE_EXIT_CODE, completed.stderr
        passed = case.fake_record()["config_content"]
        assert passed is None, f"expanded content must stay absent, got {passed!r}"
        assert server.requests == 0, "content-less launch must not probe /metrics"
        assert case.records() == [], "content-less launch must not record"


def main() -> int:
    tests = [
        test_idle_keeps_dgx,
        test_busy_moves_dgx_to_cloud,
        test_busy_without_provider_key_still_moves_dgx_to_cloud,
        test_non_routed_models_are_passed_through_without_probe,
        test_threshold_boundary_and_override,
        test_failures_fall_open_to_dgx,
        test_version_is_passed_through_without_probe,
        test_argv_stdio_exit_code_and_exec_are_preserved,
        test_records_append_and_location_is_configurable,
        test_unwritable_log_does_not_block_startup,
        test_secrets_never_reach_the_log,
    ]
    failures = 0
    for test in tests:
        # 欠陥のある実装は待ち続けたり子プロセスを残したりし得る。テスト自体が固まらないよう上限を置く。
        signal.signal(signal.SIGALRM, _on_timeout)
        signal.alarm(TEST_TIMEOUT_SECONDS)
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {test.__name__}: {error}")
        except TestTimeout:
            failures += 1
            print(f"FAIL {test.__name__}: did not finish within {TEST_TIMEOUT_SECONDS}s")
        except Exception:  # 未実装（存在しない）や想定外は、失敗として記録する
            failures += 1
            print(f"FAIL {test.__name__}:")
            traceback.print_exc()
        else:
            print(f"ok   {test.__name__}")
        finally:
            signal.alarm(0)
    print(f"{len(tests) - failures} passed, {failures} failed")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
