// Parser for .trace files: one request per line, `<rule-name> <timestamp>`,
// where timestamp is seconds (fractional allowed) since the start of the
// trace. Blank lines and '#' comments are ignored, matching the rules
// format. Byte offsets are tracked the same way config.rs does, so bad
// trace lines get the same line/column error treatment as bad rule files.

use crate::diagnostics::Diagnostic;
use std::time::Duration;

pub struct TraceEntry {
    pub rule: String,
    pub rule_offset: usize,
    pub rule_len: usize,
    pub at: Duration,
    pub line_start: usize,
}

pub fn parse_trace(src: &str) -> Result<Vec<TraceEntry>, Diagnostic> {
    let mut entries = Vec::new();
    let mut last: Option<Duration> = None;
    let mut offset = 0usize;

    for raw_line in src.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');

        let tokens = tokenize_line(line, line_start);
        if tokens.is_empty() || tokens[0].1.starts_with('#') {
            continue;
        }
        if tokens.len() < 2 {
            let (rule_offset, rule_text) = tokens[0];
            return Err(Diagnostic {
                message: "expected a timestamp after the rule name".to_string(),
                offset: rule_offset,
                len: rule_text.len(),
                help: Some("expected a line like 'login 1.5'".to_string()),
            });
        }
        if tokens.len() > 2 {
            let (extra_offset, _) = tokens[2];
            let line_end = line_start + line.len();
            return Err(Diagnostic {
                message: "unexpected extra text after timestamp".to_string(),
                offset: extra_offset,
                len: line_end - extra_offset,
                help: None,
            });
        }

        let (rule_offset, rule_text) = tokens[0];
        let (time_offset, time_text) = tokens[1];

        let seconds: f64 = time_text.parse().map_err(|_| Diagnostic {
            message: format!("invalid timestamp '{time_text}'"),
            offset: time_offset,
            len: time_text.len(),
            help: Some(
                "timestamps are seconds since the start of the trace, e.g. '1.5'".to_string(),
            ),
        })?;
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(Diagnostic {
                message: format!("invalid timestamp '{time_text}'"),
                offset: time_offset,
                len: time_text.len(),
                help: Some("timestamps must be non-negative".to_string()),
            });
        }
        let at = Duration::from_secs_f64(seconds);

        if let Some(prev) = last {
            if at < prev {
                return Err(Diagnostic {
                    message: "timestamps must be non-decreasing across the trace".to_string(),
                    offset: time_offset,
                    len: time_text.len(),
                    help: Some(
                        "sort the trace by timestamp, or split it into per-rule traces"
                            .to_string(),
                    ),
                });
            }
        }
        last = Some(at);

        entries.push(TraceEntry {
            rule: rule_text.to_string(),
            rule_offset,
            rule_len: rule_text.len(),
            at,
            line_start,
        });
    }

    Ok(entries)
}

/// Splits a single line (no trailing newline) into whitespace-separated
/// tokens paired with their absolute byte offset in the source.
fn tokenize_line(line: &str, line_start: usize) -> Vec<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut idx = 0;
    while idx < bytes.len() {
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        let start = idx;
        while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        tokens.push((line_start + start, &line[start..idx]));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_trace() {
        let entries = parse_trace("login 0\nlogin 0.5\n").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].rule, "login");
        assert_eq!(entries[1].at, Duration::from_millis(500));
    }

    #[test]
    fn skips_blank_lines_and_comments() {
        let entries = parse_trace("# a trace\n\nlogin 0\n\n# more\nlogin 1\n").unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn rejects_missing_timestamp() {
        let err = parse_trace("login\n").unwrap_err();
        assert!(err.message.contains("expected a timestamp"));
    }

    #[test]
    fn rejects_out_of_order_timestamps() {
        let err = parse_trace("login 5\nlogin 1\n").unwrap_err();
        assert!(err.message.contains("non-decreasing"));
    }

    #[test]
    fn rejects_trailing_garbage() {
        let err = parse_trace("login 1 extra\n").unwrap_err();
        assert!(err.message.contains("unexpected extra text"));
    }
}
