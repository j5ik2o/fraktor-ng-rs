// Property tests of the glob matcher outside the original GlobSpec (design
// of #186, docs/design/issue-186-util-specs.md §3, G1, G2 and G4). The
// alphabets include a line terminator and a character outside the BMP, which
// fix the documented differences from `Helpers.makePattern` and from the
// original's UTF-16 code-unit counting: the port counts Unicode scalar
// values, so `?` matches a whole character, and `?` and `*` also match line
// terminators. These are not ports of Pekko cases and are not counted in
// the ledger. Inputs come from a fixed-seed SplitMix64, so a failure
// repeats.

use fraktor_actor::kernel::glob::GlobPattern;

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

struct SplitMix64 {
  state: u64,
}

impl SplitMix64 {
  fn next_u64(&mut self) -> u64 {
    self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = self.state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
  }

  fn below(&mut self, bound: usize) -> usize {
    (self.next_u64() % bound as u64) as usize
  }
}

fn random_string(rng: &mut SplitMix64, alphabet: &[char], max_length: usize) -> String {
  let length = rng.below(max_length + 1);
  (0..length).map(|_| alphabet[rng.below(alphabet.len())]).collect()
}

#[test]
fn glob_agrees_with_reference_on_random_short_inputs() {
  let pattern_alphabet = ['a', 'b', '*', '?', '\n', '😀'];
  let input_alphabet = ['a', 'b', '\n', '😀'];
  let mut rng = SplitMix64 { state: 0x186 };
  for case in 0..4000 {
    let pattern = random_string(&mut rng, &pattern_alphabet, 6);
    let input = random_string(&mut rng, &input_alphabet, 6);
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let input_chars: Vec<char> = input.chars().collect();
    assert_eq!(
      GlobPattern::new(&pattern).matches(&input),
      reference_matches(&pattern_chars, &input_chars),
      "case {case}: pattern [{pattern}] input [{input}]"
    );
  }
}

#[test]
fn glob_without_star_needs_equal_length_and_matching_characters() {
  let alphabet = ['a', 'b', '?', '\n', '😀'];
  let input_alphabet = ['a', 'b', '\n', '😀'];
  let mut rng = SplitMix64 { state: 0x1861 };
  for case in 0..2000 {
    let pattern = random_string(&mut rng, &alphabet, 6);
    let input = random_string(&mut rng, &input_alphabet, 6);
    let expected = pattern.chars().count() == input.chars().count()
      && pattern.chars().zip(input.chars()).all(|(p, i)| p == '?' || p == i);
    assert_eq!(
      GlobPattern::new(&pattern).matches(&input),
      expected,
      "case {case}: pattern [{pattern}] input [{input}]"
    );
  }
}

#[test]
fn glob_empty_and_star_only_patterns() {
  assert!(GlobPattern::new("").matches(""));
  assert!(!GlobPattern::new("").matches("a"));
  for input in ["", "a", "anything", "\n", "😀😀"] {
    assert!(GlobPattern::new("*").matches(input), "input [{input}]");
    assert!(GlobPattern::new("***").matches(input), "input [{input}]");
  }
}

#[test]
fn glob_question_mark_matches_one_whole_character() {
  // The original counts UTF-16 code units, so its `?` can match half of a
  // surrogate pair. The port counts scalar values (design §2.2).
  assert!(GlobPattern::new("?").matches("😀"));
  assert!(!GlobPattern::new("??").matches("😀"));
  assert!(GlobPattern::new("?").matches("\n"));
}
