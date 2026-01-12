
#[derive(Debug, PartialEq, Clone)]
pub enum TokenKind {
    Number(i64),
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
    // Identifier,
    // Number,
    // String,
    // Keyword,
    // Operator,
    // Punctuation,
    // Whitespace,
    // Comment,
    // Error,
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
                // doubt
                TextSpan::new(0,0,'\u{0000}'.to_string())
            ))
        }
        let c: Option<char> = self.current_char();
        return c.map(|c: char| {
            let start = self.current_pos;
            let mut kind = TokenKind::Bad;

            if Self::is_number_start(&c) {
                let number: i64 = self.consume_number();
                kind = TokenKind::Number(number);
            } else if Self::is_whitespace(&c) {
                self.consume();
                kind = TokenKind::Whitespace;
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
}
