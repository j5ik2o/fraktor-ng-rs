//! Runs the new rules on the UI fixtures of the old dylint lints and compares
//! the result with the old expected output (`<case>.stderr`).
//!
//! Each lint has a `RESULTS.md` next to its fixtures. The test fails when that
//! file is stale. Regenerate it with:
//!
//! ```sh
//! SYN_CHECK_BLESS=1 cargo test --test legacy_ui
//! ```

mod ambiguous_suffix;
mod cfg_std_forbid;
mod mod_file;
mod module_examples;
mod module_wiring;
mod port_adaptor_boundary;
mod rustdoc;
mod spec;
mod type_per_file;
mod use_placement;

use std::{
  collections::{BTreeMap, BTreeSet},
  env, fs,
  path::{Path, PathBuf},
};

use spec::LintSpec;
use syn_check::{layout::Layout, options::Options, workspace::Workspace};

const ROOT: &str = "tests/ui/legacy";

/// A reported or expected location: the path under the fixture directory, and the line.
type Location = (String, usize);

const SPECS: [&LintSpec; 9] = [
  &mod_file::SPEC,
  &module_examples::SPEC,
  &type_per_file::SPEC,
  &use_placement::SPEC,
  &ambiguous_suffix::SPEC,
  &rustdoc::SPEC,
  &module_wiring::SPEC,
  &port_adaptor_boundary::SPEC,
  &cfg_std_forbid::SPEC,
];

/// Keeps `tests/ui/legacy/SUMMARY.md`, the one-row-per-lint overview, up to date.
#[test]
fn summary() {
  let mut rows = Vec::new();
  let mut totals = [0usize; 3];
  for spec in SPECS {
    let (_, _, counts) = evaluate(spec);
    for (total, count) in totals.iter_mut().zip(counts) {
      *total += count;
    }
    rows.push(format!(
      "| [`{dir}`]({dir}/RESULTS.md) | {rules} | {} | {} | {} | {} |",
      counts[0],
      counts[1],
      counts[2],
      spec.adaptations.len(),
      dir = spec.dir,
      rules = spec.rules.iter().map(|rule| format!("`{rule}`")).collect::<Vec<_>>().join("、"),
    ));
  }
  let markdown = format!(
    "# 旧 UI テストとの対応（まとめ）\n\n\
         旧 fraktor-rs の dylint 9 本の UI テストを、新チェッカーで判定した結果である。行は、ケースと、\
         フィクスチャを変更した補助ファイルを数える。\n\
         tests-location の UI テストは同梱されていなかったので、ここには無い。\n\
         このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。\n\n\
         | 旧 lint | 新ルール | 一致 | 差異あり（意図どおり） | 不一致 | 変更したフィクスチャ |\n\
         |---|---|---|---|---|---|\n\
         {rows}\n\
         | 合計 | | {} | {} | {} | {} |\n",
    totals[0],
    totals[1],
    totals[2],
    SPECS.iter().map(|spec| spec.adaptations.len()).sum::<usize>(),
    rows = rows.join("\n"),
  );
  let path = Path::new(ROOT).join("SUMMARY.md");
  if env::var_os("SYN_CHECK_BLESS").is_some() {
    fs::write(&path, &markdown).expect("write SUMMARY.md");
  }
  let committed = fs::read_to_string(&path).unwrap_or_default();
  assert!(
    committed == markdown,
    "{} が古い。`SYN_CHECK_BLESS=1 cargo test --test legacy_ui` で更新すること",
    path.display()
  );
}

#[test]
fn mod_file() {
  check(&mod_file::SPEC);
}

#[test]
fn module_examples() {
  check(&module_examples::SPEC);
}

#[test]
fn type_per_file() {
  check(&type_per_file::SPEC);
}

#[test]
fn use_placement() {
  check(&use_placement::SPEC);
}

#[test]
fn ambiguous_suffix() {
  check(&ambiguous_suffix::SPEC);
}

#[test]
fn rustdoc() {
  check(&rustdoc::SPEC);
}

#[test]
fn module_wiring() {
  check(&module_wiring::SPEC);
}

#[test]
fn port_adaptor_boundary() {
  check(&port_adaptor_boundary::SPEC);
}

#[test]
fn cfg_std_forbid() {
  check(&cfg_std_forbid::SPEC);
}

fn check(spec: &LintSpec) {
  assert!(!spec.rules.is_empty(), "{}: ルールが割り当てられていない", spec.dir);
  let (markdown, problems, _) = evaluate(spec);
  let results_path = Path::new(ROOT).join(spec.dir).join("RESULTS.md");
  if env::var_os("SYN_CHECK_BLESS").is_some() {
    fs::write(&results_path, &markdown).expect("write RESULTS.md");
  }
  assert!(problems.is_empty(), "{}: 旧 UI テストと一致しない\n{}", spec.dir, problems.join("\n"));
  let committed = fs::read_to_string(&results_path).unwrap_or_default();
  assert!(
    committed == markdown,
    "{} が古い。`SYN_CHECK_BLESS=1 cargo test --test legacy_ui` で更新すること",
    results_path.display()
  );
}

/// The table for one lint, what does not match, and the counts of matching,
/// intentionally differing, and mismatching rows.
fn evaluate(spec: &LintSpec) -> (String, Vec<String>, [usize; 3]) {
  let dir = Path::new(ROOT).join(spec.dir);
  let options =
    Options { layout: Layout::Legacy, rules: Some(spec.rules.iter().map(|rule| rule.to_string()).collect()) };
  let report = syn_check::run(std::slice::from_ref(&dir), &options).expect("run the checker");

  let mut problems: Vec<String> = report
    .parse_failures
    .iter()
    .map(|failure| format!("パースできない: {}: {}", failure.path.display(), failure.message))
    .collect();
  let relative = |path: &Path| path.strip_prefix(&dir).unwrap_or(path).to_string_lossy().replace('\\', "/");
  let mut actual: BTreeMap<String, Vec<usize>> = BTreeMap::new();
  for violation in &report.violations {
    actual.entry(relative(&violation.path)).or_default().push(violation.position.line);
  }

  let mut rows = Vec::new();
  let mut attributed = BTreeSet::new();
  let mut tally = BTreeMap::new();
  for case in case_files(&dir) {
    let case_name = relative(&case);
    let old_expected = parse_stderr(&case.with_extension("stderr"));
    let difference = spec.differences.iter().find(|difference| difference.case == case_name);
    let expected: Vec<Location> = match difference {
      | Some(difference) => difference.expected.iter().map(|(path, line)| (path.to_string(), *line)).collect(),
      | None => old_expected.clone(),
    };

    let mut files: BTreeSet<String> = BTreeSet::from([case_name.clone()]);
    files.extend(old_expected.iter().chain(&expected).map(|(path, _)| path.clone()));
    files.extend(path_attribute_targets(&case).iter().map(|path| relative(path)));
    let reported: Vec<Location> = files
      .iter()
      .flat_map(|path| actual.get(path).into_iter().flatten().map(move |line| (path.clone(), *line)))
      .collect();

    let verdict = if sorted(&reported) != sorted(&expected) {
      problems.push(format!("{case_name}: 期待 {} / 結果 {}", describe(&expected), describe(&reported)));
      "不一致"
    } else if difference.is_some() {
      "差異あり（意図どおり）"
    } else {
      "一致"
    };
    *tally.entry(verdict).or_insert(0usize) += 1;

    let mut notes: Vec<String> = spec
      .adaptations
      .iter()
      .filter(|(path, _)| files.contains(*path))
      .map(|(path, change)| format!("フィクスチャ変更 `{path}`: {change}"))
      .collect();
    notes.extend(difference.map(|difference| format!("差異の理由: {}", difference.reason)));
    rows.push(format!(
      "| `{case_name}` | {} | {} | {verdict} | {} |",
      describe(&old_expected),
      describe(&reported),
      notes.join("<br>")
    ));
    attributed.extend(files);
  }

  // どのケースにも属さない補助ファイルは、違反なしでなければならない。
  let all_files = Workspace::discover(std::slice::from_ref(&dir)).expect("list the fixtures");
  for path in all_files.iter().map(|path| relative(path)) {
    if attributed.contains(&path) {
      continue;
    }
    let reported: Vec<Location> = actual.get(&path).into_iter().flatten().map(|line| (path.clone(), *line)).collect();
    let notes: Vec<String> = spec
      .adaptations
      .iter()
      .filter(|(adapted, _)| *adapted == path)
      .map(|(_, change)| format!("フィクスチャ変更: {change}"))
      .collect();
    if reported.is_empty() && notes.is_empty() {
      continue;
    }
    let verdict = if reported.is_empty() {
      "一致"
    } else {
      problems.push(format!("{path}: 補助ファイルに違反 {}", describe(&reported)));
      "不一致"
    };
    *tally.entry(verdict).or_insert(0usize) += 1;
    rows.push(format!(
      "| `{path}`（補助ファイル） | クリーン | {} | {verdict} | {} |",
      describe(&reported),
      notes.join("<br>")
    ));
  }
  for (path, _) in spec.adaptations {
    if !all_files.iter().any(|file| relative(file) == *path) {
      problems.push(format!("adaptations にあるファイルが無い: {path}"));
    }
  }

  let count = |verdict: &str| tally.get(verdict).copied().unwrap_or(0);
  let markdown = format!(
    "# {dir}: 旧 UI テストとの対応\n\n\
         旧 `{dir}-lint` の UI テストを、新チェッカーのルール {rules} で判定した結果である。\n\
         期待は旧 lint の `<ケース>.stderr` から読む。`.stderr` が無いか空のケースはクリーンが期待である。\n\
         このファイルは `SYN_CHECK_BLESS=1 cargo test --test legacy_ui` が生成する。手で編集しない。\n\n\
         | ケース | 旧 lint の期待 | 新チェッカーの結果 | 判定 | 備考 |\n\
         |---|---|---|---|---|\n\
         {rows}\n\n\
         一致 {matched} 件、差異あり（意図どおり） {differing} 件、不一致 {mismatched} 件。\n",
    dir = spec.dir,
    rules = spec.rules.iter().map(|rule| format!("`{rule}`")).collect::<Vec<_>>().join("、"),
    rows = rows.join("\n"),
    matched = count("一致"),
    differing = count("差異あり（意図どおり）"),
    mismatched = count("不一致"),
  );
  let counts = [count("一致"), count("差異あり（意図どおり）"), count("不一致")];
  (markdown, problems, counts)
}

/// The top-level `.rs` files of a fixture directory. Each one was a UI test case.
fn case_files(dir: &Path) -> Vec<PathBuf> {
  let mut cases: Vec<PathBuf> = fs::read_dir(dir)
    .expect("read the fixture directory")
    .map(|entry| entry.expect("read a directory entry").path())
    .filter(|path| path.is_file() && path.extension().is_some_and(|extension| extension == "rs"))
    .collect();
  cases.sort();
  cases
}

/// The primary location of each diagnostic in a rustc `.stderr` file.
fn parse_stderr(path: &Path) -> Vec<Location> {
  let Ok(text) = fs::read_to_string(path) else {
    return Vec::new();
  };
  let lines: Vec<&str> = text.lines().collect();
  let mut locations = Vec::new();
  for (index, line) in lines.iter().enumerate() {
    let is_diagnostic = (line.starts_with("error") || line.starts_with("warning"))
      && !line.contains("aborting due to")
      && !line.ends_with(" emitted");
    if !is_diagnostic {
      continue;
    }
    let arrow = lines[index + 1..]
      .iter()
      .find_map(|next| next.trim_start().strip_prefix("--> $DIR/"))
      .unwrap_or_else(|| panic!("{}: `{line}` has no location", path.display()));
    let mut parts = arrow.rsplitn(3, ':');
    let _column = parts.next();
    let line_number = parts
      .next()
      .and_then(|number| number.parse().ok())
      .unwrap_or_else(|| panic!("{}: cannot read the line of `{arrow}`", path.display()));
    let file = parts.next().expect("location has a path");
    locations.push((file.to_string(), line_number));
  }
  locations
}

/// The files a case pulls in with `#[path = "..."]`.
fn path_attribute_targets(case: &Path) -> Vec<PathBuf> {
  let text = fs::read_to_string(case).expect("read the case");
  let directory = case.parent().expect("case has a directory");
  text
    .lines()
    .filter_map(|line| line.trim().strip_prefix("#[path"))
    .filter_map(|rest| rest.split('"').nth(1))
    .map(|target| directory.join(target))
    .collect()
}

fn sorted(locations: &[Location]) -> Vec<Location> {
  let mut sorted = locations.to_vec();
  sorted.sort();
  sorted
}

fn describe(locations: &[Location]) -> String {
  if locations.is_empty() {
    return "クリーン".to_string();
  }
  let list: Vec<String> = sorted(locations).iter().map(|(path, line)| format!("`{path}:{line}`")).collect();
  format!("違反 {} 件（{}）", locations.len(), list.join("、"))
}
