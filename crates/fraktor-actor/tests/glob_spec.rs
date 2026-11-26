// Ported from Pekko v2.0.0 (SHA 9fbd2283b1cf701c90a1223bf621942c191db61f):
//   actor-tests/src/test/scala/org/apache/pekko/util/GlobSpec.scala
//
// The original's six cases are kept as one test each. The exhaustive case
// compares against a reference matcher written directly from the glob
// meaning (`?` is any one character, `*` is any run of characters, every
// other character is literal, and the whole input must match). That is the
// meaning `Helpers.makePattern` approximates; the two agree on inputs
// without line terminators and backslashes, which is the range the
// original's alphabets cover (design of #186,
// docs/design/issue-186-util-specs.md §2.2).

use std::time::Instant;

use fraktor_actor::kernel::glob::GlobPattern;

/// The glob meaning, written as a direct recursion. Used as the reference
/// for the exhaustive case and shared with the property tests by copy.
fn reference_matches(pattern: &[char], input: &[char]) -> bool {
  if pattern.is_empty() {
    return input.is_empty();
  }
  if pattern[0] == '*' {
    return (0..=input.len()).any(|skip| reference_matches(&pattern[1..], &input[skip..]));
  }
  if !input.is_empty() && (pattern[0] == '?' || pattern[0] == input[0]) {
    return reference_matches(&pattern[1..], &input[1..]);
  }
  false
}

fn all_strings(alphabet: &[char], max_length: usize) -> Vec<String> {
  let mut result = vec![String::new()];
  let mut current = vec![String::new()];
  for _ in 0..max_length {
    let mut next = Vec::new();
    for prefix in &current {
      for ch in alphabet {
        let mut candidate = prefix.clone();
        candidate.push(*ch);
        next.push(candidate);
      }
    }
    result.extend(next.iter().cloned());
    current = next;
  }
  result
}

fn matches(pattern: &str, input: &str) -> bool {
  GlobPattern::new(pattern).matches(input)
}

#[test]
fn glob_matches_literal_cases() {
  assert!(matches("abc", "abc"));
  assert!(!matches("abc", "abd"));
  assert!(matches("", ""));
  assert!(!matches("", "a"));
  assert!(!matches("a", ""));
}

#[test]
fn glob_treats_question_mark_as_exactly_one_character() {
  assert!(matches("a?c", "abc"));
  assert!(!matches("a?c", "ac"));
  assert!(!matches("a?c", "abbc"));
  assert!(matches("???", "abc"));
}

#[test]
fn glob_treats_star_as_any_run_of_characters() {
  assert!(matches("*", ""));
  assert!(matches("*", "anything"));
  assert!(matches("a*", "a"));
  assert!(matches("a*c", "abbbc"));
  assert!(!matches("a*c", "abbbd"));
  assert!(matches("**", "ab"));
  assert!(matches("*b*", "abc"));
}

#[test]
fn glob_agrees_with_reference_matcher_exhaustively_over_short_inputs() {
  let patterns = all_strings(&['a', 'b', '*', '?'], 4);
  let inputs = all_strings(&['a', 'b'], 4);
  assert!(patterns.len() > 300, "patterns: {}", patterns.len());
  for pattern in &patterns {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let glob = GlobPattern::new(pattern);
    for input in &inputs {
      let input_chars: Vec<char> = input.chars().collect();
      assert_eq!(
        glob.matches(input),
        reference_matches(&pattern_chars, &input_chars),
        "pattern [{pattern}] input [{input}]"
      );
    }
  }
}

#[test]
fn glob_matches_backtracking_pattern_promptly() {
  let pattern = "*a".repeat(12) + "*b";
  let name = "a".repeat(36);
  let started = Instant::now();
  assert!(!matches(&pattern, &name));
  assert!(started.elapsed().as_secs() < 3, "took {:?}", started.elapsed());
}

#[test]
fn glob_stays_prompt_as_input_grows() {
  let pattern = "*a".repeat(20) + "*b";
  let started = Instant::now();
  assert!(!matches(&pattern, &"a".repeat(2000)));
  assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
}
