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
throttled. What's still missing is a way to feed it a trace and ask the
question end to end — see Roadmap below.

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

## Building

No external dependencies — just `cargo build` or `cargo run`.

## Roadmap

- add a `ratetrace trace` subcommand that answers "which request gets throttled first"
- support multi-span diagnostics for duplicate rule errors (point at both definitions)
- add unit tests for lexer/parser edge cases
- support week/day time units and fractional rates
