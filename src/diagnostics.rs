// Maps byte offsets back to 1-based line/column numbers and renders
// rustc-style error snippets. This is the piece the rest of the tool
// leans on to make every parse failure point at an exact spot in the
// source file instead of just naming what went wrong.

pub struct Diagnostic {
    pub message: String,
    pub offset: usize,
    pub len: usize,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn render(&self, file_name: &str, map: &SourceMap) -> String {
        let (line, col) = map.line_col(self.offset);
        let line_text = map.line_text(line);
        let gutter = line.to_string();
        let pad: String = " ".repeat(gutter.len());
        let caret_indent = col.saturating_sub(1);

        let mut out = String::new();
        out.push_str(&format!("error: {}\n", self.message));
        out.push_str(&format!("{pad}--> {file_name}:{line}:{col}\n"));
        out.push_str(&format!("{pad} |\n"));
        out.push_str(&format!("{gutter} | {line_text}\n"));
        out.push_str(&format!(
            "{pad} | {}{}\n",
            " ".repeat(caret_indent),
            "^".repeat(self.len.max(1))
        ));
        if let Some(help) = &self.help {
            out.push_str(&format!("{pad} = help: {help}\n"));
        }
        out
    }
}

/// Precomputes line start offsets once so repeated offset -> (line, col)
/// lookups during error reporting are cheap and don't rescan the file.
pub struct SourceMap<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> SourceMap<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        SourceMap { text, line_starts }
    }

    pub fn line_col(&self, byte_offset: usize) -> (usize, usize) {
        let idx = match self.line_starts.binary_search(&byte_offset) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let col = byte_offset - self.line_starts[idx];
        (idx + 1, col + 1)
    }

    pub fn line_text(&self, line: usize) -> &'a str {
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        self.text[start..end]
            .trim_end_matches('\n')
            .trim_end_matches('\r')
    }
}
