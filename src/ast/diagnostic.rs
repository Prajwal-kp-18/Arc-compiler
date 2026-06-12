use crate::ast::lexer::TextSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    ParseError,
    RuntimeError,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub span: Option<TextSpan>,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>, span: Option<TextSpan>) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn format_with_source(&self, source: &str) -> String {
        let prefix = match self.kind {
            DiagnosticKind::ParseError => "parse error",
            DiagnosticKind::RuntimeError => "runtime error",
        };

        let mut out = format!("{}: {}", prefix, self.message);

        if let Some(span) = &self.span {
            let (line, column) = line_col_from_offset(source, span.start);
            out.push_str(&format!(" at line {}, column {}", line, column));
        }

        if let Some(suggestion) = &self.suggestion {
            out.push_str(&format!("\nhelp: {}", suggestion));
        }

        out
    }
}

pub fn line_col_from_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;

    for (byte_idx, ch) in source.char_indices() {
        if byte_idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }

    (line, col)
}
