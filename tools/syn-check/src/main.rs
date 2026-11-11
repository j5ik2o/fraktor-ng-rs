use std::{env, path::PathBuf, process::ExitCode};

use syn_check::{layout::Layout, options::Options};

const USAGE: &str = "\
usage: syn-check [options] <path>...

Checks every .rs file under the given paths. The target code is not compiled.

options:
  --layout <ng|legacy>   repository layout for path-based rules (default: ng)
  --rules <id,id,...>    run only these rules (default: every rule enabled by default)
  --format <json|text>   output format (default: json)
  --list-rules           print the rule ids and exit
  -h, --help             print this help and exit

exit status: 0 no violations, 1 violations or unparsable files, 2 usage or I/O error
";

enum Format {
    Json,
    Text,
}

enum Command {
    Help,
    ListRules,
    Check {
        roots: Vec<PathBuf>,
        options: Options,
        format: Format,
    },
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut roots = Vec::new();
    let mut options = Options::default();
    let mut format = Format::Json;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--list-rules" => return Ok(Command::ListRules),
            "--layout" => {
                let name = value("--layout")?;
                options.layout =
                    Layout::parse(&name).ok_or_else(|| format!("unknown layout `{name}`"))?;
            }
            "--rules" => {
                options.rules = Some(
                    value("--rules")?
                        .split(',')
                        .map(|id| id.trim().to_string())
                        .filter(|id| !id.is_empty())
                        .collect(),
                );
            }
            "--format" => {
                format = match value("--format")?.as_str() {
                    "json" => Format::Json,
                    "text" => Format::Text,
                    other => return Err(format!("unknown format `{other}`")),
                };
            }
            option if option.starts_with('-') => return Err(format!("unknown option `{option}`")),
            path => roots.push(PathBuf::from(path)),
        }
    }
    if roots.is_empty() {
        return Err("no path given".to_string());
    }
    Ok(Command::Check {
        roots,
        options,
        format,
    })
}

fn main() -> ExitCode {
    match parse(env::args().skip(1)) {
        Ok(Command::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::ListRules) => {
            for (id, default_enabled) in syn_check::rule_ids() {
                println!(
                    "{id}{}",
                    if default_enabled {
                        ""
                    } else {
                        " (off by default)"
                    }
                );
            }
            ExitCode::SUCCESS
        }
        Ok(Command::Check {
            roots,
            options,
            format,
        }) => match syn_check::run(&roots, &options) {
            Ok(report) => {
                match format {
                    Format::Json => println!("{}", report.to_json()),
                    Format::Text => print!("{}", report.to_text()),
                }
                if report.is_clean() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(error) => {
                eprintln!("syn-check: {error}");
                ExitCode::from(2)
            }
        },
        Err(message) => {
            eprintln!("syn-check: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
