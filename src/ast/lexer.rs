
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
    LeftParen,
    RightParen,
    Bad,
    EOF,
    Whitespace,
    Identifier(String),
}  

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

    pub fn consume_punctuation(&mut self) -> TokenKind {
        let c: char = self.consume().unwrap();
        match c {
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => {
                // Check for ** (exponentiation)
                if self.current_char() == Some('*') {
                    self.consume();
                    TokenKind::DoubleStar
                } else {
                    TokenKind::Asterisk
                }
            },
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '&' => TokenKind::Ampersand,
            '|' => TokenKind::Pipe,
            '^' => TokenKind::Caret,
            '<' => {
                // Check for << (left shift)
                if self.current_char() == Some('<') {
                    self.consume();
                    TokenKind::LeftShift
                } else {
                    TokenKind::Bad
                }
            },
            '>' => {
                // Check for >> (right shift)
                if self.current_char() == Some('>') {
                    self.consume();
                    TokenKind::RightShift
                } else {
                    TokenKind::Bad
                }
            },
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
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
        // doubt > or >=
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

    pub fn consume_number_or_float(&mut self) -> TokenKind {
        let mut number_str = String::new();
        let mut is_float = false;
        
        // Consume integer part
        while let Some(c) = self.current_char() {
            if c.is_digit(10) {
                number_str.push(c);
                self.consume();
            } else if c == '.' && !is_float {
                // Check if next char is a digit (to distinguish from method calls)
                if let Some(next_c) = self.peek_char(1) {
                    if next_c.is_digit(10) {
                        is_float = true;
                        number_str.push(c);
                        self.consume();
                    } else {
                        break;
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

    pub fn consume_string(&mut self) -> TokenKind {
        self.consume(); // consume opening quote
        let mut string = String::new();
        
        while let Some(c) = self.current_char() {
            if c == '"' {
                self.consume(); // consume closing quote
                break;
            } else if c == '\\' {
                self.consume();
                // Handle escape sequences
                if let Some(escaped) = self.current_char() {
                    self.consume();
                    match escaped {
                        'n' => string.push('\n'),
                        't' => string.push('\t'),
                        'r' => string.push('\r'),
                        '\\' => string.push('\\'),
                        '"' => string.push('"'),
                        _ => {
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
        
        // Check for keywords (true, false)
        match identifier.as_str() {
            "true" => TokenKind::Boolean(true),
            "false" => TokenKind::Boolean(false),
            _ => TokenKind::Identifier(identifier),
        }
    }

    pub fn peek_char(&self, offset: usize) -> Option<char> {
        self.input.chars().nth(self.current_pos + offset)
    }
}
