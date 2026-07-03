//! # Lexer
//!
//! Converts Arc source text into a flat stream of [`Token`]s.
//!
//! ## Usage
//!
//! ```rust
//! use arc_compiler::ast::lexer::Lexer;
//!
//! let mut lexer = Lexer::new("let x = 42");
//! while let Some(token) = lexer.next_token() {
//!     println!("{:?}", token);
//! }
//! ```
//!
//! ## Token types
//!
//! - **Literals** — `Number`, `Float`, `Boolean`, `String`
//! - **Operators** — arithmetic, bitwise, comparison, logical
//! - **Keywords** — `let`, `const`, `fn`, `return`
//! - **Delimiters** — parentheses, braces, comma, semicolon
//! - **Special** — `Identifier`, `EOF`, `Bad` (unrecognised character)
//!
//! Comments (`//` and `/* */`) are consumed and emitted as `Whitespace`,
//! which the parser filters out before processing.

/// Every distinct token kind in the Arc language.
#[derive(Debug, PartialEq, Clone)]
pub enum TokenKind {
    Number(i64),
    Float(f64),
    Boolean(bool),
    String(String),
    Plus,
    Minus,
    Asterisk,
    Slash,
    Percent,
    DoubleStar,
    Ampersand,
    Pipe,
    Caret,
    LeftShift,
    RightShift,
    // Comparison operators
    EqualEqual,
    BangEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    // Logical operators
    DoubleAmpersand,
    DoublePipe,
    Bang,
    LeftParen,
    RightParen,
    Comma,
    Colon,
    LeftBrace,
    RightBrace,
    // Assignment and keywords
    Equal,
    Let,
    Const,
    Fn,
    Return,
    Semicolon,
    // Increment and decrement operators
    PlusPlus,
    MinusMinus,
    Bad,
    EOF,
    Whitespace,
    Identifier(String),
    // Conditional
    IF,
    ELSE
}  

/// The byte range and original text of a token in the source.
#[derive(Debug, PartialEq, Clone)]
pub struct TextSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) literal: String,
}

impl TextSpan {
    pub fn new(start: usize, end: usize, literal:String) -> Self {
        Self { start, end, literal }
    }

    /// Returns the length of this span in bytes.
    pub fn length(&self) -> usize {
        self.end - self.start
    }
}

/// A single token: its kind and the source span it covers.
#[derive(Debug, PartialEq, Clone)]
pub struct Token {
     pub(crate) kind: TokenKind,
     pub(crate) span: TextSpan,
} 

impl Token {
    pub fn new(kind: TokenKind, span: TextSpan) -> Self {
        Self { kind, span }
    }
}

/// Tokenizes Arc source code.
///
/// Call [`next_token`](Lexer::next_token) repeatedly until [`TokenKind::EOF`]
/// is returned.
pub struct Lexer<'o> {
    pub input: &'o str,
    pub current_pos: usize,
}

impl<'o> Lexer<'o> {
    /// Creates a new lexer for the given source string.
    pub fn new(input: &'o str) -> Self {
        Self {
            input,
            current_pos: 0,
        }
    }

    /// Returns the next token, or `None` if the cursor is past the end.
    ///
    /// [`TokenKind::EOF`] is emitted once when the end of input is reached.
    pub fn next_token(&mut self) -> Option<Token> {
        if self.current_pos == self.input.len() {
            self.current_pos += 1;
            let eof_pos = self.input.len();
            return Some(Token::new(
                TokenKind::EOF,
                TextSpan::new(eof_pos, eof_pos, '\u{0000}'.to_string())
            ))
        }
        let c: Option<char> = self.current_char();
        return c.map(|c: char| {
            let start = self.current_pos;
            let mut kind = TokenKind::Bad;

            if Self::is_number_start(&c) {
                kind = self.consume_number_or_float();
            } else if Self::is_whitespace(&c) {
                self.consume();
                kind = TokenKind::Whitespace;
            } else if c == '"' {
                kind = self.consume_string();
            } else if Self::is_identifier_start(&c) {
                kind = self.consume_identifier();
            } else {
                kind = self.consume_punctuation();
            }

            let end = self.current_pos;
            let literal = self.input[start..end].to_string();
            let span = TextSpan::new(start, end, literal);
            Token::new(kind, span)
        });
    }

    pub fn is_whitespace(c :&char) -> bool {
        c.is_whitespace() 
    }

    /// Tokenizes a single punctuation character or multi-character operator.
    ///
    /// Performs one-character lookahead for two-character operators such as
    /// `**`, `==`, `!=`, `<<`, `>>`, `&&`, `||`, `<=`, `>=`.
    /// Comment sequences (`//`, `/*`) are consumed here and returned as
    /// [`TokenKind::Whitespace`].
    pub fn consume_punctuation(&mut self) -> TokenKind {
        let c: char = self.consume().unwrap();
        match c {
            '+' => {
                // Check for ++ (increment)
                if self.current_char() == Some('+') {
                    self.consume();
                    TokenKind::PlusPlus
                } else {
                    TokenKind::Plus
                }
            },
            '-' => {
                // Check for -- (decrement)
                if self.current_char() == Some('-') {
                    self.consume();
                    TokenKind::MinusMinus
                } else {
                    TokenKind::Minus
                }
            },
            '*' => {
                // Lookahead for ** (exponentiation) vs single * (multiply)
                if self.current_char() == Some('*') {
                    self.consume();
                    TokenKind::DoubleStar
                } else {
                    TokenKind::Asterisk
                }
            },
            '/' => {
                // Check for // (single-line comment) or /* (multi-line comment)
                if self.current_char() == Some('/') {
                    self.consume(); // consume second /
                    self.consume_single_line_comment();
                    TokenKind::Whitespace
                } else if self.current_char() == Some('*') {
                    self.consume(); // consume *
                    self.consume_multi_line_comment();
                    TokenKind::Whitespace
                } else {
                    TokenKind::Slash
                }
            },
            '%' => TokenKind::Percent,
            '&' => {
                // Check for && (logical AND)
                if self.current_char() == Some('&') {
                    self.consume();
                    TokenKind::DoubleAmpersand
                } else {
                    TokenKind::Ampersand
                }
            },
            '|' => {
                // Check for || (logical OR)
                if self.current_char() == Some('|') {
                    self.consume();
                    TokenKind::DoublePipe
                } else {
                    TokenKind::Pipe
                }
            },
            '^' => TokenKind::Caret,
            '!' => {
                // Check for != (not equal)
                if self.current_char() == Some('=') {
                    self.consume();
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                }
            },
            '=' => {
                // Check for == (equal)
                if self.current_char() == Some('=') {
                    self.consume();
                    TokenKind::EqualEqual
                } else {
                    TokenKind::Equal
                }
            },
            ';' => TokenKind::Semicolon,
            '<' => {
                // Check for << (left shift) or <= (less or equal)
                if self.current_char() == Some('<') {
                    self.consume();
                    TokenKind::LeftShift
                } else if self.current_char() == Some('=') {
                    self.consume();
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            },
            '>' => {
                // Check for >> (right shift) or >= (greater or equal)
                if self.current_char() == Some('>') {
                    self.consume();
                    TokenKind::RightShift
                } else if self.current_char() == Some('=') {
                    self.consume();
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            },
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            _ => TokenKind::Bad,
        }
    }

    pub fn is_number_start(c: &char) -> bool {
        c.is_digit(10)
    }

    pub fn is_identifier_start(c: &char) -> bool {
        c.is_alphabetic() || *c == '_'
    }

    pub fn is_identifier_continue(c: &char) -> bool {
        c.is_alphanumeric() || *c == '_'
    }

    /// Returns the char starting at the current byte offset, decoding only
    /// that one char rather than rescanning from the start of the input.
    pub fn current_char(&self) -> Option<char> {
        self.input.get(self.current_pos..)?.chars().next()
    }

    /// Advances past the current char, moving forward by its UTF-8 byte
    /// length so `current_pos` always lands on a char boundary.
    pub fn consume(&mut self) -> Option<char> {
        let c = self.current_char()?;
        self.current_pos += c.len_utf8();
        Some(c)
    }
    
    pub fn consume_number(&mut self) -> i64 {
        let mut number :i64= 0;
        while let Some(c) = self.current_char() {
            if !c.is_digit(10) {
                break;
            }
            self.consume().unwrap();
            number = number * 10 + c.to_digit(10).unwrap() as i64;
        }
        number
    }

    /// Parses an integer or floating-point literal.
    ///
    /// A `.` is treated as a decimal point only when followed by another digit,
    /// so `obj.method` is not misread as a float.
    pub fn consume_number_or_float(&mut self) -> TokenKind {
        let mut number_str = String::new();
        let mut is_float = false;
        
        // Consume integer part
        while let Some(c) = self.current_char() {
            if c.is_digit(10) {
                number_str.push(c);
                self.consume();
            } else if c == '.' && !is_float {
                // Lookahead to distinguish float (3.14) from method call (obj.method)
                if let Some(next_c) = self.peek_char(1) {
                    if next_c.is_digit(10) {
                        is_float = true;
                        number_str.push(c);
                        self.consume();
                    } else {
                        break; // Not a float, stop here
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        if is_float {
            TokenKind::Float(number_str.parse().unwrap_or(0.0))
        } else {
            TokenKind::Number(number_str.parse().unwrap_or(0))
        }
    }

    /// Parses a double-quoted string literal, handling escape sequences.
    ///
    /// Supported escapes: `\n`, `\t`, `\r`, `\\`, `\"`.
    /// Unknown escapes preserve the backslash and the following character.
    pub fn consume_string(&mut self) -> TokenKind {
        self.consume(); // consume opening quote
        let mut string = String::new();
        
        while let Some(c) = self.current_char() {
            if c == '"' {
                self.consume(); // consume closing quote
                break;
            } else if c == '\\' {
                self.consume();
                // Process escape sequences (\n, \t, etc.)
                if let Some(escaped) = self.current_char() {
                    self.consume();
                    match escaped {
                        'n' => string.push('\n'),
                        't' => string.push('\t'),
                        'r' => string.push('\r'),
                        '\\' => string.push('\\'),
                        '"' => string.push('"'),
                        _ => {
                            // Unknown escape: keep backslash and character
                            string.push('\\');
                            string.push(escaped);
                        }
                    }
                }
            } else {
                string.push(c);
                self.consume();
            }
        }
        
        TokenKind::String(string)
    }

    /// Parses an identifier or keyword.
    ///
    /// Matches `true`, `false`, `let`, `const`, `if`, `else` as keyword tokens;
    /// everything else becomes [`TokenKind::Identifier`].
    pub fn consume_identifier(&mut self) -> TokenKind {
        let mut identifier = String::new();
        
        while let Some(c) = self.current_char() {
            if Self::is_identifier_continue(&c) {
                identifier.push(c);
                self.consume();
            } else {
                break;
            }
        }
        
        // Distinguish reserved keywords from user-defined identifiers
        match identifier.as_str() {
            "true" => TokenKind::Boolean(true),
            "false" => TokenKind::Boolean(false),
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "fn" => TokenKind::Fn,
            "return" => TokenKind::Return,
            "if" => TokenKind::IF,
            "else" => TokenKind::ELSE,
            _ => TokenKind::Identifier(identifier), // User-defined name
        }
    }

    /// Returns the char `offset` positions after the current one, without advancing.
    pub fn peek_char(&self, offset: usize) -> Option<char> {
        self.input.get(self.current_pos..)?.chars().nth(offset)
    }

    /// Skips characters until (but not including) the next newline.
    pub fn consume_single_line_comment(&mut self) {
        // Consume until newline or end of input
        while let Some(c) = self.current_char() {
            if c == '\n' {
                break;
            }
            self.consume();
        }
    }

    /// Skips characters until the closing `*/`, inclusive.
    pub fn consume_multi_line_comment(&mut self) {
        // Consume until */ or end of input
        while let Some(c) = self.current_char() {
            if c == '*' && self.peek_char(1) == Some('/') {
                self.consume(); // consume *
                self.consume(); // consume /
                break;
            }
            self.consume();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
        }
        tokens
    }

    fn kinds(input: &str) -> Vec<TokenKind> {
        tokenize(input).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn test_number_and_float() {
        assert_eq!(kinds("42"), vec![TokenKind::Number(42), TokenKind::EOF]);
        assert_eq!(kinds("3.14"), vec![TokenKind::Float(3.14), TokenKind::EOF]);
    }

    #[test]
    fn test_dot_without_digit_is_not_a_float() {
        // `obj.method` shouldn't be misread as `obj` followed by a float.
        let ks = kinds("obj.method");
        assert_eq!(ks[0], TokenKind::Identifier("obj".to_string()));
    }

    #[test]
    fn test_string_with_escapes() {
        let ks = kinds(r#""a\nb\t\"c\"""#);
        assert_eq!(ks[0], TokenKind::String("a\nb\t\"c\"".to_string()));
    }

    /// Tokenizes and drops whitespace, so tests can focus on meaningful tokens.
    fn non_whitespace_kinds(input: &str) -> Vec<TokenKind> {
        kinds(input).into_iter().filter(|k| *k != TokenKind::Whitespace).collect()
    }

    #[test]
    fn test_keywords_and_identifiers() {
        assert_eq!(
            non_whitespace_kinds("let const fn return if else true false foo"),
            vec![
                TokenKind::Let,
                TokenKind::Const,
                TokenKind::Fn,
                TokenKind::Return,
                TokenKind::IF,
                TokenKind::ELSE,
                TokenKind::Boolean(true),
                TokenKind::Boolean(false),
                TokenKind::Identifier("foo".to_string()),
                TokenKind::EOF,
            ]
        );
    }

    #[test]
    fn test_two_char_operators() {
        assert_eq!(
            non_whitespace_kinds("== != <= >= && || ** << >> ++ --"),
            vec![
                TokenKind::EqualEqual,
                TokenKind::BangEqual,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::DoubleAmpersand,
                TokenKind::DoublePipe,
                TokenKind::DoubleStar,
                TokenKind::LeftShift,
                TokenKind::RightShift,
                TokenKind::PlusPlus,
                TokenKind::MinusMinus,
                TokenKind::EOF,
            ]
        );
    }

    #[test]
    fn test_comments_become_whitespace() {
        assert_eq!(
            kinds("// comment\n1"),
            vec![TokenKind::Whitespace, TokenKind::Whitespace, TokenKind::Number(1), TokenKind::EOF]
        );
        assert_eq!(
            kinds("/* block */1"),
            vec![TokenKind::Whitespace, TokenKind::Number(1), TokenKind::EOF]
        );
    }

    #[test]
    fn test_eof_span_is_end_of_input_not_zero() {
        let tokens = tokenize("let x = 1");
        let eof = tokens.last().unwrap();
        assert_eq!(eof.kind, TokenKind::EOF);
        assert_eq!(eof.span.start, "let x = 1".len());
        assert_eq!(eof.span.end, "let x = 1".len());
    }

    #[test]
    fn test_non_ascii_string_does_not_panic_and_spans_correctly() {
        // Regression test: current_pos used to be a char count sliced into
        // the byte-indexed source string, which panics or mis-slices on any
        // multi-byte character.
        let source = r#"let s = "café 🎉""#;
        let tokens = tokenize(source);
        let string_token = tokens.iter().find(|t| matches!(t.kind, TokenKind::String(_))).unwrap();
        assert_eq!(string_token.kind, TokenKind::String("café 🎉".to_string()));
        // The span's literal must match a valid slice of the original source.
        assert_eq!(&source[string_token.span.start..string_token.span.end], string_token.span.literal);
    }

    #[test]
    fn test_non_ascii_identifier_position_after_multibyte_content() {
        // A multi-byte token earlier in the source must not desync byte
        // offsets for tokens that follow it.
        let tokens = tokenize(r#"let café = 1; let after = 2;"#);
        let names: Vec<String> = tokens
            .into_iter()
            .filter_map(|t| match t.kind {
                TokenKind::Identifier(name) => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["café".to_string(), "after".to_string()]);
    }
}
