mod config;
mod diagnostics;
mod simulate;
mod trace;

use diagnostics::{Diagnostic, SourceMap};
use simulate::TokenBucket;
use std::collections::HashMap;
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
        Some("trace") => match (args.get(2), args.get(3)) {
            (Some(rules_path), Some(trace_path)) => run_trace(rules_path, trace_path),
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: ratetrace check <rules-file>");
    eprintln!("       ratetrace trace <rules-file> <trace-file>");
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

fn run_trace(rules_path: &str, trace_path: &str) -> ExitCode {
    let rules_src = match fs::read_to_string(rules_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read '{rules_path}': {e}");
            return ExitCode::FAILURE;
        }
    };
    let rules = match config::parse_rules(&rules_src) {
        Ok(r) => r,
        Err(diag) => {
            let map = SourceMap::new(&rules_src);
            eprint!("{}", diag.render(rules_path, &map));
            return ExitCode::FAILURE;
        }
    };

    let trace_src = match fs::read_to_string(trace_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read '{trace_path}': {e}");
            return ExitCode::FAILURE;
        }
    };
    let entries = match trace::parse_trace(&trace_src) {
        Ok(e) => e,
        Err(diag) => {
            let map = SourceMap::new(&trace_src);
            eprint!("{}", diag.render(trace_path, &map));
            return ExitCode::FAILURE;
        }
    };
    let trace_map = SourceMap::new(&trace_src);

    let mut buckets: HashMap<&str, TokenBucket> = HashMap::new();

    for (i, entry) in entries.iter().enumerate() {
        let Some(rule) = rules.iter().find(|r| r.name == entry.rule) else {
            let diag = Diagnostic {
                message: format!("trace references undefined rule '{}'", entry.rule),
                offset: entry.rule_offset,
                len: entry.rule_len,
                help: Some(format!(
                    "define 'rule {} {{ ... }}' in {rules_path}",
                    entry.rule
                )),
            };
            eprint!("{}", diag.render(trace_path, &trace_map));
            return ExitCode::FAILURE;
        };

        let bucket = buckets
            .entry(entry.rule.as_str())
            .or_insert_with(|| TokenBucket::new(rule));
        let outcome = bucket.admit(entry.at);

        if !outcome.allowed {
            let (line, _) = trace_map.line_col(entry.line_start);
            println!(
                "request {} ({} @ {:.3}s) is throttled -- {trace_path}:{line}",
                i + 1,
                entry.rule,
                entry.at.as_secs_f64(),
            );
            return ExitCode::SUCCESS;
        }
    }

    println!(
        "all {} requests in {trace_path} were allowed",
        entries.len()
    );
    ExitCode::SUCCESS
}

fn fmt_period(period: &Duration) -> String {
    let secs = period.as_secs_f64();
    if secs.fract() == 0.0 {
        format!("{}s", secs as u64)
    } else {
        format!("{secs}s")
    }
}
