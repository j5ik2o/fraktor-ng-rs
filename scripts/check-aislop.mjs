// aislop の scan を実行し、結果を scripts/aislop-accepted.json と照合する機械ゲート。
//
// 判定:
//   - scan が異常終了した（JSON レポートが返らない）、schemaVersion が "1" で
//     ない、scoreable が false、`engines.*.failed === true` のいずれかなら fail。
//     aislop は診断の内容でも終了コード 1 を返すため、レポートが返った場合は
//     終了コードでは判定せず、検証と照合の結果で判定する。
//   - 各診断を rule + filePath + line で accepted 項目と照合し、未登録の診断が
//     1 件でもあれば fail（診断を列挙）。
//   - accepted にあるが今回の scan に無い項目は、stale として警告する（fail にしない）。
//   - accepted 一覧が無い・読めない・形が不正なら fail（食い違いを検出できなくなるため）。
//
// 実行時に利用統計と更新通知を止めるため、aislop へ AISLOP_NO_TELEMETRY=1 と
// AISLOP_NO_UPDATE_NOTIFIER=1 を渡す。
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const AISLOP_BIN = path.join(REPO_ROOT, "node_modules", ".bin", "aislop");
const ACCEPTED_FILE = path.join(REPO_ROOT, "scripts", "aislop-accepted.json");
const EXPECTED_SCHEMA_VERSION = "1";

function fail(lines) {
  for (const line of lines) process.stdout.write(`${line}\n`);
  process.exitCode = 1;
}

function loadAccepted() {
  if (!existsSync(ACCEPTED_FILE)) {
    throw new Error(`accepted list not found: ${path.relative(REPO_ROOT, ACCEPTED_FILE)}`);
  }
  const parsed = JSON.parse(readFileSync(ACCEPTED_FILE, "utf8"));
  if (!parsed || !Array.isArray(parsed.accepted)) {
    throw new Error("accepted list must be an object with an 'accepted' array");
  }
  return parsed.accepted.map((entry) => {
    if (typeof entry.rule !== "string" || entry.rule === "") {
      throw new Error("accepted entry is missing a non-empty 'rule'");
    }
    if (typeof entry.file !== "string" || entry.file === "") {
      throw new Error(`accepted entry for ${entry.rule} is missing a non-empty 'file'`);
    }
    if (!Number.isInteger(entry.line)) {
      throw new Error(`accepted entry for ${entry.rule} in ${entry.file} is missing an integer 'line'`);
    }
    if (typeof entry.reason !== "string" || entry.reason.trim() === "") {
      throw new Error(`accepted entry for ${entry.rule} in ${entry.file}:${entry.line} has no 'reason'`);
    }
    return entry;
  });
}

function runScan() {
  if (!existsSync(AISLOP_BIN)) {
    return { error: `aislop is not installed (missing ${path.relative(REPO_ROOT, AISLOP_BIN)}); run 'npm ci'` };
  }
  const proc = spawnSync(AISLOP_BIN, ["scan", "--json", "."], {
    cwd: REPO_ROOT,
    encoding: "utf8",
    timeout: 600_000,
    env: {
      ...process.env,
      AISLOP_NO_TELEMETRY: "1",
      AISLOP_NO_UPDATE_NOTIFIER: "1",
    },
  });
  if (proc.error) return { error: `failed to run aislop: ${proc.error.message}` };
  // aislop は、error 診断がある場合やスコアが下限を下回る場合にも終了コード 1 を
  // 返す。その場合も標準出力には JSON レポートが入っているため、終了コードだけでは
  // 異常終了と区別できない。レポートとして読める場合は終了コードでは落とさず、
  // 後段の検証と受け入れ照合で判定する。レポートが読めない場合だけ異常終了として扱う。
  let report;
  try {
    report = JSON.parse(proc.stdout);
  } catch {
    return {
      error: `aislop scan exited with ${proc.status} and did not return JSON: ${(proc.stderr || "").trim()}`,
    };
  }
  if (report === null || typeof report !== "object" || Array.isArray(report)) {
    return {
      error: `aislop scan exited with ${proc.status} and did not return a report object: ${(proc.stderr || "").trim()}`,
    };
  }
  return { report };
}

function keyOf(entry) {
  return `${entry.rule}\u0000${entry.file}\u0000${entry.line}`;
}

function main() {
  let accepted;
  try {
    accepted = loadAccepted();
  } catch (error) {
    fail([`check-aislop: invalid accepted list: ${error.message}`]);
    return;
  }

  const { report, error } = runScan();
  if (error !== undefined) {
    fail([`check-aislop: ${error}`]);
    return;
  }

  if (report.schemaVersion !== EXPECTED_SCHEMA_VERSION) {
    fail([`check-aislop: unexpected schemaVersion ${JSON.stringify(report.schemaVersion)}`]);
    return;
  }
  if (report.scoreable === false) {
    fail(["check-aislop: scan is not scoreable (coverage insufficient); refusing to pass"]);
    return;
  }
  const failedEngines = Object.entries(report.engines ?? {})
    .filter(([, engine]) => engine && engine.failed === true)
    .map(([name]) => name);
  if (failedEngines.length > 0) {
    fail([`check-aislop: engine(s) failed: ${failedEngines.join(", ")}`]);
    return;
  }

  const acceptedKeys = new Set(accepted.map(keyOf));
  const diagnostics = report.diagnostics ?? [];
  const diagnosticsKeys = new Set(diagnostics.map((d) => keyOf({ rule: d.rule, file: d.filePath, line: d.line })));
  const unaccepted = diagnostics.filter((d) => !acceptedKeys.has(keyOf({ rule: d.rule, file: d.filePath, line: d.line })));
  const stale = accepted.filter((entry) => !diagnosticsKeys.has(keyOf(entry)));

  for (const entry of stale) {
    process.stderr.write(`check-aislop: stale accepted entry (no longer reported): ${entry.rule} ${entry.file}:${entry.line}\n`);
  }

  const summary = report.summary ?? {};
  process.stdout.write(
    `check-aislop: score ${report.score} (${report.label}); ` +
      `${summary.errors ?? 0} error(s), ${summary.warnings ?? 0} warning(s), ` +
      `${diagnostics.length} finding(s), ${accepted.length} accepted\n`,
  );

  if (unaccepted.length === 0) return;

  const lines = [`check-aislop: ${unaccepted.length} unaccepted finding(s):`];
  for (const d of unaccepted) {
    lines.push(`  ${d.severity} ${d.rule} ${d.filePath}:${d.line} ${d.message}`);
  }
  lines.push("Fix the finding, or add it to scripts/aislop-accepted.json with a reason.");
  fail(lines);
}

main();
