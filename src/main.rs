mod config;
mod diagnostics;

use diagnostics::SourceMap;
use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("check") => match args.get(2) {
            Some(path) => run_check(path),
            None => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: ratetrace check <rules-file>");
    ExitCode::FAILURE
}

fn run_check(path: &str) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read '{path}': {e}");
            return ExitCode::FAILURE;
        }
    };

    match config::parse_rules(&src) {
        Ok(rules) => {
            if rules.is_empty() {
                println!("no rules defined in {path}");
            }
            for rule in &rules {
                println!(
                    "{}: {} per {} (burst {})",
                    rule.name,
                    rule.count,
                    fmt_period(&rule.period),
                    rule.burst
                );
            }
            ExitCode::SUCCESS
        }
        Err(diag) => {
            let map = SourceMap::new(&src);
            eprint!("{}", diag.render(path, &map));
            ExitCode::FAILURE
        }
    }
}

fn fmt_period(period: &Duration) -> String {
    let secs = period.as_secs_f64();
    if secs.fract() == 0.0 {
        format!("{}s", secs as u64)
    } else {
        format!("{secs}s")
    }
}
