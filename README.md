# ratetrace

A command-line tool for one question: **given a set of rate limit rules and
a stream of requests, when does a specific request get throttled, and why?**

Rate limit configuration tends to live in scattered places — nginx configs,
API gateway YAML, a hand-rolled token bucket in application code — and it's
easy to get the numbers wrong (mixing up a per-minute rate with a per-second
one, forgetting a burst allowance) without noticing until production traffic
hits the limit. `ratetrace` reads rate limit rules from a small text format
and validates them, so a bad config gets caught immediately with an exact
pointer to the mistake instead of a stack trace at 2am.

The rules parser and its error reporting are done, and so is the token
bucket that decides whether a given request at a given time is allowed or
throttled. The `trace` subcommand ties the two together: point it at a
rules file and a trace file and it walks the trace through one bucket per
rule, in order, and reports the first request that gets throttled.

## The rules format

```
# examples/login.rl
rule login {
    rate = 5/sec
    burst = 10
}

rule password_reset {
    rate = 3/min
    burst = 1
}

rule bulk_export {
    rate = 100/hour
    burst = 20
}
```

Each `rule` block names a rate limit bucket. `rate` is `<count>/<unit>` where
unit is `sec`, `min`, or `hour`. `burst` is the number of requests allowed to
exceed the steady rate in a single spike before they start getting rejected.

## Usage

```
$ cargo run -- check examples/login.rl
login: 5 per 1s (burst 10)
password_reset: 3 per 60s (burst 1)
bulk_export: 100 per 3600s (burst 20)
```

When a rule file has a mistake, the error points at the exact line and
column, with the offending text underlined:

```
$ cat examples/broken.rl
rule login {
    rate = 5/wk
    burst = 10
}

$ cargo run -- check examples/broken.rl
error: unknown time unit 'wk'
 --> examples/broken.rl:2:14
  |
2 |     rate = 5/wk
  |              ^^
  = help: expected one of: sec, min, hour
```

Every parse error in the tool is rendered this way: message, exact source
location, the source line itself, and a caret under the specific token that's
wrong. No "syntax error somewhere in your file" — you get the line, the
column, and usually a suggestion for what was expected instead.

## Tracing a request stream

A trace file is one request per line: `<rule-name> <timestamp>`, where the
timestamp is seconds since the start of the trace. Blank lines and `#`
comments are allowed, same as the rules format.

```
# examples/login.trace
password_reset 0
password_reset 0
password_reset 0
password_reset 0
password_reset 0
```

```
$ cargo run -- trace examples/login.rl examples/login.trace
request 5 (password_reset @ 0.000s) is throttled -- examples/login.trace:7
```

`password_reset` allows a rate of 3/min plus a burst of 1, so its bucket
starts with 4 tokens; the fifth request at the same instant has nothing left
to draw from. If every request in the trace is admitted, the tool says so
instead. A trace that names a rule the rules file doesn't define, or whose
timestamps go backwards, is rejected with the same line/column diagnostics
as a bad rules file.

## Building

No external dependencies — just `cargo build` or `cargo run`.

## Roadmap

- support multi-span diagnostics for duplicate rule errors (point at both definitions)
- add unit tests for lexer/parser edge cases
- support week/day time units and fractional rates
