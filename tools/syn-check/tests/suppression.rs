//! Suppression comments: what they cover, and how they show up for audit.

use std::path::PathBuf;

use syn_check::{layout::Layout, options::Options, report::Report};

fn run(fixture: &str) -> Report {
  let path = PathBuf::from("tests/ui/synthetic/suppression").join(fixture);
  let options =
    Options { layout: Layout::Ng, rules: Some(vec!["type-per-file".to_string(), "use-placement".to_string()]) };
  syn_check::run(&[path], &options).expect("run the checker")
}

fn violations(report: &Report) -> Vec<(&str, usize)> {
  report.violations.iter().map(|violation| (violation.rule.as_str(), violation.position.line)).collect()
}

#[test]
fn allow_covers_the_next_element_only() {
  let report = run("allow_next.rs");
  assert_eq!(violations(&report), vec![("type-per-file", 9)]);
  let record = &report.suppressions[0];
  assert_eq!((record.line, record.file_scope, record.used), (3, false, true));
  assert_eq!(record.rules, vec!["type-per-file"]);
  assert_eq!(record.reason, "2 つ目の型は First とだけ一緒に使う");
}

#[test]
fn allow_covers_everything_inside_the_element() {
  let report = run("allow_covers_body.rs");
  assert_eq!(violations(&report), vec![("use-placement", 10)]);
}

#[test]
fn allow_separated_by_a_blank_line_covers_nothing() {
  let report = run("allow_blank_line.rs");
  assert_eq!(violations(&report), vec![("type-per-file", 5)]);
  assert!(!report.suppressions[0].used);
}

#[test]
fn allow_file_covers_the_file() {
  let report = run("allow_file.rs");
  assert_eq!(violations(&report), Vec::<(&str, usize)>::new());
  let record = &report.suppressions[0];
  assert!(record.file_scope && record.used);
  assert_eq!(record.rules, vec!["type-per-file", "use-placement"]);
}

#[test]
fn malformed_comments_are_violations_and_suppress_nothing() {
  let report = run("malformed.rs");
  assert_eq!(
    violations(&report),
    vec![
      ("suppression", 1),
      ("suppression", 4),
      ("type-per-file", 5),
      ("suppression", 7),
      ("type-per-file", 8),
      ("type-per-file", 11),
    ]
  );
  assert!(report.suppressions.is_empty());
}

#[test]
fn json_lists_violations_and_suppressions() {
  let json = run("allow_next.rs").to_json();
  assert_eq!(json["clean"], false);
  assert_eq!(json["summary"]["violations_by_rule"]["type-per-file"], 1);
  assert_eq!(json["violations"][0]["line"], 9);
  assert_eq!(json["suppressions"][0]["scope"], "next");
  assert_eq!(json["suppressions"][0]["used"], true);
}
