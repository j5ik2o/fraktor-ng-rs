//! find_inline_test_modules: `mod tests` の後に空白を挟んで `{` が来れば違反。

mod tests {
    fn helper() {}
}

pub(crate) mod tests
{
    fn helper() {}
}

mod outer {
    mod tests{}
}

// ここから下は違反にならない。
mod tests;

mod tests_util {}

mod testsuite {}
