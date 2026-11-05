//! Hand-written lexer. No dependencies.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),

    // keywords
    Fn,
    Let,
    Var,
    If,
    Else,
    While,
    Return,
    Print,
    True,
    False,
    Struct,
    Arena,
    Import,
    As,
    Assert,
    For,
    Par,
    In,
    Break,
    Continue,

    // type keywords
    I32,
    I64,
    U8,
    F64,
    Bool,
    StrTy,

    // punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Arrow,
    Dot,
    DotDot,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,

    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Tok,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer {
    src: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(src: &str) -> Self {
        Lexer {
            src: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.src.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut out: Vec<Token> = Vec::new();

        loop {
            while matches!(self.peek(), Some(c) if c.is_whitespace()) {
                self.bump();
            }

            // line comment
            if self.peek() == Some('/') && self.peek2() == Some('/') {
                while let Some(c) = self.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.bump();
                }
                continue;
            }

            let (line, col) = (self.line, self.col);
            let c = match self.peek() {
                Some(c) => c,
                None => {
                    out.push(Token {
                        kind: Tok::Eof,
                        line,
                        col,
                    });
                    break;
                }
            };

            let kind = if c.is_ascii_alphabetic() || c == '_' {
                let mut s = String::new();
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        s.push(ch);
                        self.bump();
                    } else {
                        break;
                    }
                }
                match s.as_str() {
                    "fn" => Tok::Fn,
                    "let" => Tok::Let,
                    "var" => Tok::Var,
                    "if" => Tok::If,
                    "else" => Tok::Else,
                    "while" => Tok::While,
                    "return" => Tok::Return,
                    "print" => Tok::Print,
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "struct" => Tok::Struct,
                    "arena" => Tok::Arena,
                    "import" => Tok::Import,
                    "as" => Tok::As,
                    "assert" => Tok::Assert,
                    "for" => Tok::For,
                    "par" => Tok::Par,
                    "in" => Tok::In,
                    "break" => Tok::Break,
                    "continue" => Tok::Continue,
                    "i32" => Tok::I32,
                    "i64" => Tok::I64,
                    "u8" => Tok::U8,
                    "f64" => Tok::F64,
                    "bool" => Tok::Bool,
                    "str" => Tok::StrTy,
                    _ => Tok::Ident(s),
                }
            } else if c.is_ascii_digit() {
                let mut s = String::new();
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        s.push(ch);
                        self.bump();
                    } else {
                        break;
                    }
                }
                let mut is_float = false;
                if self.peek() == Some('.')
                    && matches!(self.peek2(), Some(d) if d.is_ascii_digit())
                {
                    is_float = true;
                    s.push('.');
                    self.bump();
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() {
                            s.push(ch);
                            self.bump();
                        } else {
                            break;
                        }
                    }
                }
                if is_float {
                    let v: f64 = s
                        .parse()
                        .map_err(|_| format!("{}:{}: bad float literal '{}'", line, col, s))?;
                    Tok::Float(v)
                } else {
                    let v: i64 = s
                        .parse()
                        .map_err(|_| format!("{}:{}: bad integer literal '{}'", line, col, s))?;
                    Tok::Int(v)
                }
            } else if c == '"' {
                self.bump();
                let mut s = String::new();
                loop {
                    match self.bump() {
                        None => {
                            return Err(format!("{}:{}: unterminated string literal", line, col))
                        }
                        Some('"') => break,
                        Some('\\') => match self.bump() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some('r') => s.push('\r'),
                            Some('\\') => s.push('\\'),
                            Some('"') => s.push('"'),
                            Some(other) => {
                                return Err(format!(
                                    "{}:{}: unknown escape '\\{}'",
                                    line, col, other
                                ))
                            }
                            None => {
                                return Err(format!(
                                    "{}:{}: unterminated string literal",
                                    line, col
                                ))
                            }
                        },
                        Some(ch) => s.push(ch),
                    }
                }
                Tok::Str(s)
            } else {
                self.bump();
                match c {
                    '(' => Tok::LParen,
                    ')' => Tok::RParen,
                    '{' => Tok::LBrace,
                    '}' => Tok::RBrace,
                    '[' => Tok::LBracket,
                    ']' => Tok::RBracket,
                    ',' => Tok::Comma,
                    ';' => Tok::Semi,
                    ':' => Tok::Colon,
                    '.' => {
                        if self.peek() == Some('.') {
                            self.bump();
                            Tok::DotDot
                        } else {
                            Tok::Dot
                        }
                    }
                    '+' => Tok::Plus,
                    '-' => {
                        if self.peek() == Some('>') {
                            self.bump();
                            Tok::Arrow
                        } else {
                            Tok::Minus
                        }
                    }
                    '*' => Tok::Star,
                    '/' => Tok::Slash,
                    '%' => Tok::Percent,
                    '=' => {
                        if self.peek() == Some('=') {
                            self.bump();
                            Tok::EqEq
                        } else {
                            Tok::Assign
                        }
                    }
                    '!' => {
                        if self.peek() == Some('=') {
                            self.bump();
                            Tok::NotEq
                        } else {
                            Tok::Bang
                        }
                    }
                    '<' => {
                        if self.peek() == Some('=') {
                            self.bump();
                            Tok::Le
                        } else {
                            Tok::Lt
                        }
                    }
                    '>' => {
                        if self.peek() == Some('=') {
                            self.bump();
                            Tok::Ge
                        } else {
                            Tok::Gt
                        }
                    }
                    '&' => {
                        if self.peek() == Some('&') {
                            self.bump();
                            Tok::AndAnd
                        } else {
                            return Err(format!(
                                "{}:{}: unexpected '&' (did you mean '&&'?)",
                                line, col
                            ));
                        }
                    }
                    '|' => {
                        if self.peek() == Some('|') {
                            self.bump();
                            Tok::OrOr
                        } else {
                            return Err(format!(
                                "{}:{}: unexpected '|' (did you mean '||'?)",
                                line, col
                            ));
                        }
                    }
                    _ => {
                        return Err(format!(
                            "{}:{}: unexpected character '{}'",
                            line, col, c
                        ))
                    }
                }
            };

            out.push(Token { kind, line, col });
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_keywords_and_ops() {
        let toks = Lexer::new("fn main() -> i32 { return 1 + 2; }")
            .tokenize()
            .unwrap();
        let kinds: Vec<Tok> = toks.into_iter().map(|t| t.kind).collect();
        assert_eq!(kinds[0], Tok::Fn);
        assert_eq!(kinds[1], Tok::Ident("main".into()));
        assert!(kinds.contains(&Tok::Arrow));
        assert!(kinds.contains(&Tok::Int(1)));
        assert_eq!(kinds[kinds.len() - 1], Tok::Eof);
    }

    #[test]
    fn tracks_line_numbers() {
        let toks = Lexer::new("fn main() {\n  let x = 1;\n}").tokenize().unwrap();
        let x = toks.iter().find(|t| t.kind == Tok::Ident("x".into())).unwrap();
        assert_eq!(x.line, 2);
    }
}
