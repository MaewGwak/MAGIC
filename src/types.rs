//! Minimal type checker. It also annotates `let` and `print` with the type
//! codegen needs, so the emitter never has to re-infer anything.

use crate::ast::*;
use std::collections::HashMap;

type Vars = HashMap<String, (Ty, bool)>;

#[derive(Default)]
struct Frame {
    vars: Vars,
    arrays: HashMap<String, Ty>,
    vecs: HashMap<String, Ty>,
    is_arena: bool,
    is_loop: bool,
}

pub struct Checker {
    funcs: HashMap<String, (Vec<ParamTy>, Ty)>,
    structs: Vec<StructDef>,
    struct_index: HashMap<String, u16>,
}

impl Checker {
    pub fn check(prog: &mut Program) -> Result<(), String> {
        let mut funcs: HashMap<String, (Vec<ParamTy>, Ty)> = HashMap::new();
        for f in &prog.funcs {
            if funcs.contains_key(&f.name) {
                return Err(format!("{}: duplicate function '{}'", f.line, f.name));
            }
            let params = f.params.iter().map(|(_, t)| *t).collect();
            funcs.insert(f.name.clone(), (params, f.ret));
        }
        if !funcs.contains_key("main") {
            return Err("no fn main() found".to_string());
        }
        let mut struct_index: HashMap<String, u16> = HashMap::new();
        for (i, s) in prog.structs.iter().enumerate() {
            struct_index.insert(s.name.clone(), i as u16);
        }
        let checker = Checker {
            funcs,
            structs: prog.structs.clone(),
            struct_index,
        };
        for f in prog.funcs.iter_mut() {
            checker.check_func(f)?;
        }
        Ok(())
    }

    fn check_func(&self, f: &mut Func) -> Result<(), String> {
        if f.name == "main" && f.ret != Ty::Void {
            return Err("fn main() must not return a value".to_string());
        }
        let mut scopes: Vec<Frame> = vec![Frame::default()];
        for (name, pty) in &f.params {
            match pty {
                ParamTy::Scalar(t) => {
                    scopes[0].vars.insert(name.clone(), (*t, false));
                }
                ParamTy::Array(t) => {
                    if !(t.is_numeric() || *t == Ty::Bool) {
                        return Err(format!(
                            "array parameter '{}' must have a numeric or bool element",
                            name
                        ));
                    }
                    scopes[0].arrays.insert(name.clone(), *t);
                }
                ParamTy::Vec(t) => {
                    if !(t.is_numeric() || *t == Ty::Bool) && !matches!(t, Ty::Struct(_)) {
                        return Err(format!(
                            "growable-array parameter '{}' must have a numeric, bool or struct element",
                            name
                        ));
                    }
                    scopes[0].vecs.insert(name.clone(), *t);
                }
            }
        }
        self.check_block(&mut f.body, &mut scopes, f.ret)
    }

    fn check_block(
        &self,
        stmts: &mut [Stmt],
        scopes: &mut Vec<Frame>,
        ret: Ty,
    ) -> Result<(), String> {
        for s in stmts.iter_mut() {
            self.check_stmt(s, scopes, ret)?;
        }
        Ok(())
    }

    fn check_stmt(
        &self,
        s: &mut Stmt,
        scopes: &mut Vec<Frame>,
        ret: Ty,
    ) -> Result<(), String> {
        let line = s.line;
        match &mut s.kind {
            StmtKind::Let {
                name,
                ty,
                mutable,
                init,
            } => {
                let declared = *ty;
                let actual = self.check_expr(init, declared, scopes)?;
                if let Some(d) = declared {
                    if actual != d {
                        return Err(format!(
                            "{}: expected {}, found {}",
                            line,
                            d.c_name(),
                            actual.c_name()
                        ));
                    }
                }
                let final_ty = declared.unwrap_or(actual);
                *ty = Some(final_ty);
                let is_mut = *mutable;
                scopes
                    .last_mut()
                    .expect("scope stack is never empty")
                    .vars
                    .insert(name.clone(), (final_ty, is_mut));
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                let (t, is_mut) = lookup(scopes, name)
                    .ok_or_else(|| format!("{}: unknown variable '{}'", line, name))?;
                if !is_mut {
                    return Err(format!("{}: cannot assign to immutable '{}'", line, name));
                }
                self.check_expr(value, Some(t), scopes)?;
                Ok(())
            }
            StmtKind::If { cond, then, els } => {
                self.check_expr(cond, Some(Ty::Bool), scopes)?;
                scopes.push(Frame::default());
                let r = self.check_block(then, scopes, ret);
                scopes.pop();
                r?;
                if !els.is_empty() {
                    scopes.push(Frame::default());
                    let r = self.check_block(els, scopes, ret);
                    scopes.pop();
                    r?;
                }
                Ok(())
            }
            StmtKind::While { cond, body } => {
                self.check_expr(cond, Some(Ty::Bool), scopes)?;
                scopes.push(Frame {
                    is_loop: true,
                    ..Frame::default()
                });
                let r = self.check_block(body, scopes, ret);
                scopes.pop();
                r
            }
            StmtKind::Return(value) => {
                if scopes.iter().any(|f| f.is_arena) {
                    return Err(format!(
                        "{}: return inside an arena is not supported in v0",
                        line
                    ));
                }
                if ret == Ty::Void {
                    if value.is_some() {
                        return Err(format!("{}: cannot return a value here", line));
                    }
                } else {
                    match value {
                        Some(e) => {
                            self.check_expr(e, Some(ret), scopes)?;
                        }
                        None => return Err(format!("{}: missing return value", line)),
                    }
                }
                Ok(())
            }
            StmtKind::Print { value, ty } => {
                let t = self.check_expr(value, None, scopes)?;
                if t == Ty::Void {
                    return Err(format!("{}: cannot print a void value", line));
                }
                if matches!(t, Ty::Struct(_)) {
                    return Err(format!("{}: cannot print a struct", line));
                }
                *ty = Some(t);
                Ok(())
            }
            StmtKind::FieldPathAssign { base, path, value } => {
                let (bt, is_mut) = lookup(scopes, base)
                    .ok_or_else(|| format!("{}: unknown variable '{}'", line, base))?;
                if !is_mut {
                    return Err(format!("{}: cannot assign to immutable '{}'", line, base));
                }
                let mut ty = bt;
                for seg in &path[..path.len() - 1] {
                    let idx = match ty {
                        Ty::Struct(i) => i,
                        _ => {
                            return Err(format!(
                                "{}: '{}' is not a struct",
                                line, seg
                            ))
                        }
                    };
                    let def = &self.structs[idx as usize];
                    ty = def
                        .fields
                        .iter()
                        .find(|(n, _)| n == seg)
                        .map(|(_, t)| *t)
                        .ok_or_else(|| {
                            format!("{}: no field '{}' on '{}'", line, seg, def.name)
                        })?;
                }
                let last = path.last().expect("path is non-empty");
                let idx = match ty {
                    Ty::Struct(i) => i,
                    _ => return Err(format!("{}: '{}' is not a struct", line, base)),
                };
                let def = &self.structs[idx as usize];
                let fty = def
                    .fields
                    .iter()
                    .find(|(n, _)| n == last)
                    .map(|(_, t)| *t)
                    .ok_or_else(|| {
                        format!("{}: no field '{}' on '{}'", line, last, def.name)
                    })?;
                self.check_expr(value, Some(fty), scopes)?;
                Ok(())
            }
            StmtKind::FieldAssign { base, field, value } => {
                let (bt, is_mut) = lookup(scopes, base)
                    .ok_or_else(|| format!("{}: unknown variable '{}'", line, base))?;
                if !is_mut {
                    return Err(format!("{}: cannot assign to immutable '{}'", line, base));
                }
                let idx = match bt {
                    Ty::Struct(i) => i,
                    _ => return Err(format!("{}: '{}' is not a struct", line, base)),
                };
                let def = &self.structs[idx as usize];
                let fty = def
                    .fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| *t)
                    .ok_or_else(|| {
                        format!("{}: no field '{}' on '{}'", line, field, def.name)
                    })?;
                self.check_expr(value, Some(fty), scopes)?;
                Ok(())
            }
            StmtKind::VecLet { name, elem } => {
                if scopes.len() != 1 {
                    return Err(format!(
                        "{}: growable arrays must be declared at function top level",
                        line
                    ));
                }
                if !(elem.is_numeric() || *elem == Ty::Bool) && !matches!(elem, Ty::Struct(_)) {
                    return Err(format!(
                        "{}: vec element must be bool, u8, i32, i64, f64 or a struct",
                        line
                    ));
                }
                scopes
                    .last_mut()
                    .expect("scope stack is never empty")
                    .vecs
                    .insert(name.clone(), *elem);
                Ok(())
            }
            StmtKind::Push { name, value } => {
                let elem = lookup_vec(scopes, name)
                    .ok_or_else(|| format!("{}: '{}' is not a growable array", line, name))?;
                self.check_expr(value, Some(elem), scopes)?;
                Ok(())
            }
            StmtKind::ArrayLet {
                name,
                elem,
                value,
                count,
            } => {
                let in_arena = scopes.last().map(|f| f.is_arena).unwrap_or(false);
                if !(scopes.len() == 1 || (scopes.len() == 2 && in_arena)) {
                    return Err(format!(
                        "{}: arrays must be declared at function top level or directly inside an arena",
                        line
                    ));
                }
                if !(elem.is_numeric() || *elem == Ty::Bool) {
                    if !matches!(elem, Ty::Struct(_)) {
                        return Err(format!(
                            "{}: array element must be bool, u8, i32, i64, f64 or a struct",
                            line
                        ));
                    }
                    if in_arena {
                        return Err(format!(
                            "{}: struct arrays inside an arena are not supported yet",
                            line
                        ));
                    }
                }
                let vt = self.check_expr(value, Some(*elem), scopes)?;
                if vt != *elem {
                    return Err(format!(
                        "{}: expected {} element, found {}",
                        line,
                        elem.c_name(),
                        vt.c_name()
                    ));
                }
                let ct = self.check_expr(count, Some(Ty::I64), scopes)?;
                if !ct.is_numeric() {
                    return Err(format!("{}: array length must be an integer", line));
                }
                scopes
                    .last_mut()
                    .expect("scope stack is never empty")
                    .arrays
                    .insert(name.clone(), *elem);
                Ok(())
            }
            StmtKind::IndexFieldPathAssign {
                name,
                index,
                path,
                value,
            } => {
                let elem = lookup_array(scopes, name)
                    .ok_or_else(|| format!("{}: unknown array '{}'", line, name))?;
                if !matches!(elem, Ty::Struct(_)) {
                    return Err(format!("{}: '{}' does not hold structs", line, name));
                }
                let it = self.check_expr(index, None, scopes)?;
                if !it.is_numeric() {
                    return Err(format!("{}: index must be an integer", line));
                }
                let mut ty = elem;
                for seg in &path[..path.len() - 1] {
                    let idx = match ty {
                        Ty::Struct(i) => i,
                        _ => return Err(format!("{}: '{}' is not a struct", line, name)),
                    };
                    let def = &self.structs[idx as usize];
                    ty = def
                        .fields
                        .iter()
                        .find(|(n, _)| n == seg)
                        .map(|(_, t)| *t)
                        .ok_or_else(|| {
                            format!("{}: no field '{}' on '{}'", line, seg, def.name)
                        })?;
                }
                let last = path.last().expect("path is non-empty");
                let idx = match ty {
                    Ty::Struct(i) => i,
                    _ => return Err(format!("{}: '{}' is not a struct", line, name)),
                };
                let def = &self.structs[idx as usize];
                let fty = def
                    .fields
                    .iter()
                    .find(|(n, _)| n == last)
                    .map(|(_, t)| *t)
                    .ok_or_else(|| {
                        format!("{}: no field '{}' on '{}'", line, last, def.name)
                    })?;
                self.check_expr(value, Some(fty), scopes)?;
                Ok(())
            }
            StmtKind::IndexFieldAssign {
                name,
                index,
                field,
                value,
            } => {
                let elem = lookup_array(scopes, name)
                    .ok_or_else(|| format!("{}: unknown array '{}'", line, name))?;
                let sidx = match elem {
                    Ty::Struct(i) => i,
                    _ => {
                        return Err(format!("{}: '{}' does not hold structs", line, name))
                    }
                };
                let it = self.check_expr(index, None, scopes)?;
                if !it.is_numeric() {
                    return Err(format!("{}: index must be an integer", line));
                }
                let def = &self.structs[sidx as usize];
                let fty = def
                    .fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| *t)
                    .ok_or_else(|| {
                        format!("{}: no field '{}' on '{}'", line, field, def.name)
                    })?;
                self.check_expr(value, Some(fty), scopes)?;
                Ok(())
            }
            StmtKind::IndexAssign { name, index, value } => {
                let elem = lookup_array(scopes, name)
                    .ok_or_else(|| format!("{}: unknown array '{}'", line, name))?;
                let it = self.check_expr(index, Some(Ty::I64), scopes)?;
                if !it.is_numeric() {
                    return Err(format!("{}: index must be an integer", line));
                }
                self.check_expr(value, Some(elem), scopes)?;
                Ok(())
            }
            StmtKind::Arena { body } => {
                if scopes.iter().any(|f| f.is_arena) {
                    return Err(format!("{}: nested arenas are not supported in v0", line));
                }
                scopes.push(Frame {
                    is_arena: true,
                    ..Frame::default()
                });
                let r = self.check_block(body, scopes, ret);
                scopes.pop();
                r
            }
            StmtKind::Assert { cond } => {
                self.check_expr(cond, Some(Ty::Bool), scopes)?;
                Ok(())
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
                par: _,
            } => {
                self.check_expr(start, Some(Ty::I32), scopes)?;
                self.check_expr(end, Some(Ty::I32), scopes)?;
                scopes.push(Frame {
                    is_loop: true,
                    ..Frame::default()
                });
                scopes
                    .last_mut()
                    .expect("scope stack is never empty")
                    .vars
                    .insert(var.clone(), (Ty::I32, true));
                let r = self.check_block(body, scopes, ret);
                scopes.pop();
                r
            }
            StmtKind::Break | StmtKind::Continue => {
                if scopes.iter().any(|f| f.is_arena) {
                    return Err(format!(
                        "{}: break/continue inside an arena is not supported in v0",
                        line
                    ));
                }
                if !scopes.iter().any(|f| f.is_loop) {
                    return Err(format!("{}: break/continue outside a loop", line));
                }
                Ok(())
            }
            StmtKind::VecParamPush { .. } => Err(format!(
                "{}: internal: vec-parameter push before lowering",
                line
            )),
            StmtKind::Expr(e) => {
                self.check_expr(e, None, scopes)?;
                Ok(())
            }
        }
    }

    fn check_expr(
        &self,
        e: &Expr,
        expected: Option<Ty>,
        scopes: &[Frame],
    ) -> Result<Ty, String> {
        match &e.kind {
            ExprKind::Int(v) => {
                if let Some(t) = expected {
                    match t {
                        Ty::I32 => {
                            if *v < i32::MIN as i64 || *v > i32::MAX as i64 {
                                return Err(format!("{}: {} does not fit in i32", e.line, v));
                            }
                            return Ok(Ty::I32);
                        }
                        Ty::I64 => return Ok(Ty::I64),
                        Ty::U8 => {
                            if *v < 0 || *v > 255 {
                                return Err(format!("{}: {} does not fit in u8", e.line, v));
                            }
                            return Ok(Ty::U8);
                        }
                        Ty::F64 => return Ok(Ty::F64),
                        other => {
                            return Err(format!(
                                "{}: expected {}, found integer literal",
                                e.line,
                                other.c_name()
                            ))
                        }
                    }
                }
                Ok(Ty::I32)
            }
            ExprKind::Float(_) => {
                if let Some(t) = expected {
                    if t != Ty::F64 {
                        return Err(format!(
                            "{}: expected {}, found float literal",
                            e.line,
                            t.c_name()
                        ));
                    }
                }
                Ok(Ty::F64)
            }
            ExprKind::Str(_) => {
                if let Some(t) = expected {
                    if t != Ty::Str {
                        return Err(format!(
                            "{}: expected {}, found string literal",
                            e.line,
                            t.c_name()
                        ));
                    }
                }
                Ok(Ty::Str)
            }
            ExprKind::Bool(_) => {
                if let Some(t) = expected {
                    if t != Ty::Bool {
                        return Err(format!(
                            "{}: expected {}, found bool literal",
                            e.line,
                            t.c_name()
                        ));
                    }
                }
                Ok(Ty::Bool)
            }
            ExprKind::Var(name) => {
                if let Some((t, _)) = lookup(scopes, name) {
                    return Ok(t);
                }
                if lookup_vec(scopes, name).is_some() {
                    return Err(format!(
                        "{}: growable array '{}' cannot be used as a value",
                        e.line, name
                    ));
                }
                if lookup_array(scopes, name).is_some() {
                    return Err(format!(
                        "{}: array '{}' cannot be used as a value",
                        e.line, name
                    ));
                }
                Err(format!("{}: unknown variable '{}'", e.line, name))
            }
            ExprKind::Un { op, e: inner } => match op {
                UnOp::Neg => {
                    let t = self.check_expr(inner, expected, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: cannot negate {}", e.line, t.c_name()));
                    }
                    Ok(t)
                }
                UnOp::Not => {
                    self.check_expr(inner, Some(Ty::Bool), scopes)?;
                    Ok(Ty::Bool)
                }
            },
            ExprKind::Bin { op, l, r } => match op {
                BinOp::And | BinOp::Or => {
                    self.check_expr(l, Some(Ty::Bool), scopes)?;
                    self.check_expr(r, Some(Ty::Bool), scopes)?;
                    Ok(Ty::Bool)
                }
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                    let lt = match expected {
                        Some(t) if t.is_numeric() => self.check_expr(l, Some(t), scopes)?,
                        _ => self.check_expr(l, None, scopes)?,
                    };
                    if *op == BinOp::Add && lt == Ty::Str {
                        self.check_expr(r, Some(Ty::Str), scopes)?;
                        return Ok(Ty::Str);
                    }
                    let t = match expected {
                        Some(t) if t.is_numeric() => t,
                        _ => lt,
                    };
                    if !t.is_numeric() {
                        return Err(format!("{}: arithmetic on {}", e.line, t.c_name()));
                    }
                    self.check_expr(r, Some(t), scopes)?;
                    Ok(t)
                }
                _ => {
                    let t = self.check_expr(l, None, scopes)?;
                    if matches!(op, BinOp::Eq | BinOp::Ne) && t == Ty::Str {
                        self.check_expr(r, Some(Ty::Str), scopes)?;
                        return Ok(Ty::Bool);
                    }
                    if !(t.is_numeric() || t == Ty::Bool) {
                        return Err(format!("{}: cannot compare {}", e.line, t.c_name()));
                    }
                    self.check_expr(r, Some(t), scopes)?;
                    Ok(Ty::Bool)
                }
            },
            ExprKind::Call { name, args } => {
                if name == "len" {
                    if args.len() != 1 {
                        return Err(format!("{}: len takes 1 argument", e.line));
                    }
                    if let ExprKind::Var(vname) = &args[0].kind {
                        if lookup_array(scopes, vname).is_some() || lookup_vec(scopes, vname).is_some() {
                            return Ok(Ty::I64);
                        }
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if t == Ty::Str {
                        return Ok(Ty::I64);
                    }
                    return Err(format!("{}: len needs an array, vec or string", e.line));
                }
                if name == "min" || name == "max" {
                    if args.len() != 2 {
                        return Err(format!("{}: {} takes 2 arguments", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs numbers", e.line, name));
                    }
                    self.check_expr(&args[1], Some(t), scopes)?;
                    if !expr_is_pure(&args[0]) || !expr_is_pure(&args[1]) {
                        return Err(format!(
                            "{}: {} arguments must be call-free (they are evaluated twice)",
                            e.line, name
                        ));
                    }
                    return Ok(t);
                }
                if name == "abs" {
                    if args.len() != 1 {
                        return Err(format!("{}: abs takes 1 argument", e.line));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: abs needs a number", e.line));
                    }
                    if !expr_is_pure(&args[0]) {
                        return Err(format!(
                            "{}: abs takes a call-free expression (it is evaluated twice)",
                            e.line
                        ));
                    }
                    return Ok(t);
                }
                if name == "str_byte_n" {
                    if args.len() != 3 {
                        return Err(format!("{}: str_byte_n takes 3 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    for a in &args[1..] {
                        let t = self.check_expr(a, None, scopes)?;
                        if !t.is_numeric() {
                            return Err(format!(
                                "{}: str_byte_n index and length must be integers",
                                e.line
                            ));
                        }
                    }
                    if !expr_is_pure(&args[0]) || !expr_is_pure(&args[1]) {
                        return Err(format!(
                            "{}: str_byte_n string/index must be call-free",
                            e.line
                        ));
                    }
                    return Ok(Ty::U8);
                }
                if name == "str_byte" {
                    if args.len() != 2 {
                        return Err(format!("{}: str_byte takes 2 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    let it = self.check_expr(&args[1], None, scopes)?;
                    if !it.is_numeric() {
                        return Err(format!("{}: str_byte index must be an integer", e.line));
                    }
                    if !expr_is_pure(&args[0]) || !expr_is_pure(&args[1]) {
                        return Err(format!(
                            "{}: str_byte arguments must be free of calls (they are evaluated twice)",
                            e.line
                        ));
                    }
                    return Ok(Ty::U8);
                }
                if name == "str_len" {
                    if args.len() != 1 {
                        return Err(format!("{}: str_len takes 1 argument", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    return Ok(Ty::I64);
                }
                if name == "str_eq" {
                    if args.len() != 2 {
                        return Err(format!("{}: str_eq takes 2 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    self.check_expr(&args[1], Some(Ty::Str), scopes)?;
                    return Ok(Ty::Bool);
                }
                if name == "flush" {
                    if !args.is_empty() {
                        return Err(format!("{}: flush takes no arguments", e.line));
                    }
                    return Ok(Ty::Void);
                }
                if name == "net_listen" || name == "net_port" || name == "net_accept" || name == "net_close" {
                    if args.len() != 1 {
                        return Err(format!("{}: {} takes 1 argument", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs a socket handle or port", e.line, name));
                    }
                    return Ok(Ty::I32);
                }
                if name == "net_connect" {
                    if args.len() != 2 {
                        return Err(format!("{}: net_connect takes 2 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    let t = self.check_expr(&args[1], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: net_connect port must be an integer", e.line));
                    }
                    return Ok(Ty::I32);
                }
                if name == "net_read_at" || name == "net_write_at" {
                    if args.len() != 4 {
                        return Err(format!("{}: {} takes 4 arguments", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs a socket handle", e.line, name));
                    }
                    let ok = match &args[1].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: {} needs a [u8] array variable as its second argument",
                            e.line, name
                        ));
                    }
                    for a in &args[2..] {
                        let t = self.check_expr(a, None, scopes)?;
                        if !t.is_numeric() {
                            return Err(format!("{}: {} offset/length must be integers", e.line, name));
                        }
                    }
                    return Ok(Ty::I32);
                }
                if name == "net_read" || name == "net_write" {
                    if args.len() != 3 {
                        return Err(format!("{}: {} takes 3 arguments", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs a socket handle", e.line, name));
                    }
                    let ok = match &args[1].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: {} needs a [u8] array variable as its second argument",
                            e.line, name
                        ));
                    }
                    let lt = self.check_expr(&args[2], None, scopes)?;
                    if !lt.is_numeric() {
                        return Err(format!("{}: {} length must be an integer", e.line, name));
                    }
                    return Ok(Ty::I32);
                }
                if name == "input_read" {
                    if args.len() != 2 {
                        return Err(format!("{}: input_read takes 2 arguments", e.line));
                    }
                    let ok = match &args[0].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: input_read needs a [u8] array as its first argument",
                            e.line
                        ));
                    }
                    let t = self.check_expr(&args[1], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: input_read length must be an integer", e.line));
                    }
                    return Ok(Ty::I32);
                }
                if name == "print_bytes" {
                    if args.len() != 1 {
                        return Err(format!("{}: print_bytes takes 1 argument", e.line));
                    }
                    let ok = match &args[0].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: print_bytes needs a [u8] array or vec",
                            e.line
                        ));
                    }
                    return Ok(Ty::Void);
                }
                if name == "argc" {
                    if !args.is_empty() {
                        return Err(format!("{}: argc takes no arguments", e.line));
                    }
                    return Ok(Ty::I32);
                }
                if name == "arg" {
                    if args.len() != 1 {
                        return Err(format!("{}: arg takes 1 argument", e.line));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: arg index must be an integer", e.line));
                    }
                    return Ok(Ty::Str);
                }
                if name == "env" {
                    if args.len() != 1 {
                        return Err(format!("{}: env takes 1 argument", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    return Ok(Ty::Str);
                }
                if name == "file_open" {
                    if args.len() != 2 {
                        return Err(format!("{}: file_open takes 2 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    self.check_expr(&args[1], Some(Ty::Str), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "file_close" || name == "file_tell" || name == "file_size" {
                    if args.len() != 1 {
                        return Err(format!("{}: {} takes 1 argument", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs a file handle", e.line, name));
                    }
                    return Ok(if name == "file_close" { Ty::I32 } else { Ty::I64 });
                }
                if name == "file_pread_at" {
                    if args.len() != 5 {
                        return Err(format!("{}: file_pread_at takes 5 arguments", e.line));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: file_pread_at needs a file handle", e.line));
                    }
                    let ok = match &args[1].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: file_pread_at needs a [u8] array as its second argument",
                            e.line
                        ));
                    }
                    for a in &args[2..] {
                        let t = self.check_expr(a, None, scopes)?;
                        if !t.is_numeric() {
                            return Err(format!("{}: file_pread_at numbers must be integers", e.line));
                        }
                    }
                    return Ok(Ty::I32);
                }
                if name == "file_pread" {
                    if args.len() != 4 {
                        return Err(format!("{}: file_pread takes 4 arguments", e.line));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: file_pread needs a file handle", e.line));
                    }
                    let ok = match &args[1].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: file_pread needs a [u8] array as its second argument",
                            e.line
                        ));
                    }
                    for a in &args[2..] {
                        let t = self.check_expr(a, None, scopes)?;
                        if !t.is_numeric() {
                            return Err(format!("{}: file_pread length/offset must be integers", e.line));
                        }
                    }
                    return Ok(Ty::I32);
                }
                if name == "file_read_at" || name == "file_write_at" {
                    if args.len() != 4 {
                        return Err(format!("{}: {} takes 4 arguments", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs a file handle", e.line, name));
                    }
                    let ok = match &args[1].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: {} needs a [u8] array variable as its second argument",
                            e.line, name
                        ));
                    }
                    for a in &args[2..] {
                        let t = self.check_expr(a, None, scopes)?;
                        if !t.is_numeric() {
                            return Err(format!("{}: {} offset/length must be integers", e.line, name));
                        }
                    }
                    return Ok(Ty::I32);
                }
                if name == "file_read" || name == "file_write" {
                    if args.len() != 3 {
                        return Err(format!("{}: {} takes 3 arguments", e.line, name));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: {} needs a file handle", e.line, name));
                    }
                    let ok = match &args[1].kind {
                        ExprKind::Var(n) => lookup_array(scopes, n) == Some(Ty::U8),
                        _ => false,
                    };
                    if !ok {
                        return Err(format!(
                            "{}: {} needs a [u8] array variable as its second argument",
                            e.line, name
                        ));
                    }
                    let lt = self.check_expr(&args[2], None, scopes)?;
                    if !lt.is_numeric() {
                        return Err(format!("{}: {} length must be an integer", e.line, name));
                    }
                    return Ok(Ty::I32);
                }
                if name == "file_seek" {
                    if args.len() != 3 {
                        return Err(format!("{}: file_seek takes 3 arguments", e.line));
                    }
                    for a in args {
                        let t = self.check_expr(a, None, scopes)?;
                        if !t.is_numeric() {
                            return Err(format!("{}: file_seek arguments must be numbers", e.line));
                        }
                    }
                    return Ok(Ty::I64);
                }
                if name == "file_write_str" {
                    if args.len() != 2 {
                        return Err(format!("{}: file_write_str takes 2 arguments", e.line));
                    }
                    let t = self.check_expr(&args[0], None, scopes)?;
                    if !t.is_numeric() {
                        return Err(format!("{}: file_write_str needs a file handle", e.line));
                    }
                    self.check_expr(&args[1], Some(Ty::Str), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "pop" {
                    if args.len() != 1 {
                        return Err(format!("{}: pop takes 1 argument", e.line));
                    }
                    let vname = match &args[0].kind {
                        ExprKind::Var(n) => n,
                        _ => return Err(format!("{}: pop takes a growable array", e.line)),
                    };
                    let elem = lookup_vec(scopes, vname)
                        .ok_or_else(|| format!("{}: '{}' is not a growable array", e.line, vname))?;
                    return Ok(elem);
                }
                if name == "clear" {
                    if args.len() != 1 {
                        return Err(format!("{}: clear takes 1 argument", e.line));
                    }
                    let vname = match &args[0].kind {
                        ExprKind::Var(n) => n,
                        _ => return Err(format!("{}: clear takes a growable array", e.line)),
                    };
                    lookup_vec(scopes, vname).ok_or_else(|| {
                        format!("{}: '{}' is not a growable array", e.line, vname)
                    })?;
                    return Ok(Ty::Void);
                }
                if name == "reserve" {
                    if args.len() != 2 {
                        return Err(format!("{}: reserve takes 2 arguments", e.line));
                    }
                    let vname = match &args[0].kind {
                        ExprKind::Var(n) => n,
                        _ => {
                            return Err(format!(
                                "{}: reserve takes a growable array",
                                e.line
                            ))
                        }
                    };
                    lookup_vec(scopes, vname).ok_or_else(|| {
                        format!("{}: '{}' is not a growable array", e.line, vname)
                    })?;
                    let nt = self.check_expr(&args[1], Some(Ty::I64), scopes)?;
                    if !nt.is_numeric() {
                        return Err(format!("{}: reserve needs an integer capacity", e.line));
                    }
                    return Ok(Ty::Void);
                }
                if name == "proc_exit" {
                    if args.len() != 1 { return Err(format!("{}: proc_exit takes 1 argument", e.line)); }
                    self.check_expr(&args[0], Some(Ty::I32), scopes)?;
                    return Ok(Ty::Void);
                }
                if name == "sha1_file" {
                    if args.len() != 3 { return Err(format!("{}: sha1_file takes 3 arguments", e.line)); }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    let vname = match &args[1].kind { ExprKind::Var(n) => n, _ => return Err(format!("{}: sha1_file needs a [u8] variable", e.line)) };
                    if lookup_array(scopes, vname) != Some(Ty::U8) { return Err(format!("{}: sha1_file needs a [u8] array", e.line)); }
                    self.check_expr(&args[2], Some(Ty::I32), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "dir_list" {
                    if args.len() != 3 { return Err(format!("{}: dir_list takes 3 arguments", e.line)); }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    let vname = match &args[1].kind { ExprKind::Var(n) => n, _ => return Err(format!("{}: dir_list needs a [u8] variable", e.line)) };
                    if lookup_array(scopes, vname) != Some(Ty::U8) { return Err(format!("{}: dir_list needs a [u8] array", e.line)); }
                    self.check_expr(&args[2], Some(Ty::I32), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "str_cat" {
                    if args.len() != 2 { return Err(format!("{}: str_cat takes 2 arguments", e.line)); }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    self.check_expr(&args[1], Some(Ty::Str), scopes)?;
                    return Ok(Ty::Str);
                }
                if name == "str_from_bytes" {
                    if args.len() != 2 { return Err(format!("{}: str_from_bytes takes 2 arguments", e.line)); }
                    let vname = match &args[0].kind { ExprKind::Var(n) => n, _ => return Err(format!("{}: str_from_bytes needs a [u8] variable", e.line)) };
                    if lookup_array(scopes, vname) != Some(Ty::U8) { return Err(format!("{}: str_from_bytes needs a [u8] array", e.line)); }
                    self.check_expr(&args[1], Some(Ty::I32), scopes)?;
                    return Ok(Ty::Str);
                }
                if name == "proc_run" {
                    if args.len() != 1 { return Err(format!("{}: proc_run takes 1 argument", e.line)); }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "dir_mkdir" {
                    if args.len() != 1 { return Err(format!("{}: dir_mkdir takes 1 argument", e.line)); }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "path_exists" {
                    if args.len() != 1 { return Err(format!("{}: path_exists takes 1 argument", e.line)); }
                    self.check_expr(&args[0], Some(Ty::Str), scopes)?;
                    return Ok(Ty::Bool);
                }
                if name == "v4_splat" || name == "v4_set" || name == "v4_add"
                    || name == "v4_sub" || name == "v4_mul" || name == "v4_div"
                    || name == "v4_sqrt" || name == "v4_fma" || name == "v4_cmpgt"
                    || name == "v4_cmplt" || name == "v4_cmpeq" || name == "v4_and"
                    || name == "v4_blend" || name == "v4_max" || name == "v4_any"
                    || name == "v4_extract"
                {
                    let vid = *self.struct_index.get("V4")
                        .ok_or_else(|| format!("{}: vector intrinsics require `struct V4`", e.line))?;
                    let vt = Ty::Struct(vid);
                    let need = |n: usize| -> Result<(), String> {
                        if args.len() == n { Ok(()) } else { Err(format!("{}: {} expects {} argument(s)", e.line, name, n)) }
                    };
                    match name.as_str() {
                        "v4_splat" => { need(1)?; self.check_expr(&args[0], Some(Ty::F64), scopes)?; }
                        "v4_set" => { need(4)?; for a in args { self.check_expr(a, Some(Ty::F64), scopes)?; } }
                        "v4_add" | "v4_sub" | "v4_mul" | "v4_div" | "v4_max" | "v4_and" | "v4_cmpgt" | "v4_cmplt" | "v4_cmpeq" => {
                            need(2)?; self.check_expr(&args[0], Some(vt), scopes)?; self.check_expr(&args[1], Some(vt), scopes)?;
                        }
                        "v4_sqrt" => { need(1)?; self.check_expr(&args[0], Some(vt), scopes)?; }
                        "v4_fma" | "v4_blend" => { need(3)?; for a in args { self.check_expr(a, Some(vt), scopes)?; } }
                        "v4_any" => { need(1)?; self.check_expr(&args[0], Some(vt), scopes)?; }
                        "v4_extract" => { need(2)?; self.check_expr(&args[0], Some(vt), scopes)?; self.check_expr(&args[1], Some(Ty::I32), scopes)?; }
                        _ => {}
                    }
                    return Ok(match name.as_str() {
                        "v4_any" => Ty::Bool,
                        "v4_extract" => Ty::F64,
                        _ => vt,
                    });
                }
                if name == "cnt_init" {
                    if !args.is_empty() { return Err(format!("{}: cnt_init takes no arguments", e.line)); }
                    return Ok(Ty::Void);
                }
                if name == "cnt_set" {
                    if args.len() != 2 { return Err(format!("{}: cnt_set takes 2 arguments", e.line)); }
                    self.check_expr(&args[0], Some(Ty::I32), scopes)?;
                    self.check_expr(&args[1], Some(Ty::I32), scopes)?;
                    return Ok(Ty::Void);
                }
                if name == "cnt_dec" {
                    if args.len() != 1 { return Err(format!("{}: cnt_dec takes 1 argument", e.line)); }
                    self.check_expr(&args[0], Some(Ty::I32), scopes)?;
                    return Ok(Ty::I32);
                }
                if name == "perm_init" {
                    if !args.is_empty() {
                        return Err(format!("{}: perm_init takes no arguments", e.line));
                    }
                    return Ok(Ty::Void);
                }
                if name == "perm_rot" {
                    if args.len() != 2 {
                        return Err(format!("{}: perm_rot takes 2 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::I32), scopes)?;
                    self.check_expr(&args[1], Some(Ty::I32), scopes)?;
                    return Ok(Ty::Void);
                }
                if name == "perm_rol1" {
                    if args.len() != 1 {
                        return Err(format!("{}: perm_rol1 takes 1 argument", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::I32), scopes)?;
                    return Ok(Ty::Void);
                }
                if name == "perm_flips" {
                    if !args.is_empty() {
                        return Err(format!("{}: perm_flips takes no arguments", e.line));
                    }
                    return Ok(Ty::I32);
                }
                if name == "sqrt" {
                    if args.len() != 1 {
                        return Err(format!("{}: sqrt takes 1 argument", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::F64), scopes)?;
                    return Ok(Ty::F64);
                }
                if name == "print_prec" {
                    if args.len() != 2 {
                        return Err(format!("{}: print_prec takes 2 arguments", e.line));
                    }
                    self.check_expr(&args[0], Some(Ty::F64), scopes)?;
                    self.check_expr(&args[1], Some(Ty::I32), scopes)?;
                    return Ok(Ty::Void);
                }
                let (params, ret) = self
                    .funcs
                    .get(name)
                    .ok_or_else(|| format!("{}: unknown function '{}'", e.line, name))?;
                if params.len() != args.len() {
                    return Err(format!(
                        "{}: '{}' takes {} argument(s), got {}",
                        e.line,
                        name,
                        params.len(),
                        args.len()
                    ));
                }
                // A growable array may reallocate. Passing the same variable as
                // both a fixed `[T]` view and a `[T+]` parameter would leave the
                // view dangling after push/reserve; reject the aliasing call.
                let mut vec_args: std::collections::HashSet<&str> = Default::default();
                let mut array_args: std::collections::HashSet<&str> = Default::default();
                for (a, p) in args.iter().zip(params.iter()) {
                    if let ExprKind::Var(n) = &a.kind {
                        match p {
                            ParamTy::Vec(_) => { vec_args.insert(n.as_str()); }
                            ParamTy::Array(_) => { array_args.insert(n.as_str()); }
                            ParamTy::Scalar(_) => {}
                        }
                    }
                }
                if let Some(n) = vec_args.intersection(&array_args).next() {
                    return Err(format!(
                        "{}: cannot pass '{}' as both a fixed [T] view and a growable [T+]; push/reserve may invalidate the view",
                        e.line, n
                    ));
                }
                for (a, p) in args.iter().zip(params.iter()) {
                    match p {
                        ParamTy::Scalar(t) => {
                            self.check_expr(a, Some(*t), scopes)?;
                        }
                        ParamTy::Array(elem) => {
                            let vname = match &a.kind {
                                ExprKind::Var(n) => n,
                                _ => {
                                    return Err(format!(
                                        "{}: array argument must be a variable",
                                        a.line
                                    ))
                                }
                            };
                            match lookup_array(scopes, vname) {
                                Some(t) if t == *elem => {}
                                Some(_) => {
                                    return Err(format!(
                                        "{}: array element type mismatch for '{}'",
                                        a.line, vname
                                    ))
                                }
                                None => {
                                    return Err(format!(
                                        "{}: '{}' is not an array",
                                        a.line, vname
                                    ))
                                }
                            }
                        }
                        ParamTy::Vec(elem) => {
                            let vname = match &a.kind {
                                ExprKind::Var(n) => n,
                                _ => {
                                    return Err(format!(
                                        "{}: growable-array argument must be a variable",
                                        a.line
                                    ))
                                }
                            };
                            match lookup_vec(scopes, vname) {
                                Some(t) if t == *elem => {}
                                Some(_) => {
                                    return Err(format!(
                                        "{}: growable-array element type mismatch for '{}'",
                                        a.line, vname
                                    ))
                                }
                                None => {
                                    return Err(format!(
                                        "{}: '{}' is not a growable array",
                                        a.line, vname
                                    ))
                                }
                            }
                        }
                    }
                }
                Ok(*ret)
            }
            ExprKind::StructLit { name, fields } => {
                let idx = *self
                    .struct_index
                    .get(name)
                    .ok_or_else(|| format!("{}: unknown struct '{}'", e.line, name))?;
                let def = &self.structs[idx as usize];
                if fields.len() != def.fields.len() {
                    return Err(format!(
                        "{}: '{}' expects {} field(s), got {}",
                        e.line,
                        name,
                        def.fields.len(),
                        fields.len()
                    ));
                }
                for (fname, fty) in &def.fields {
                    let lit = fields
                        .iter()
                        .find(|(n, _)| n == fname)
                        .map(|(_, ex)| ex)
                        .ok_or_else(|| format!("{}: missing field '{}'", e.line, fname))?;
                    let got = self.check_expr(lit, Some(*fty), scopes)?;
                    if got != *fty {
                        return Err(format!(
                            "{}: field '{}' expects {}, found {}",
                            e.line,
                            fname,
                            fty.c_name(),
                            got.c_name()
                        ));
                    }
                }
                Ok(Ty::Struct(idx))
            }
            ExprKind::Field { base, field } => {
                let bt = lookup(scopes, base)
                    .map(|(t, _)| t)
                    .ok_or_else(|| format!("{}: unknown variable '{}'", e.line, base))?;
                let idx = match bt {
                    Ty::Struct(i) => i,
                    _ => return Err(format!("{}: '{}' is not a struct", e.line, base)),
                };
                let def = &self.structs[idx as usize];
                def.fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| *t)
                    .ok_or_else(|| {
                        format!("{}: no field '{}' on '{}'", e.line, field, def.name)
                    })
            }
            ExprKind::FieldPath { base, path } => {
                let bt = lookup(scopes, base)
                    .map(|(t, _)| t)
                    .ok_or_else(|| format!("{}: unknown variable '{}'", e.line, base))?;
                let mut ty = bt;
                for seg in path {
                    let idx = match ty {
                        Ty::Struct(i) => i,
                        _ => return Err(format!("{}: '{}' is not a struct", e.line, base)),
                    };
                    let def = &self.structs[idx as usize];
                    ty = def
                        .fields
                        .iter()
                        .find(|(n, _)| n == seg)
                        .map(|(_, t)| *t)
                        .ok_or_else(|| {
                            format!("{}: no field '{}' on '{}'", e.line, seg, def.name)
                        })?;
                }
                Ok(ty)
            }
            ExprKind::IndexFieldPath { name, index, path } => {
                let elem = lookup_array(scopes, name)
                    .ok_or_else(|| format!("{}: unknown array '{}'", e.line, name))?;
                if !matches!(elem, Ty::Struct(_)) {
                    return Err(format!("{}: '{}' does not hold structs", e.line, name));
                }
                let it = self.check_expr(index, None, scopes)?;
                if !it.is_numeric() {
                    return Err(format!("{}: index must be an integer", e.line));
                }
                let mut ty = elem;
                for seg in path {
                    let idx = match ty {
                        Ty::Struct(i) => i,
                        _ => return Err(format!("{}: '{}' is not a struct", e.line, name)),
                    };
                    let def = &self.structs[idx as usize];
                    ty = def
                        .fields
                        .iter()
                        .find(|(n, _)| n == seg)
                        .map(|(_, t)| *t)
                        .ok_or_else(|| {
                            format!("{}: no field '{}' on '{}'", e.line, seg, def.name)
                        })?;
                }
                Ok(ty)
            }
            ExprKind::IndexField { name, index, field } => {
                let elem = lookup_array(scopes, name)
                    .ok_or_else(|| format!("{}: unknown array '{}'", e.line, name))?;
                let sidx = match elem {
                    Ty::Struct(i) => i,
                    _ => {
                        return Err(format!("{}: '{}' does not hold structs", e.line, name))
                    }
                };
                let it = self.check_expr(index, None, scopes)?;
                if !it.is_numeric() {
                    return Err(format!("{}: index must be an integer", e.line));
                }
                let def = &self.structs[sidx as usize];
                let fty = def
                    .fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| *t)
                    .ok_or_else(|| {
                        format!("{}: no field '{}' on '{}'", e.line, field, def.name)
                    })?;
                Ok(fty)
            }
            ExprKind::Index { name, index } => {
                if lookup(scopes, name).map(|(t, _)| t) == Some(Ty::Str) {
                    let it = self.check_expr(index, Some(Ty::I64), scopes)?;
                    if !it.is_numeric() {
                        return Err(format!("{}: index must be an integer", e.line));
                    }
                    return Ok(Ty::U8);
                }
                let elem = match lookup_array(scopes, name) {
                    Some(t) => t,
                    None => lookup_vec(scopes, name).ok_or_else(|| {
                        format!("{}: unknown array '{}'", e.line, name)
                    })?,
                };
                let it = self.check_expr(index, Some(Ty::I64), scopes)?;
                if !it.is_numeric() {
                    return Err(format!("{}: index must be an integer", e.line));
                }
                Ok(elem)
            }
            ExprKind::Cast { e: inner, ty } => {
                let t = self.check_expr(inner, None, scopes)?;
                if !(t.is_numeric() || t == Ty::Bool) {
                    return Err(format!("{}: cannot cast {}", e.line, t.c_name()));
                }
                if !(ty.is_numeric() || *ty == Ty::Bool) {
                    return Err(format!("{}: cannot cast to {}", e.line, ty.c_name()));
                }
                Ok(*ty)
            }
            ExprKind::VecParamIndex { .. }
            | ExprKind::VecParamLen(_)
            | ExprKind::VecArgPtr(_)
            | ExprKind::VecArgLen(_)
            | ExprKind::VecArgCap(_)
            | ExprKind::VecParamArgPtr(_)
            | ExprKind::VecParamArgLen(_)
            | ExprKind::VecParamArgCap(_)
            | ExprKind::VecParamPop(_)
            | ExprKind::VecParamClear(_)
            | ExprKind::VecParamReserve { .. } => Err(format!(
                "{}: internal: vec-parameter node before lowering",
                e.line
            )),
            ExprKind::ArrayLen(_) => Err(format!(
                "{}: internal error: array length before lowering",
                e.line
            )),
        }
    }
}

fn lookup(frames: &[Frame], name: &str) -> Option<(Ty, bool)> {
    for s in frames.iter().rev() {
        if let Some(v) = s.vars.get(name) {
            return Some(*v);
        }
    }
    None
}

fn expr_is_pure(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Bool(_)
        | ExprKind::Var(_) => true,
        ExprKind::Un { e, .. } | ExprKind::Cast { e, .. } => expr_is_pure(e),
        ExprKind::Bin { l, r, .. } => expr_is_pure(l) && expr_is_pure(r),
        ExprKind::Index { index, .. } => expr_is_pure(index),
        _ => false,
    }
}

fn lookup_vec(frames: &[Frame], name: &str) -> Option<Ty> {
    for s in frames.iter().rev() {
        if let Some(t) = s.vecs.get(name) {
            return Some(*t);
        }
    }
    None
}

fn lookup_array(frames: &[Frame], name: &str) -> Option<Ty> {
    for s in frames.iter().rev() {
        if let Some(t) = s.arrays.get(name) {
            return Some(*t);
        }
        if let Some(t) = s.vecs.get(name) {
            return Some(*t);
        }
    }
    None
}

/// Expand array arguments at call sites into `(ptr, len)` pairs, so codegen
/// never needs the callee's signature.
pub fn lower_calls(prog: &mut Program) {
    let mut sigs: HashMap<String, Vec<ParamTy>> = HashMap::new();
    for f in &prog.funcs {
        sigs.insert(f.name.clone(), f.params.iter().map(|(_, t)| *t).collect());
    }
    for f in prog.funcs.iter_mut() {
        let mut vec_params: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (n, p) in &f.params {
            if matches!(p, ParamTy::Vec(_)) {
                vec_params.insert(n.clone());
            }
        }
        if !vec_params.is_empty() {
            rewrite_vec_names(&mut f.body, &vec_params);
        }
        VEC_PARAM_NAMES.with(|c| *c.borrow_mut() = vec_params.clone());
        lower_stmts(&mut f.body, &sigs);
        VEC_PARAM_NAMES.with(|c| c.borrow_mut().clear());
    }
}

fn rewrite_vec_names(stmts: &mut [Stmt], names: &std::collections::HashSet<String>) {
    for s in stmts.iter_mut() {
        match &mut s.kind {
            StmtKind::Let { init, .. } => rewrite_vec_expr(init, names),
            StmtKind::Assign { value, .. } => rewrite_vec_expr(value, names),
            StmtKind::IndexFieldPathAssign {
                index, value, ..
            } => {
                rewrite_vec_expr(index, names);
                rewrite_vec_expr(value, names);
            }
            StmtKind::IndexFieldAssign {
                index, value, ..
            } => {
                rewrite_vec_expr(index, names);
                rewrite_vec_expr(value, names);
            }
            StmtKind::IndexAssign { index, value, .. } => {
                rewrite_vec_expr(index, names);
                rewrite_vec_expr(value, names);
            }
            StmtKind::FieldPathAssign { value, .. } => rewrite_vec_expr(value, names),
            StmtKind::FieldAssign { value, .. } => rewrite_vec_expr(value, names),
            StmtKind::If { cond, then, els } => {
                rewrite_vec_expr(cond, names);
                rewrite_vec_names(then, names);
                rewrite_vec_names(els, names);
            }
            StmtKind::While { cond, body } => {
                rewrite_vec_expr(cond, names);
                rewrite_vec_names(body, names);
            }
            StmtKind::Return(Some(e)) => rewrite_vec_expr(e, names),
            StmtKind::Print { value, .. } => rewrite_vec_expr(value, names),
            StmtKind::Expr(e) => rewrite_vec_expr(e, names),
            StmtKind::ArrayLet { value, count, .. } => {
                rewrite_vec_expr(value, names);
                rewrite_vec_expr(count, names);
            }
            StmtKind::Arena { body } => rewrite_vec_names(body, names),
            StmtKind::Assert { cond } => rewrite_vec_expr(cond, names),
            StmtKind::For {
                start,
                end,
                body,
                ..
            } => {
                rewrite_vec_expr(start, names);
                rewrite_vec_expr(end, names);
                rewrite_vec_names(body, names);
            }
            StmtKind::Push { name, value } => {
                rewrite_vec_expr(value, names);
                if names.contains(name) {
                    let n = name.clone();
                    let v = value.clone();
                    s.kind = StmtKind::VecParamPush { name: n, value: v };
                }
            }
            _ => {}
        }
    }
}

fn rewrite_vec_expr(e: &mut Expr, names: &std::collections::HashSet<String>) {
    match &mut e.kind {
        ExprKind::Index { name, index } => {
            rewrite_vec_expr(index, names);
            if names.contains(name) {
                let n = name.clone();
                let idx = index.clone();
                e.kind = ExprKind::VecParamIndex {
                    name: n,
                    index: idx,
                };
            }
        }
        ExprKind::IndexField { index, .. } => rewrite_vec_expr(index, names),
        ExprKind::Call { name, args } => {
            if name == "pop" && args.len() == 1 {
                if let ExprKind::Var(v) = &args[0].kind {
                    if names.contains(v) {
                        let v = v.clone();
                        e.kind = ExprKind::VecParamPop(v);
                        return;
                    }
                }
            }
            if name == "clear" && args.len() == 1 {
                if let ExprKind::Var(v) = &args[0].kind {
                    if names.contains(v) {
                        let v = v.clone();
                        e.kind = ExprKind::VecParamClear(v);
                        return;
                    }
                }
            }
            if name == "reserve" && args.len() == 2 {
                if let ExprKind::Var(v) = &args[0].kind {
                    if names.contains(v) {
                        let v = v.clone();
                        let value = args[1].clone();
                        e.kind = ExprKind::VecParamReserve {
                            name: v,
                            value: Box::new(value),
                        };
                        return;
                    }
                }
            }
            if name == "len" && args.len() == 1 {
                if let ExprKind::Var(v) = &args[0].kind {
                    if names.contains(v) {
                        let v = v.clone();
                        e.kind = ExprKind::VecParamLen(v);
                        return;
                    }
                }
            }
            for a in args.iter_mut() {
                rewrite_vec_expr(a, names);
            }
        }
        ExprKind::Bin { l, r, .. } => {
            rewrite_vec_expr(l, names);
            rewrite_vec_expr(r, names);
        }
        ExprKind::Un { e, .. } | ExprKind::Cast { e, .. } => rewrite_vec_expr(e, names),
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields.iter_mut() {
                rewrite_vec_expr(v, names);
            }
        }
        _ => {}
    }
}

fn lower_stmts(stmts: &mut [Stmt], sigs: &HashMap<String, Vec<ParamTy>>) {
    for s in stmts.iter_mut() {
        match &mut s.kind {
            StmtKind::Let { init, .. } => lower_expr(init, sigs),
            StmtKind::Assign { value, .. } => lower_expr(value, sigs),
            StmtKind::IndexFieldPathAssign {
                index, value, ..
            } => {
                lower_expr(index, sigs);
                lower_expr(value, sigs);
            }
            StmtKind::IndexFieldAssign {
                index, value, ..
            } => {
                lower_expr(index, sigs);
                lower_expr(value, sigs);
            }
            StmtKind::IndexAssign { index, value, .. } => {
                lower_expr(index, sigs);
                lower_expr(value, sigs);
            }
            StmtKind::FieldPathAssign { value, .. } => lower_expr(value, sigs),
            StmtKind::FieldAssign { value, .. } => lower_expr(value, sigs),
            StmtKind::If { cond, then, els } => {
                lower_expr(cond, sigs);
                lower_stmts(then, sigs);
                lower_stmts(els, sigs);
            }
            StmtKind::While { cond, body } => {
                lower_expr(cond, sigs);
                lower_stmts(body, sigs);
            }
            StmtKind::Return(Some(e)) => lower_expr(e, sigs),
            StmtKind::Print { value, .. } => lower_expr(value, sigs),
            StmtKind::Expr(e) => lower_expr(e, sigs),
            StmtKind::ArrayLet { value, count, .. } => {
                lower_expr(value, sigs);
                lower_expr(count, sigs);
            }
            StmtKind::Arena { body } => lower_stmts(body, sigs),
            StmtKind::Assert { cond } => lower_expr(cond, sigs),
            StmtKind::For {
                start,
                end,
                body,
                ..
            } => {
                lower_expr(start, sigs);
                lower_expr(end, sigs);
                lower_stmts(body, sigs);
            }
            StmtKind::VecLet { .. } => {}
            StmtKind::Push { value, .. } => lower_expr(value, sigs),
            StmtKind::VecParamPush { value, .. } => lower_expr(value, sigs),
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Return(None) => {}
        }
    }
}

thread_local! {
    static VEC_PARAM_NAMES: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

fn lower_expr(e: &mut Expr, sigs: &HashMap<String, Vec<ParamTy>>) {
    match &mut e.kind {
        ExprKind::Call { name, args } => {
            for a in args.iter_mut() {
                lower_expr(a, sigs);
            }
            let params = match sigs.get(name) {
                Some(p) => p.clone(),
                None => return,
            };
            let mut new_args: Vec<Expr> = Vec::new();
            for (i, a) in std::mem::take(args).into_iter().enumerate() {
                let is_array = matches!(params.get(i), Some(ParamTy::Array(_)));
                let is_vec = matches!(params.get(i), Some(ParamTy::Vec(_)));
                if is_vec {
                    if let ExprKind::Var(n) = &a.kind {
                        let n = n.clone();
                        let line = a.line;
                        let is_param = VEC_PARAM_NAMES.with(|c| c.borrow().contains(&n));
                        let (ptr_kind, len_kind, cap_kind) = if is_param {
                            (
                                ExprKind::VecParamArgPtr(n.clone()),
                                ExprKind::VecParamArgLen(n.clone()),
                                ExprKind::VecParamArgCap(n),
                            )
                        } else {
                            (
                                ExprKind::VecArgPtr(n.clone()),
                                ExprKind::VecArgLen(n.clone()),
                                ExprKind::VecArgCap(n),
                            )
                        };
                        new_args.push(Expr { kind: ptr_kind, line });
                        new_args.push(Expr { kind: len_kind, line });
                        new_args.push(Expr { kind: cap_kind, line });
                        continue;
                    }
                }
                if is_array {
                    if let ExprKind::Var(n) = &a.kind {
                        let n = n.clone();
                        let line = a.line;
                        new_args.push(a);
                        new_args.push(Expr {
                            kind: ExprKind::ArrayLen(n),
                            line,
                        });
                        continue;
                    }
                }
                new_args.push(a);
            }
            *args = new_args;
        }
        ExprKind::Bin { l, r, .. } => {
            lower_expr(l, sigs);
            lower_expr(r, sigs);
        }
        ExprKind::Un { e, .. } => lower_expr(e, sigs),
        ExprKind::Cast { e, .. } => lower_expr(e, sigs),
        ExprKind::Index { index, .. } => lower_expr(index, sigs),
        ExprKind::IndexField { index, .. } => lower_expr(index, sigs),
        ExprKind::IndexFieldPath { index, .. } => lower_expr(index, sigs),
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields.iter_mut() {
                lower_expr(v, sigs);
            }
        }
        ExprKind::VecParamReserve { value, .. } => lower_expr(value, sigs),
        _ => {}
    }
}
