// Ported from Pekko v1.7.1 (SHA 9a54d1294c3ac7821bbed3c940769c3d5027d1ff):
//   actor-tests/src/test/scala/org/apache/pekko/io/dns/internal/ResolvConfParserSpec.scala
//
// The original's nine cases under `"The ResolvConfParser" should` are kept as one
// test each, named after the case, with the original name as a comment. Each case
// parses a `resolv.conf` string and checks `search` and `ndots` against the
// original's expected values, in the original's order and with the original's
// counts.
//
// The original's helper is `parse(str: String) = ResolvConfParser.parseLines(
// str.linesIterator)`, returning a `ResolvConf`. The port's `parse` takes a
// `&str` and calls `ResolvConfParser::parse_lines(text.lines())`, whose `Item` is
// `&str` (the port's `str.lines()` is the original's `linesIterator`), and
// unwraps the `Result` with `expect`. The original's `ResolvConf.search`
// (`List[String]`) is the port's `&[String]` from `search()`, compared against the
// expected elements in a slice literal (`["example.com"]`); the original's
// `ResolvConf.ndots` (`Int`) is the port's `ndots()` returning `i32`.
//
// The original inputs are Scala `"""...""".stripMargin` strings. `stripMargin`
// drops each line's leading whitespace and `|`, so the port uses the expanded
// strings: the "use the last domain element found" input keeps its final
// eight-space line (`"domain example.com\ndomain foobar.com\n        "`) and the
// tab-separated options input uses real tabs (`\t`).
//
// The port's `ResolvConfParser` and `ResolvConf` live at
// `fraktor_actor::kernel::io::dns`. Until they exist the file does not compile:
// that compile error is the intended red for this node.

use fraktor_actor::kernel::io::dns::{ResolvConf, ResolvConfParser};

fn parse(text: &str) -> ResolvConf {
  ResolvConfParser::parse_lines(text.lines()).expect("the resolv.conf must parse")
}

// "parse an actual Kubernetes resolv.conf file"
#[test]
fn parse_an_actual_kubernetes_resolv_conf_file() {
  let conf =
    parse("nameserver 172.30.0.2\nsearch myproject.svc.cluster.local svc.cluster.local cluster.local\noptions ndots:5");
  assert_eq!(conf.search(), ["myproject.svc.cluster.local", "svc.cluster.local", "cluster.local"]);
  assert_eq!(conf.ndots(), 5);
}

// "ignore # comments"
#[test]
fn ignore_hash_comments() {
  let conf = parse("search example.com\n#search foobar.com");
  assert_eq!(conf.search(), ["example.com"]);
}

// "ignore ; comments"
#[test]
fn ignore_semicolon_comments() {
  let conf = parse("search example.com\n;search foobar.com");
  assert_eq!(conf.search(), ["example.com"]);
}

// "use the last search element found"
#[test]
fn use_the_last_search_element_found() {
  let conf = parse("search example.com\nsearch foobar.com");
  assert_eq!(conf.search(), ["foobar.com"]);
}

// "support domain elements"
#[test]
fn support_domain_elements() {
  let conf = parse("domain example.com");
  assert_eq!(conf.search(), ["example.com"]);
}

// "use the last domain element found"
#[test]
fn use_the_last_domain_element_found() {
  let conf = parse("domain example.com\ndomain foobar.com\n        ");
  assert_eq!(conf.search(), ["foobar.com"]);
}

// "ignore non ndots options"
#[test]
fn ignore_non_ndots_options() {
  let conf = parse("options\trotate\tinet6\tndots:3\tattempts:4");
  assert_eq!(conf.ndots(), 3);
}

// "ignore tabs and spaces"
#[test]
fn ignore_tabs_and_spaces() {
  let conf = parse("  \t \n \t domain \t \texample.com  \t \t\n\t\t  ");
  assert_eq!(conf.search(), ["example.com"]);
}

// "default to ndots 1"
#[test]
fn default_to_ndots_1() {
  let conf = parse("");
  assert_eq!(conf.ndots(), 1);
}
