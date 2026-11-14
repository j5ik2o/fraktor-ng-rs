// scripts/check-aislop.mjs の照合契約を、偽の aislop 実行物で確かめる。
//
// 使い方:
//     node --test scripts/check-aislop.test.mjs
//
// check-aislop.mjs は aislop の scan を実行し、結果の指摘を
// scripts/aislop-accepted.json と突き合わせる。テストでは、リポジトリを模した
// 一時ツリーを作り、node_modules/.bin/aislop に固定の JSON を返す偽物を置いて、
// 通信も実スキャンもせずに照合だけを観測する。
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const REPO_ROOT = path.resolve(import.meta.dirname, "..");
const CHECK_SCRIPT = path.join(REPO_ROOT, "scripts", "check-aislop.mjs");

// 偽の aislop: 環境変数 AISLOP_FAKE_REPORT のパスから JSON を読み、そのまま返す。
// 終了コードは AISLOP_FAKE_EXIT で与える（既定 0）。aislop 本体の --json 出力を模す。
const FAKE_CLI = `#!/usr/bin/env node
const fs = require("node:fs");
const report = process.env.AISLOP_FAKE_REPORT;
if (report) process.stdout.write(fs.readFileSync(report, "utf8"));
process.exit(Number(process.env.AISLOP_FAKE_EXIT || "0"));
`;

function diagnostic(overrides) {
  return {
    filePath: "ledger/tools/sample.py",
    engine: "ai-slop",
    rule: "ai-slop/unused-import",
    severity: "warning",
    message: "Imported symbol 'sys' is never used",
    help: "",
    line: 15,
    column: 0,
    category: "AI Slop",
    fixable: true,
    ...overrides,
  };
}

function report(diagnostics, overrides = {}) {
  return {
    schemaVersion: "1",
    cliVersion: "0.20.1",
    version: "0.20.1",
    score: 96,
    label: "good",
    scoreable: true,
    coverage: { supportedFiles: 47, unsupportedFiles: 0, dominantUnsupported: null, scoreable: true },
    engines: {},
    engineDefinitions: {},
    diagnostics,
    findingAssessment: { rows: [], byKind: {}, byConfidence: {} },
    summary: {
      errors: diagnostics.filter((d) => d.severity === "error").length,
      warnings: diagnostics.filter((d) => d.severity === "warning").length,
      fixable: diagnostics.filter((d) => d.fixable).length,
      files: 47,
      elapsed: "1ms",
    },
    ...overrides,
  };
}

function makeRepo(reportObject, accepted) {
  const root = mkdtempSync(path.join(tmpdir(), "check-aislop-"));
  mkdirSync(path.join(root, "scripts"), { recursive: true });
  mkdirSync(path.join(root, "node_modules", ".bin"), { recursive: true });
  writeFileSync(path.join(root, "node_modules", ".bin", "aislop"), FAKE_CLI, { mode: 0o755 });
  if (reportObject !== undefined) {
    writeFileSync(path.join(root, "aislop-report.json"), JSON.stringify(reportObject));
  }
  if (accepted !== undefined) {
    writeFileSync(path.join(root, "scripts", "aislop-accepted.json"), JSON.stringify(accepted));
  }
  return { root };
}

function runCheck(root, extraEnv = {}) {
  const env = { ...process.env, ...extraEnv };
  delete env.AISLOP_NO_TELEMETRY;
  delete env.AISLOP_NO_UPDATE_NOTIFIER;
  // check-aislop.mjs はリポジトリルートをスクリプト自身の位置（import.meta.url）から
  // 解決する。偽ツリーで走らせるため、実スクリプトをツリーへ複製して実行する。
  const script = path.join(root, "scripts", "check-aislop.mjs");
  writeFileSync(script, readFileSync(CHECK_SCRIPT, "utf8"));
  return spawnSync(process.execPath, [script], {
    cwd: root,
    env,
    encoding: "utf8",
    timeout: 60_000,
  });
}

function withReport(reportObject, accepted, extraEnv = {}) {
  const { root } = makeRepo(reportObject, accepted);
  return { result: runCheck(root, {
    AISLOP_FAKE_REPORT: path.join(root, "aislop-report.json"),
    ...extraEnv,
  }), root };
}

test("登録済みの指摘だけなら成功する", () => {
  const accepted = {
    accepted: [
      { rule: "ai-slop/unused-import", file: "ledger/tools/sample.py", line: 15, reason: "移植対象外" },
    ],
  };
  const { result } = withReport(report([diagnostic()]), accepted);
  assert.equal(result.status, 0, result.stderr + result.stdout);
});

test("未登録の指摘があれば失敗する", () => {
  const accepted = { accepted: [] };
  const { result } = withReport(report([diagnostic()]), accepted);
  assert.notEqual(result.status, 0, result.stdout);
  assert.match(result.stdout + result.stderr, /sample\.py/);
  assert.match(result.stdout + result.stderr, /ai-slop\/unused-import/);
});

test("登録の規則が違えば別の指摘として失敗する", () => {
  const accepted = {
    accepted: [{ rule: "ai-slop/unused-import", file: "ledger/tools/other.py", line: 15, reason: "別ファイル" }],
  };
  const { result } = withReport(report([diagnostic()]), accepted);
  assert.notEqual(result.status, 0, result.stdout);
});

test("登録行が食い違えば別の指摘として失敗する", () => {
  const accepted = {
    accepted: [
      { rule: "ai-slop/unused-import", file: "ledger/tools/sample.py", line: 99, reason: "行がずれている" },
    ],
  };
  const { result } = withReport(report([diagnostic()]), accepted);
  assert.notEqual(result.status, 0, result.stdout);
});

test("aislop 実行時に利用統計と更新通知を止める環境変数を渡す", () => {
  const accepted = { accepted: [] };
  const { root } = makeRepo(report([]), accepted);
  const envDump = path.join(root, "aislop-env.json");
  writeFileSync(
    path.join(root, "node_modules", ".bin", "aislop"),
    `#!/usr/bin/env node
const fs = require("node:fs");
fs.writeFileSync(process.env.AISLOP_ENV_DUMP, JSON.stringify({
  telemetry: process.env.AISLOP_NO_TELEMETRY || null,
  notifier: process.env.AISLOP_NO_UPDATE_NOTIFIER || null,
}));
process.stdout.write(JSON.stringify({
  schemaVersion: "1", score: 100, scoreable: true,
  coverage: { scoreable: true }, diagnostics: [], engines: {},
}));
`,
    { mode: 0o755 },
  );
  chmodSync(path.join(root, "node_modules", ".bin", "aislop"), 0o755);
  const result = runCheck(root, { AISLOP_ENV_DUMP: envDump });
  assert.equal(result.status, 0, result.stderr + result.stdout);
  const seen = JSON.parse(
    spawnSync(process.execPath, ["-e", "process.stdout.write(require('node:fs').readFileSync(process.argv[1],'utf8'))", envDump], {
      encoding: "utf8",
    }).stdout,
  );
  assert.equal(seen.telemetry, "1");
  assert.equal(seen.notifier, "1");
});

test("aislop が指摘を返しても scoreable が false なら合格にしない", () => {
  const accepted = { accepted: [] };
  const notScoreable = report([], {
    score: null,
    label: "not scored",
    scoreable: false,
    coverage: { supportedFiles: 0, unsupportedFiles: 5, dominantUnsupported: "swift", scoreable: false },
  });
  const { result } = withReport(notScoreable, accepted);
  assert.notEqual(result.status, 0, result.stdout);
});

test("エンジンが失敗していれば合格にしない", () => {
  const accepted = { accepted: [] };
  const failedEngine = report([], {
    engines: { "ai-slop": { issues: 0, skipped: false, elapsed: 1, failed: true } },
  });
  const { result } = withReport(failedEngine, accepted);
  assert.notEqual(result.status, 0, result.stdout);
});

test("aislop が異常終了しても指摘 0 件として合格にしない", () => {
  const accepted = { accepted: [] };
  const { root } = makeRepo(report([]), accepted);
  const result = runCheck(root, {
    AISLOP_FAKE_REPORT: path.join(root, "aislop-report.json"),
    AISLOP_FAKE_EXIT: "2",
  });
  assert.notEqual(result.status, 0, result.stdout);
});

test("受け入れ一覧を読めなければ失敗する", () => {
  const { root } = makeRepo(report([]), undefined);
  writeFileSync(path.join(root, "scripts", "aislop-accepted.json"), "{ broken");
  const result = runCheck(root, { AISLOP_FAKE_REPORT: path.join(root, "aislop-report.json") });
  assert.notEqual(result.status, 0, result.stdout);
});

test("aislop の実行物が無ければ失敗する", () => {
  const root = mkdtempSync(path.join(tmpdir(), "check-aislop-"));
  mkdirSync(path.join(root, "scripts"), { recursive: true });
  writeFileSync(path.join(root, "scripts", "aislop-accepted.json"), JSON.stringify({ accepted: [] }));
  const result = runCheck(root);
  assert.notEqual(result.status, 0, result.stdout);
});

test("package.json の check:aislop がこのスクリプトを指す", () => {
  const pkg = JSON.parse(readFileSync(path.join(REPO_ROOT, "package.json"), "utf8"));
  assert.ok(pkg.scripts && typeof pkg.scripts["check:aislop"] === "string");
  assert.match(pkg.scripts["check:aislop"], /check-aislop\.mjs/);
  assert.ok(pkg.devDependencies && typeof pkg.devDependencies.aislop === "string");
  assert.equal(pkg.devDependencies.aislop, "0.20.1");
});

test(".gitignore が node_modules をリポジトリ定義で無視する", () => {
  const gitignore = readFileSync(path.join(REPO_ROOT, ".gitignore"), "utf8");
  const lines = gitignore.split(/\r?\n/).map((line) => line.trim());
  assert.ok(
    lines.includes("/node_modules/") || lines.includes("node_modules/") || lines.includes("/node_modules"),
    "node_modules is not ignored by the repository .gitignore",
  );
});

test("accepted にあるが今回の scan に無い項目は警告だけで合格させる", () => {
  // 修正して指摘が消えたとき、accepted の掃除を忘れても機械ゲートを赤にしない。
  // stale を fail にする実装を拒否する。
  const accepted = {
    accepted: [
      { rule: "ai-slop/unused-import", file: "ledger/tools/gone.py", line: 3, reason: "解消済み" },
    ],
  };
  const { result } = withReport(report([]), accepted);
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.match(result.stderr, /stale/);
});
