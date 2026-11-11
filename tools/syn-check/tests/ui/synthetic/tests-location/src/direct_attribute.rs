//! find_direct_test_attributes: 属性名が `test` か `::test` で終われば違反。

#[test]
fn plain() {}

#[ test ]
fn spaced() {}

#[tokio::test]
async fn tokio_plain() {}

#[tokio::test(flavor = "multi_thread")]
async fn tokio_flavor() {}

#[::core::prelude::v1::test]
fn absolute() {}

// ここから下は違反にならない。
#[test_case(1)]
fn case(_: u8) {}

#[rstest]
fn fixture() {}

#[should_panic]
#[testing]
#[my::test_util]
fn not_test() {}
