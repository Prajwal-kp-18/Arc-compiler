use crate::ast::lexer::TextSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    ParseError,
    RuntimeError,
    ResolveError,
}

/// Whether a diagnostic blocks execution or is merely informational.
///
/// Only `Error` severity stops the pipeline before evaluation; `Warning`
/// diagnostics (e.g. the Resolver falling back to a dynamically-checked
/// type) are printed but never change program behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub severity: Severity,
    pub message: String,
    pub span: Option<TextSpan>,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>, span: Option<TextSpan>) -> Self {
        Self {
            kind,
            severity: Severity::Error,
            message: message.into(),
            span,
            suggestion: None,
        }
    }

    /// Creates a non-blocking diagnostic (e.g. a Resolver type-inference fallback notice).
    pub fn warning(kind: DiagnosticKind, message: impl Into<String>, span: Option<TextSpan>) -> Self {
        Self {
            kind,
            severity: Severity::Warning,
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
        let domain = match self.kind {
            DiagnosticKind::ParseError => "parse",
            DiagnosticKind::RuntimeError => "runtime",
            DiagnosticKind::ResolveError => "resolve",
        };
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };

        let mut out = format!("{} {}: {}", domain, severity, self.message);

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

/// Finds the closest visible name to `target` (Levenshtein distance <= 2),
/// used to power "did you mean '...'?" suggestions. Shared by the Resolver
/// and the evaluator so their diagnostics agree.
pub fn closest_name(target: &str, candidates: &[String]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for candidate in candidates {
        let d = levenshtein(target, candidate);
        if d <= 2 {
            match &best {
                Some((best_d, _)) if d >= *best_d => {}
                _ => best = Some((d, candidate.clone())),
            }
        }
    }
    best.map(|(_, s)| s)
}

fn levenshtein(a: &str, b: &str) -> usize {
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr: Vec<usize> = vec![0; b_chars.len() + 1];

    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (curr[j] + 1)
                .min(prev[j + 1] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_chars.len()]
}
