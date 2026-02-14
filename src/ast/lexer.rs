//! Lexical analyzer - converts source code into tokens

/// Represents different token types in Arc language
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
    LeftBrace,
    RightBrace,
    // Assignment and keywords
    Equal,
    Let,
    Const,
    Semicolon,
    Bad,
    EOF,
    Whitespace,
    Identifier(String),
}  

/// Tracks location and content of a token in source code
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

    pub fn length(&self) -> usize {
        self.end - self.start
    }
}

/// A token with its type and source location
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

/// Tokenizes Arc source code into a stream of tokens
pub struct Lexer<'o> {
    pub input: &'o str,
    pub current_pos: usize,
}

impl <'o> Lexer<'o> {
    pub fn new(input: &'o str) -> Self {
        Self {
            input,
            current_pos: 0,
        }
    }

    /// Returns the next token from input stream
    pub fn next_token(&mut self) -> Option<Token> {
        if self.current_pos == self.input.len() {
            self.current_pos += 1;
            return Some(Token::new(
                TokenKind::EOF,
                TextSpan::new(0,0,'\u{0000}'.to_string())
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

    /// Handles operators and punctuation, including multi-character operators
    pub fn consume_punctuation(&mut self) -> TokenKind {
        let c: char = self.consume().unwrap();
        match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
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

    pub fn current_char(&self) -> Option<char> {
        self.input.chars().nth(self.current_pos) 
    }


    pub fn consume(&mut self) -> Option<char> {
        if self.current_pos > self.input.len() {
            return None;
        }
        let c: Option<char> = self.current_char();
        self.current_pos += 1;

        c
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

    /// Parses numeric literals (integers or floats)
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

    /// Parses string literals with escape sequence support
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

    /// Parses identifiers and keywords (let, const, true, false)
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
            _ => TokenKind::Identifier(identifier), // User-defined name
        }
    }

    pub fn peek_char(&self, offset: usize) -> Option<char> {
        self.input.chars().nth(self.current_pos + offset)
    }

    pub fn consume_single_line_comment(&mut self) {
        // Consume until newline or end of input
        while let Some(c) = self.current_char() {
            if c == '\n' {
                break;
            }
            self.consume();
        }
    }

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
