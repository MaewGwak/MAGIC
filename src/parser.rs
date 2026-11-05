//! Recursive-descent parser for MAGIC v0.

use crate::ast::*;
use crate::lexer::{Tok, Token};

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
    structs: Vec<StructDef>,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser {
            toks,
            pos: 0,
            structs: Vec::new(),
        }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].kind
    }

    fn cur_line(&self) -> usize {
        self.toks[self.pos].line
    }

    fn cur_col(&self) -> usize {
        self.toks[self.pos].col
    }

    fn at(&self, k: &Tok) -> bool {
        self.peek() == k
    }

    fn bump(&mut self) -> Tok {
        let k = self.toks[self.pos].kind.clone();
        if !matches!(k, Tok::Eof) {
            self.pos += 1;
        }
        k
    }

    fn eat(&mut self, k: &Tok) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, k: Tok, what: &str) -> Result<(), String> {
        if self.at(&k) {
            self.bump();
            Ok(())
        } else {
            Err(format!(
                "{}:{}: expected {}, found {:?}",
                self.cur_line(),
                self.cur_col(),
                what,
                self.peek()
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        let tok = self.peek().clone();
        match tok {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            other => Err(format!(
                "{}:{}: expected identifier, found {:?}",
                self.cur_line(),
                self.cur_col(),
                other
            )),
        }
    }

    pub fn parse_program(&mut self) -> Result<Program, String> {
        let mut funcs = Vec::new();
        let mut imports = Vec::new();
        while !self.at(&Tok::Eof) {
            if self.at(&Tok::Struct) {
                self.parse_struct()?;
            } else if self.at(&Tok::Import) {
                self.bump();
                let path = match self.bump() {
                    Tok::Str(s) => s,
                    other => {
                        return Err(format!(
                            "{}: expected a string after import, found {:?}",
                            self.cur_line(),
                            other
                        ))
                    }
                };
                self.expect(Tok::Semi, "';'")?;
                imports.push(path);
            } else {
                funcs.push(self.parse_func()?);
            }
        }
        Ok(Program {
            structs: std::mem::take(&mut self.structs),
            funcs,
            imports,
        })
    }

    fn parse_struct(&mut self) -> Result<(), String> {
        self.expect(Tok::Struct, "'struct'")?;
        let name = self.expect_ident()?;
        self.expect(Tok::LBrace, "'{'")?;
        let mut fields: Vec<(String, Ty)> = Vec::new();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            let fname = self.expect_ident()?;
            self.expect(Tok::Colon, "':'")?;
            let ty = self.parse_ty()?;
            fields.push((fname, ty));
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RBrace, "'}'")?;
        if self.structs.iter().any(|s| s.name == name) {
            return Err(format!("{}: duplicate struct '{}'", self.cur_line(), name));
        }
        self.structs.push(StructDef { name, fields });
        Ok(())
    }

    fn parse_func(&mut self) -> Result<Func, String> {
        self.expect(Tok::Fn, "'fn'")?;
        let name = self.expect_ident()?;
        self.expect(Tok::LParen, "'('")?;

        let mut params: Vec<(String, ParamTy)> = Vec::new();
        if !self.at(&Tok::RParen) {
            loop {
                let pname = self.expect_ident()?;
                self.expect(Tok::Colon, "':'")?;
                let pty = if self.eat(&Tok::LBracket) {
                    let ty = self.parse_ty()?;
                    if self.eat(&Tok::Plus) {
                        self.expect(Tok::RBracket, "']'")?;
                        ParamTy::Vec(ty)
                    } else {
                        self.expect(Tok::RBracket, "']'")?;
                        ParamTy::Array(ty)
                    }
                } else {
                    ParamTy::Scalar(self.parse_ty()?)
                };
                params.push((pname, pty));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "')'")?;

        let ret = if self.eat(&Tok::Arrow) {
            self.parse_ty()?
        } else {
            Ty::Void
        };

        let body = self.parse_block()?;
        Ok(Func {
            name,
            params,
            ret,
            body,
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(Tok::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(Tok::RBrace, "'}'")?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let line = self.cur_line();
        let tok = self.peek().clone();

        match tok {
            Tok::Let | Tok::Var => {
                let mutable = matches!(tok, Tok::Var);
                self.bump();
                let name = self.expect_ident()?;
                let mut ty: Option<Ty> = None;
                if self.eat(&Tok::Colon) {
                    if self.at(&Tok::LBracket) {
                        if !mutable {
                            return Err(format!(
                                "{}: arrays must be declared with 'var'",
                                line
                            ));
                        }
                        self.bump();
                        let elem = self.parse_ty()?;
                        if self.eat(&Tok::Plus) {
                            self.expect(Tok::RBracket, "']'")?;
                            self.expect(Tok::Assign, "'='")?;
                            self.expect(Tok::LBracket, "'['")?;
                            self.expect(Tok::RBracket, "']'")?;
                            self.expect(Tok::Semi, "';'")?;
                            return Ok(Stmt {
                                kind: StmtKind::VecLet { name, elem },
                                line,
                            });
                        }
                        self.expect(Tok::RBracket, "']'")?;
                        self.expect(Tok::Assign, "'='")?;
                        self.expect(Tok::LBracket, "'['")?;
                        let value = self.parse_expr()?;
                        self.expect(Tok::Semi, "';'")?;
                        let count = self.parse_expr()?;
                        self.expect(Tok::RBracket, "']'")?;
                        self.expect(Tok::Semi, "';'")?;
                        return Ok(Stmt {
                            kind: StmtKind::ArrayLet {
                                name,
                                elem,
                                value,
                                count,
                            },
                            line,
                        });
                    }
                    ty = Some(self.parse_ty()?);
                }
                self.expect(Tok::Assign, "'='")?;
                let init = self.parse_expr()?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Let {
                        name,
                        ty,
                        mutable,
                        init,
                    },
                    line,
                })
            }
            Tok::If => {
                self.bump();
                let cond = self.parse_expr()?;
                let then = self.parse_block()?;
                let els = if self.eat(&Tok::Else) {
                    self.parse_block()?
                } else {
                    Vec::new()
                };
                Ok(Stmt {
                    kind: StmtKind::If { cond, then, els },
                    line,
                })
            }
            Tok::While => {
                self.bump();
                let cond = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Stmt {
                    kind: StmtKind::While { cond, body },
                    line,
                })
            }
            Tok::Return => {
                self.bump();
                let value = if self.at(&Tok::Semi) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Return(value),
                    line,
                })
            }
            Tok::Print => {
                self.bump();
                self.expect(Tok::LParen, "'('")?;
                let value = self.parse_expr()?;
                self.expect(Tok::RParen, "')'")?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Print { value, ty: None },
                    line,
                })
            }
            Tok::Ident(_) if self.next_is_assign() => {
                let name = self.expect_ident()?;
                self.expect(Tok::Assign, "'='")?;
                let value = self.parse_expr()?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Assign { name, value },
                    line,
                })
            }
            Tok::Ident(_) if self.next_is_index() => {
                let name = self.expect_ident()?;
                self.expect(Tok::LBracket, "'['")?;
                let index = self.parse_expr()?;
                self.expect(Tok::RBracket, "']'")?;
                if self.eat(&Tok::Dot) {
                    let mut path = vec![self.expect_ident()?];
                    while self.eat(&Tok::Dot) {
                        path.push(self.expect_ident()?);
                    }
                    self.expect(Tok::Assign, "'='")?;
                    let value = self.parse_expr()?;
                    self.expect(Tok::Semi, "';'")?;
                    return Ok(Stmt {
                        kind: if path.len() == 1 {
                            StmtKind::IndexFieldAssign {
                                name,
                                index,
                                field: path.remove(0),
                                value,
                            }
                        } else {
                            StmtKind::IndexFieldPathAssign {
                                name,
                                index,
                                path,
                                value,
                            }
                        },
                        line,
                    });
                }
                self.expect(Tok::Assign, "'='")?;
                let value = self.parse_expr()?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::IndexAssign {
                        name,
                        index,
                        value,
                    },
                    line,
                })
            }
            Tok::Ident(_) if self.next_is_push() => {
                let name = self.expect_ident()?;
                self.expect(Tok::Dot, "'.'")?;
                let method = self.expect_ident()?;
                if method != "push" {
                    return Err(format!(
                        "{}: only 'push' is supported on growable arrays",
                        line
                    ));
                }
                self.expect(Tok::LParen, "'('")?;
                let value = self.parse_expr()?;
                self.expect(Tok::RParen, "')'")?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Push { name, value },
                    line,
                })
            }
            Tok::Ident(_) if self.next_is_field_assign() => {
                let base = self.expect_ident()?;
                let mut path = Vec::new();
                while self.eat(&Tok::Dot) {
                    path.push(self.expect_ident()?);
                }
                self.expect(Tok::Assign, "'='")?;
                let value = self.parse_expr()?;
                self.expect(Tok::Semi, "';'")?;
                if path.len() == 1 {
                    Ok(Stmt {
                        kind: StmtKind::FieldAssign {
                            base,
                            field: path.remove(0),
                            value,
                        },
                        line,
                    })
                } else {
                    Ok(Stmt {
                        kind: StmtKind::FieldPathAssign { base, path, value },
                        line,
                    })
                }
            }
            Tok::Arena => {
                self.bump();
                let body = self.parse_block()?;
                Ok(Stmt {
                    kind: StmtKind::Arena { body },
                    line,
                })
            }
            Tok::For => {
                self.bump();
                let var = self.expect_ident()?;
                self.expect(Tok::In, "'in'")?;
                let start = self.parse_expr()?;
                self.expect(Tok::DotDot, "'..'")?;
                let end = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Stmt {
                    kind: StmtKind::For {
                        var,
                        start,
                        end,
                        body,
                    },
                    line,
                })
            }
            Tok::Break => {
                self.bump();
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Break,
                    line,
                })
            }
            Tok::Continue => {
                self.bump();
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Continue,
                    line,
                })
            }
            Tok::Assert => {
                self.bump();
                let cond = self.parse_expr()?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Assert { cond },
                    line,
                })
            }
            _ => {
                let e = self.parse_expr()?;
                self.expect(Tok::Semi, "';'")?;
                Ok(Stmt {
                    kind: StmtKind::Expr(e),
                    line,
                })
            }
        }
    }

    fn next_is_assign(&self) -> bool {
        self.toks.get(self.pos + 1).map(|t| &t.kind) == Some(&Tok::Assign)
    }

    fn next_is_index(&self) -> bool {
        self.toks.get(self.pos + 1).map(|t| &t.kind) == Some(&Tok::LBracket)
    }

    fn next_is_push(&self) -> bool {
        matches!(
            self.toks.get(self.pos + 1).map(|t| &t.kind),
            Some(Tok::Dot)
        ) && matches!(
            self.toks.get(self.pos + 2).map(|t| &t.kind),
            Some(Tok::Ident(_))
        ) && matches!(
            self.toks.get(self.pos + 3).map(|t| &t.kind),
            Some(Tok::LParen)
        )
    }

    fn next_is_field_assign(&self) -> bool {
        let mut i = self.pos + 1;
        loop {
            if !matches!(self.toks.get(i).map(|t| &t.kind), Some(Tok::Dot)) {
                return false;
            }
            if !matches!(self.toks.get(i + 1).map(|t| &t.kind), Some(Tok::Ident(_))) {
                return false;
            }
            i += 2;
            if matches!(self.toks.get(i).map(|t| &t.kind), Some(Tok::Assign)) {
                return true;
            }
        }
    }

    fn at_struct_lit(&self) -> bool {
        matches!(
            self.toks.get(self.pos).map(|t| &t.kind),
            Some(Tok::LBrace)
        ) && matches!(
            self.toks.get(self.pos + 1).map(|t| &t.kind),
            Some(Tok::Ident(_))
        ) && matches!(
            self.toks.get(self.pos + 2).map(|t| &t.kind),
            Some(Tok::Colon)
        )
    }

    fn parse_ty(&mut self) -> Result<Ty, String> {
        let tok = self.peek().clone();
        let ty = match tok {
            Tok::I32 => Ty::I32,
            Tok::I64 => Ty::I64,
            Tok::U8 => Ty::U8,
            Tok::F64 => Ty::F64,
            Tok::Bool => Ty::Bool,
            Tok::StrTy => Ty::Str,
            Tok::Ident(name) => {
                let idx = self.structs.iter().position(|s| s.name == name);
                match idx {
                    Some(i) => Ty::Struct(i as u16),
                    None => {
                        return Err(format!(
                            "{}:{}: unknown type '{}'",
                            self.cur_line(),
                            self.cur_col(),
                            name
                        ))
                    }
                }
            }
            other => {
                return Err(format!(
                    "{}:{}: expected type, found {:?}",
                    self.cur_line(),
                    self.cur_col(),
                    other
                ))
            }
        };
        self.bump();
        Ok(ty)
    }

    // ---- expressions: lowest to highest precedence ----

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_and()?;
        while self.eat(&Tok::OrOr) {
            let line = self.cur_line();
            let r = self.parse_and()?;
            e = Expr {
                kind: ExprKind::Bin {
                    op: BinOp::Or,
                    l: Box::new(e),
                    r: Box::new(r),
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_eq()?;
        while self.eat(&Tok::AndAnd) {
            let line = self.cur_line();
            let r = self.parse_eq()?;
            e = Expr {
                kind: ExprKind::Bin {
                    op: BinOp::And,
                    l: Box::new(e),
                    r: Box::new(r),
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_eq(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_cmp()?;
        loop {
            let op = if self.eat(&Tok::EqEq) {
                BinOp::Eq
            } else if self.eat(&Tok::NotEq) {
                BinOp::Ne
            } else {
                break;
            };
            let line = self.cur_line();
            let r = self.parse_cmp()?;
            e = Expr {
                kind: ExprKind::Bin {
                    op,
                    l: Box::new(e),
                    r: Box::new(r),
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_add()?;
        loop {
            let op = if self.eat(&Tok::Lt) {
                BinOp::Lt
            } else if self.eat(&Tok::Le) {
                BinOp::Le
            } else if self.eat(&Tok::Gt) {
                BinOp::Gt
            } else if self.eat(&Tok::Ge) {
                BinOp::Ge
            } else {
                break;
            };
            let line = self.cur_line();
            let r = self.parse_add()?;
            e = Expr {
                kind: ExprKind::Bin {
                    op,
                    l: Box::new(e),
                    r: Box::new(r),
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_mul()?;
        loop {
            let op = if self.eat(&Tok::Plus) {
                BinOp::Add
            } else if self.eat(&Tok::Minus) {
                BinOp::Sub
            } else {
                break;
            };
            let line = self.cur_line();
            let r = self.parse_mul()?;
            e = Expr {
                kind: ExprKind::Bin {
                    op,
                    l: Box::new(e),
                    r: Box::new(r),
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_unary()?;
        loop {
            let op = if self.eat(&Tok::Star) {
                BinOp::Mul
            } else if self.eat(&Tok::Slash) {
                BinOp::Div
            } else if self.eat(&Tok::Percent) {
                BinOp::Rem
            } else {
                break;
            };
            let line = self.cur_line();
            let r = self.parse_unary()?;
            e = Expr {
                kind: ExprKind::Bin {
                    op,
                    l: Box::new(e),
                    r: Box::new(r),
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        let line = self.cur_line();
        if self.eat(&Tok::Minus) {
            let e = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Un {
                    op: UnOp::Neg,
                    e: Box::new(e),
                },
                line,
            });
        }
        if self.eat(&Tok::Bang) {
            let e = self.parse_unary()?;
            return Ok(Expr {
                kind: ExprKind::Un {
                    op: UnOp::Not,
                    e: Box::new(e),
                },
                line,
            });
        }
        self.parse_cast()
    }

    fn parse_cast(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_primary()?;
        while self.at(&Tok::As) {
            let line = self.cur_line();
            self.bump();
            let ty = self.parse_ty()?;
            e = Expr {
                kind: ExprKind::Cast {
                    e: Box::new(e),
                    ty,
                },
                line,
            };
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let line = self.cur_line();
        let tok = self.peek().clone();

        match tok {
            Tok::Int(v) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Int(v),
                    line,
                })
            }
            Tok::Float(v) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Float(v),
                    line,
                })
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Str(s),
                    line,
                })
            }
            Tok::True => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Bool(true),
                    line,
                })
            }
            Tok::False => {
                self.bump();
                Ok(Expr {
                    kind: ExprKind::Bool(false),
                    line,
                })
            }
            Tok::Ident(name) => {
                self.bump();
                if self.eat(&Tok::LParen) {
                    let mut args = Vec::new();
                    if !self.at(&Tok::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if !self.eat(&Tok::Comma) {
                                break;
                            }
                        }
                    }
                    self.expect(Tok::RParen, "')'")?;
                    Ok(Expr {
                        kind: ExprKind::Call { name, args },
                        line,
                    })
                } else if self.eat(&Tok::LBracket) {
                    let index = self.parse_expr()?;
                    self.expect(Tok::RBracket, "']'")?;
                    if self.eat(&Tok::Dot) {
                        let mut path = vec![self.expect_ident()?];
                        while self.eat(&Tok::Dot) {
                            path.push(self.expect_ident()?);
                        }
                        if path.len() == 1 {
                            Ok(Expr {
                                kind: ExprKind::IndexField {
                                    name,
                                    index: Box::new(index),
                                    field: path.remove(0),
                                },
                                line,
                            })
                        } else {
                            Ok(Expr {
                                kind: ExprKind::IndexFieldPath {
                                    name,
                                    index: Box::new(index),
                                    path,
                                },
                                line,
                            })
                        }
                    } else {
                        Ok(Expr {
                            kind: ExprKind::Index {
                                name,
                                index: Box::new(index),
                            },
                            line,
                        })
                    }
                } else if self.eat(&Tok::Dot) {
                    let mut path = vec![self.expect_ident()?];
                    while self.eat(&Tok::Dot) {
                        path.push(self.expect_ident()?);
                    }
                    if path.len() == 1 {
                        Ok(Expr {
                            kind: ExprKind::Field {
                                base: name,
                                field: path.remove(0),
                            },
                            line,
                        })
                    } else {
                        Ok(Expr {
                            kind: ExprKind::FieldPath { base: name, path },
                            line,
                        })
                    }
                } else if self.at_struct_lit() {
                    self.expect(Tok::LBrace, "'{'")?;
                    let mut fields: Vec<(String, Expr)> = Vec::new();
                    while !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
                        let fname = self.expect_ident()?;
                        self.expect(Tok::Colon, "':'")?;
                        let value = self.parse_expr()?;
                        fields.push((fname, value));
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(Tok::RBrace, "'}'")?;
                    Ok(Expr {
                        kind: ExprKind::StructLit { name, fields },
                        line,
                    })
                } else {
                    Ok(Expr {
                        kind: ExprKind::Var(name),
                        line,
                    })
                }
            }
            Tok::LParen => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(Tok::RParen, "')'")?;
                Ok(e)
            }
            other => Err(format!(
                "{}:{}: expected expression, found {:?}",
                line,
                self.cur_col(),
                other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(src: &str) -> Program {
        let toks = Lexer::new(src).tokenize().unwrap();
        Parser::new(toks).parse_program().unwrap()
    }

    #[test]
    fn parses_fib() {
        let p = parse(
            "fn fib(n: i32) -> i32 { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }\n\
             fn main() { print(fib(10)); }",
        );
        assert_eq!(p.funcs.len(), 2);
        assert_eq!(p.funcs[0].name, "fib");
        assert_eq!(p.funcs[0].ret, Ty::I32);
        assert_eq!(p.funcs[1].ret, Ty::Void);
    }

    #[test]
    fn precedence() {
        let p = parse("fn main() { let x = 1 + 2 * 3; }");
        match &p.funcs[0].body[0].kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::Bin { op, r, .. } => {
                    assert_eq!(*op, BinOp::Add);
                    assert!(matches!(r.kind, ExprKind::Bin { op: BinOp::Mul, .. }));
                }
                _ => panic!("expected binop"),
            },
            _ => panic!("expected let"),
        }
    }
}
