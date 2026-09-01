// Parser for the .rl rules DSL:
//
//   rule <name> {
//       rate = <count>/<sec|min|hour>
//       burst = <count>
//   }
//
// Hand-written lexer + recursive descent parser, both tracking byte
// offsets, so every failure can be reported with an exact line/column
// and a source snippet via `diagnostics::Diagnostic`.

use crate::diagnostics::Diagnostic;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RateLimit {
    pub name: String,
    pub name_offset: usize,
    pub count: u32,
    pub period: Duration,
    pub burst: u32,
}

pub fn parse_rules(src: &str) -> Result<Vec<RateLimit>, Diagnostic> {
    let mut parser = Parser::new(src)?;
    let mut rules = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    while !matches!(parser.cur.kind, TokenKind::Eof) {
        let rule = parser.parse_rule()?;
        if seen.contains_key(&rule.name) {
            return Err(Diagnostic {
                message: format!("rule '{}' is defined more than once", rule.name),
                offset: rule.name_offset,
                len: rule.name.len(),
                help: Some("rule names must be unique within a file".to_string()),
            });
        }
        seen.insert(rule.name.clone(), rule.name_offset);
        rules.push(rule);
    }

    Ok(rules)
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Ident(String),
    Number(f64),
    Slash,
    Equals,
    LBrace,
    RBrace,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    offset: usize,
    len: usize,
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s) => format!("identifier '{s}'"),
        TokenKind::Number(n) => format!("number '{n}'"),
        TokenKind::Slash => "'/'".to_string(),
        TokenKind::Equals => "'='".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::Eof => "end of file".to_string(),
    }
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {
                    self.bump();
                }
                Some(b'#') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
    }

    fn lex_number(&mut self) -> Result<Token, Diagnostic> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }
        if self.peek() == Some(b'.') {
            self.bump();
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        let value: f64 = text.parse().map_err(|_| Diagnostic {
            message: format!("invalid number '{text}'"),
            offset: start,
            len: self.pos - start,
            help: None,
        })?;
        Ok(Token {
            kind: TokenKind::Number(value),
            offset: start,
            len: self.pos - start,
        })
    }

    fn lex_ident(&mut self) -> Result<Token, Diagnostic> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z') | Some(b'A'..=b'Z') | Some(b'0'..=b'9') | Some(b'_')
        ) {
            self.bump();
        }
        let text = std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .to_string();
        Ok(Token {
            kind: TokenKind::Ident(text),
            offset: start,
            len: self.pos - start,
        })
    }

    fn next_token(&mut self) -> Result<Token, Diagnostic> {
        self.skip_trivia();
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                offset: start,
                len: 0,
            });
        };
        match c {
            b'{' => {
                self.bump();
                Ok(Token {
                    kind: TokenKind::LBrace,
                    offset: start,
                    len: 1,
                })
            }
            b'}' => {
                self.bump();
                Ok(Token {
                    kind: TokenKind::RBrace,
                    offset: start,
                    len: 1,
                })
            }
            b'=' => {
                self.bump();
                Ok(Token {
                    kind: TokenKind::Equals,
                    offset: start,
                    len: 1,
                })
            }
            b'/' => {
                self.bump();
                Ok(Token {
                    kind: TokenKind::Slash,
                    offset: start,
                    len: 1,
                })
            }
            b'0'..=b'9' => self.lex_number(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident(),
            other => Err(Diagnostic {
                message: format!("unexpected character '{}'", other as char),
                offset: start,
                len: 1,
                help: None,
            }),
        }
    }
}

struct NumTok {
    value: f64,
    offset: usize,
    len: usize,
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    cur: Token,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Result<Self, Diagnostic> {
        let mut lexer = Lexer::new(src);
        let cur = lexer.next_token()?;
        Ok(Parser { lexer, cur })
    }

    fn bump(&mut self) -> Result<Token, Diagnostic> {
        let next = self.lexer.next_token()?;
        Ok(std::mem::replace(&mut self.cur, next))
    }

    fn unexpected(&self, what: &str) -> Diagnostic {
        Diagnostic {
            message: format!("expected {}, found {}", what, describe(&self.cur.kind)),
            offset: self.cur.offset,
            len: self.cur.len.max(1),
            help: None,
        }
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), Diagnostic> {
        match &self.cur.kind {
            TokenKind::Ident(s) if s == kw => {
                self.bump()?;
                Ok(())
            }
            _ => Err(Diagnostic {
                message: format!("expected keyword '{kw}', found {}", describe(&self.cur.kind)),
                offset: self.cur.offset,
                len: self.cur.len.max(1),
                help: None,
            }),
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<Token, Diagnostic> {
        match &self.cur.kind {
            TokenKind::Ident(_) => self.bump(),
            _ => Err(self.unexpected(what)),
        }
    }

    fn expect_number(&mut self, what: &str) -> Result<NumTok, Diagnostic> {
        match self.cur.kind {
            TokenKind::Number(v) => {
                let tok = self.bump()?;
                Ok(NumTok {
                    value: v,
                    offset: tok.offset,
                    len: tok.len,
                })
            }
            _ => Err(self.unexpected(what)),
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, Diagnostic> {
        if self.cur.kind == kind {
            self.bump()
        } else {
            Err(self.unexpected(what))
        }
    }

    fn parse_rule(&mut self) -> Result<RateLimit, Diagnostic> {
        self.expect_keyword("rule")?;
        let name_tok = self.expect_ident("a rule name")?;
        let name = match name_tok.kind {
            TokenKind::Ident(s) => s,
            _ => unreachable!(),
        };
        let name_offset = name_tok.offset;

        self.expect(TokenKind::LBrace, "'{'")?;

        let mut count: Option<u32> = None;
        let mut period: Option<Duration> = None;
        let mut burst: Option<u32> = None;

        while !matches!(self.cur.kind, TokenKind::RBrace) {
            if matches!(self.cur.kind, TokenKind::Eof) {
                return Err(Diagnostic {
                    message: "unexpected end of file while parsing rule".to_string(),
                    offset: self.cur.offset,
                    len: 1,
                    help: Some(format!("rule '{name}' is missing a closing '}}'")),
                });
            }

            let field_tok = self.expect_ident("a field name ('rate' or 'burst')")?;
            let field_name = match &field_tok.kind {
                TokenKind::Ident(s) => s.clone(),
                _ => unreachable!(),
            };
            self.expect(TokenKind::Equals, "'='")?;

            match field_name.as_str() {
                "rate" => {
                    let n_tok = self.expect_number("a request count")?;
                    self.expect(TokenKind::Slash, "'/'")?;
                    let unit_tok = self.expect_ident("a time unit (sec, min, or hour)")?;
                    let unit_name = match &unit_tok.kind {
                        TokenKind::Ident(s) => s.clone(),
                        _ => unreachable!(),
                    };
                    let secs = match unit_name.as_str() {
                        "sec" | "s" => 1.0,
                        "min" | "m" => 60.0,
                        "hour" | "h" => 3600.0,
                        other => {
                            return Err(Diagnostic {
                                message: format!("unknown time unit '{other}'"),
                                offset: unit_tok.offset,
                                len: unit_tok.len,
                                help: Some("expected one of: sec, min, hour".to_string()),
                            });
                        }
                    };
                    count = Some(expect_whole(
                        n_tok.value,
                        n_tok.offset,
                        n_tok.len,
                        "request count",
                    )?);
                    period = Some(Duration::from_secs_f64(secs));
                }
                "burst" => {
                    let n_tok = self.expect_number("a burst size")?;
                    burst = Some(expect_whole(
                        n_tok.value,
                        n_tok.offset,
                        n_tok.len,
                        "burst size",
                    )?);
                }
                other => {
                    return Err(Diagnostic {
                        message: format!("unknown field '{other}' in rule '{name}'"),
                        offset: field_tok.offset,
                        len: field_tok.len,
                        help: Some("expected 'rate' or 'burst'".to_string()),
                    });
                }
            }
        }
        self.bump()?; // consume '}'

        let count = count.ok_or_else(|| Diagnostic {
            message: format!("rule '{name}' is missing a 'rate' field"),
            offset: name_offset,
            len: name.len(),
            help: Some("add a line like 'rate = 5/sec'".to_string()),
        })?;
        let period = period.unwrap();
        let burst = burst.unwrap_or(0);

        Ok(RateLimit {
            name,
            name_offset,
            count,
            period,
            burst,
        })
    }
}

fn expect_whole(value: f64, offset: usize, len: usize, what: &str) -> Result<u32, Diagnostic> {
    if value.fract() != 0.0 || value < 0.0 || value > u32::MAX as f64 {
        return Err(Diagnostic {
            message: format!("{what} must be a whole number"),
            offset,
            len,
            help: Some("fractional or negative values are not supported".to_string()),
        });
    }
    Ok(value as u32)
}
