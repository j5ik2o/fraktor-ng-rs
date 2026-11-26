//! The `%g` formatting of Java's `Formatter`, rebuilt from core pieces.
//!
//! Java's `%g` rounds the *shortest* decimal digits of the value (the
//! digits `Double.toString` produces) with HALF_UP. Rounding the exact
//! binary value instead gives different results, for example 1.0005 with
//! precision 4 becomes `1.001` in Java and `1.000` from the exact value.
//! The steps here are: take the shortest digits from core's `{:e}`, round
//! them HALF_UP to the precision, choose the decimal or exponential form
//! from the exponent *after* rounding, and write the exponent in Java's
//! form (`e+04`, signed, at least two digits).

use alloc::{format, string::String, vec::Vec};

/// Formats a non-negative finite `value` like Java's `%.<precision>g`.
pub(crate) fn format_g(value: f64, precision: usize) -> String {
  if value == 0.0 {
    let digits = alloc::vec![b'0'; precision];
    return render(&digits, 0, precision);
  }

  // The shortest digits and the exponent of the first digit. `{:e}` writes
  // values like `1.0005e3` or `9e0`; durations formatted here are at least
  // 1 in their chosen unit, but the code below keeps negative exponents
  // working as well.
  let scientific = format!("{value:e}");
  let (mantissa, exponent_text) = scientific.split_once('e').expect("LowerExp always writes an exponent");
  let mut exponent: i32 = exponent_text.parse().expect("the exponent is an integer");
  let mut digits: Vec<u8> = mantissa.bytes().filter(|byte| *byte != b'.').collect();

  if digits.len() > precision {
    let dropped = digits[precision];
    digits.truncate(precision);
    if dropped >= b'5' {
      // HALF_UP on the shortest digits: a dropped part starting at 5 or
      // more rounds up, whatever follows it.
      let mut index = precision;
      loop {
        if index == 0 {
          digits.insert(0, b'1');
          digits.truncate(precision);
          exponent += 1;
          break;
        }
        index -= 1;
        if digits[index] == b'9' {
          digits[index] = b'0';
        } else {
          digits[index] += 1;
          break;
        }
      }
    }
  } else {
    digits.resize(precision, b'0');
  }

  render(&digits, exponent, precision)
}

/// Renders exactly `precision` significant `digits`, whose first digit has
/// decimal exponent `exponent`, in Java's `%g` form.
fn render(digits: &[u8], exponent: i32, precision: usize) -> String {
  let digits_text = core::str::from_utf8(digits).expect("digits are ASCII");
  if exponent < -4 || exponent >= precision as i32 {
    // Exponential form: one digit, then the rest after a point (unless the
    // precision is 1), then `e`, a sign, and at least two exponent digits.
    let mut out = String::new();
    out.push(digits_text.as_bytes()[0] as char);
    if precision > 1 {
      out.push('.');
      out.push_str(&digits_text[1..]);
    }
    out.push('e');
    out.push(if exponent < 0 { '-' } else { '+' });
    let magnitude = exponent.unsigned_abs();
    if magnitude < 10 {
      out.push('0');
    }
    out.push_str(&alloc::format!("{magnitude}"));
    return out;
  }

  let point = exponent + 1;
  if point <= 0 {
    let mut out = String::from("0.");
    for _ in 0..(-point) {
      out.push('0');
    }
    out.push_str(digits_text);
    out
  } else if point as usize >= precision {
    let mut out = String::from(digits_text);
    for _ in 0..(point as usize - precision) {
      out.push('0');
    }
    out
  } else {
    let point = point as usize;
    let mut out = String::from(&digits_text[..point]);
    out.push('.');
    out.push_str(&digits_text[point..]);
    out
  }
}
