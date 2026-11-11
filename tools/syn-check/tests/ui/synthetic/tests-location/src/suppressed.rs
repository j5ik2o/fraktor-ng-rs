// syn-check: allow(tests-location) -- 抑制がモジュールの中まで届くことを確かめる
#[cfg(test)]
#[allow(dead_code)]
mod tests {
    #[cfg(test)]
    fn helper() {}

    #[test]
    fn ok() {}
}

#[test]
fn not_suppressed() {}
