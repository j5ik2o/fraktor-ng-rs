// SPDX-License-Identifier: Apache-2.0

//! CLI entrypoint that prints a crate's canonical public API items as JSON.

use std::{
  collections::BTreeMap,
  env,
  path::{Path, PathBuf},
  process::ExitCode,
};

use syn_check::pub_api;

const USAGE: &str = "\
usage: pub_api_list <crate-dir>...

Extracts the public API items of each crate directory using syn and writes
a JSON object mapping crate directory name to sorted item lines.
";

fn main() -> ExitCode {
  let args: Vec<String> = env::args().skip(1).collect();
  if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
    print!("{USAGE}");
    return if args.is_empty() { ExitCode::from(2) } else { ExitCode::SUCCESS };
  }

  let mut by_crate: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for raw in args {
    let dir = PathBuf::from(&raw);
    let key = crate_key(&dir);
    match pub_api::collect_crate(&dir) {
      | Ok(items) => {
        by_crate.insert(key, items);
      },
      | Err(error) => {
        eprintln!("pub_api_list: {error}");
        return ExitCode::from(1);
      },
    }
  }

  match serde_json::to_string_pretty(&by_crate) {
    | Ok(json) => {
      println!("{json}");
      ExitCode::SUCCESS
    },
    | Err(error) => {
      eprintln!("pub_api_list: failed to serialize json: {error}");
      ExitCode::from(1)
    },
  }
}

fn crate_key(dir: &Path) -> String {
  dir.file_name().and_then(|name| name.to_str()).unwrap_or("unknown").to_string()
}
