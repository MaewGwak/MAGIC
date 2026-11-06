//! Typed three-address IR and a naive x86-64 emitter (prototype).
//!
//! Scope so far: integer scalars, locals, arithmetic/comparisons, integer casts,
//! `if`/`while`/`for`, `print`, `return` and calls with up to six integer
//! arguments. Arrays, structs, floats and vectors are rejected; the AST backend
//! keeps handling them until the IR grows those types.
//!
//! Every value gets a stack slot in this first emitter (no register allocation
//! yet). That is deliberately simple: the point is to establish the IR, its
//! lowering tests and the differential harness before replacing the slots with
//! linear-scan allocation.

use crate::ast::{BinOp, Expr, ExprKind, Func, ParamTy, Program, Stmt, StmtKind, Ty, UnOp};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Value(pub u32);

#[derive(Clone, Copy, Debug)]
pub enum LenRef {
    Const(i64),
    Var(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub enum Instr {
    Const { dst: Value, v: i64 },
    LoadVar { dst: Value, var: usize },
    StoreVar { var: usize, src: Value },
    Bin { dst: Value, op: Op, l: Value, r: Value },
    Un { dst: Value, neg: bool, a: Value },
    Trunc { dst: Value, ty: Ty, a: Value },
    Call { dst: Option<Value>, name: String, args: Vec<Value> },
    Print { v: Value, boolean: bool },
    Ret(Option<Value>),
    Br { cond: Value, then_bb: usize, else_bb: usize },
    Jmp(usize),
    Alloc { ptr_var: usize, elem: Ty, len: Value },
    BoundsCheck { idx: Value, len: LenRef, line: usize },
    IndexLoad { dst: Value, ptr_var: usize, idx: Value, elem: Ty },
    IndexStore { ptr_var: usize, idx: Value, elem: Ty, src: Value },
}

#[derive(Clone, Debug)]
pub struct Block {
    pub instrs: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct IrFunc {
    pub name: String,
    pub nvars: usize,
    pub nvalues: usize,
    pub nparams: usize,
    pub blocks: Vec<Block>,
}

pub struct IrProgram {
    pub funcs: Vec<IrFunc>,
}

struct Builder {
    blocks: Vec<Block>,
    cur: usize,
    vars: HashMap<String, usize>,
    nvars: usize,
    nvalues: usize,
    vbool: Vec<bool>,
    var_bool: HashMap<String, bool>,
    arrays: HashMap<String, (Ty, LenRef)>,
    ret: Ty,
}

impl Builder {
    fn new(ret: Ty) -> Self {
        Builder {
            blocks: vec![Block { instrs: Vec::new() }],
            cur: 0,
            vars: HashMap::new(),
            nvars: 0,
            nvalues: 0,
            vbool: Vec::new(),
            var_bool: HashMap::new(),
            arrays: HashMap::new(),
            ret,
        }
    }

    fn value(&mut self) -> Value {
        let v = Value(self.nvalues as u32);
        self.nvalues += 1;
        self.vbool.push(false);
        v
    }

    fn var(&mut self, name: &str) -> usize {
        if let Some(i) = self.vars.get(name) {
            return *i;
        }
        let i = self.nvars;
        self.nvars += 1;
        self.vars.insert(name.to_string(), i);
        i
    }

    fn push(&mut self, i: Instr) {
        self.blocks[self.cur].instrs.push(i);
    }

    fn new_block(&mut self) -> usize {
        self.blocks.push(Block { instrs: Vec::new() });
        self.blocks.len() - 1
    }

    fn lower_expr(&mut self, e: &Expr) -> Result<Value, String> {
        match &e.kind {
            ExprKind::Int(v) => {
                let dst = self.value();
                self.push(Instr::Const { dst, v: *v });
                Ok(dst)
            }
            ExprKind::Bool(b) => {
                let dst = self.value();
                self.push(Instr::Const { dst, v: if *b { 1 } else { 0 } });
                self.vbool[dst.0 as usize] = true;
                Ok(dst)
            }
            ExprKind::Var(n) => {
                let var = *self
                    .vars
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown variable `{}`", e.line, n))?;
                let dst = self.value();
                self.push(Instr::LoadVar { dst, var });
                self.vbool[dst.0 as usize] = self.var_bool.get(n).copied().unwrap_or(false);
                Ok(dst)
            }
            ExprKind::Bin { op, l, r } => {
                let a = self.lower_expr(l)?;
                let b = self.lower_expr(r)?;
                if let BinOp::Div | BinOp::Rem = op {
                    // the emitter guards division by zero
                    let _ = (a, b);
                }
                let op = match op {
                    BinOp::Add => Op::Add,
                    BinOp::Sub => Op::Sub,
                    BinOp::Mul => Op::Mul,
                    BinOp::Div => Op::Div,
                    BinOp::Rem => Op::Rem,
                    BinOp::Eq => Op::Eq,
                    BinOp::Ne => Op::Ne,
                    BinOp::Lt => Op::Lt,
                    BinOp::Le => Op::Le,
                    BinOp::Gt => Op::Gt,
                    BinOp::Ge => Op::Ge,
                    BinOp::And => Op::And,
                    BinOp::Or => Op::Or,
                };
                let dst = self.value();
                self.push(Instr::Bin { dst, op, l: a, r: b });
                if matches!(
                    op,
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::And | Op::Or
                ) {
                    self.vbool[dst.0 as usize] = true;
                }
                Ok(dst)
            }
            ExprKind::Un { op, e } => {
                let a = self.lower_expr(e)?;
                let dst = self.value();
                self.push(Instr::Un {
                    dst,
                    neg: *op == UnOp::Neg,
                    a,
                });
                if *op == UnOp::Not {
                    self.vbool[dst.0 as usize] = true;
                }
                Ok(dst)
            }
            ExprKind::Index { name, index } => {
                let (elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let idx = self.lower_expr(index)?;
                self.push(Instr::BoundsCheck { idx, len, line: e.line });
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let dst = self.value();
                self.push(Instr::IndexLoad { dst, ptr_var, idx, elem });
                Ok(dst)
            }
            ExprKind::Cast { e, ty } => {
                let a = self.lower_expr(e)?;
                if *ty == Ty::F64 || *ty == Ty::Str {
                    return Err(format!("line {}: IR: cast to {:?}", e.line, ty));
                }
                let dst = self.value();
                self.push(Instr::Trunc { dst, ty: *ty, a });
                self.vbool[dst.0 as usize] = *ty == Ty::Bool;
                Ok(dst)
            }
            ExprKind::ArrayLen(name) => {
                let (_, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let dst = self.value();
                match len {
                    LenRef::Const(n) => self.push(Instr::Const { dst, v: n }),
                    LenRef::Var(v) => self.push(Instr::LoadVar { dst, var: v }),
                }
                Ok(dst)
            }
            ExprKind::Call { name, args } => {
                if args.len() > 6 {
                    return Err(format!(
                        "line {}: IR: calls with more than 6 arguments are not supported yet",
                        e.line
                    ));
                }
                let mut vals = Vec::new();
                for a in args {
                    vals.push(self.lower_expr(a)?);
                }
                let dst = self.value();
                self.push(Instr::Call {
                    dst: Some(dst),
                    name: name.clone(),
                    args: vals,
                });
                Ok(dst)
            }
            other => Err(format!(
                "line {}: IR: expression not supported yet ({:?})",
                e.line, other
            )),
        }
    }

    fn lower_stmt(&mut self, s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                let v = self.lower_expr(init)?;
                let is_bool = self.vbool[v.0 as usize];
                let var = self.var(name);
                self.push(Instr::StoreVar { var, src: v });
                self.var_bool.insert(name.clone(), is_bool);
            }
            StmtKind::Assign { name, value } => {
                let v = self.lower_expr(value)?;
                let is_bool = self.vbool[v.0 as usize];
                let var = *self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("line {}: IR: unknown variable `{}`", s.line, name))?;
                self.push(Instr::StoreVar { var, src: v });
                self.var_bool.insert(name.clone(), is_bool);
            }
            StmtKind::ArrayLet { name, elem, value, count } => {
                if *elem == Ty::Bool {
                    return Err(format!(
                        "line {}: IR: `[bool]` arrays are bit-packed; the IR does not support them yet",
                        s.line
                    ));
                }
                let lenv = self.lower_expr(count)?;
                let ptr_var = self.var(name);
                self.push(Instr::Alloc { ptr_var, elem: *elem, len: lenv });
                let len_var = self.nvars;
                self.nvars += 1;
                self.push(Instr::StoreVar { var: len_var, src: lenv });
                let len_ref = LenRef::Var(len_var);
                self.arrays.insert(name.clone(), (*elem, len_ref));
                let zero = matches!(value.kind, ExprKind::Int(0))
                    || matches!(value.kind, ExprKind::Bool(false));
                if !zero {
                    let init = self.lower_expr(value)?;
                    let iv = self.nvars;
                    self.nvars += 1;
                    let z = self.value();
                    self.push(Instr::Const { dst: z, v: 0 });
                    self.push(Instr::StoreVar { var: iv, src: z });
                    let head = self.new_block();
                    let body_bb = self.new_block();
                    let incr = self.new_block();
                    let end = self.new_block();
                    self.push(Instr::Jmp(head));
                    self.cur = head;
                    let a = self.value();
                    self.push(Instr::LoadVar { dst: a, var: iv });
                    let b = self.value();
                    match len_ref {
                        LenRef::Const(n) => self.push(Instr::Const { dst: b, v: n }),
                        LenRef::Var(v) => self.push(Instr::LoadVar { dst: b, var: v }),
                    }
                    let c = self.value();
                    self.push(Instr::Bin { dst: c, op: Op::Lt, l: a, r: b });
                    self.push(Instr::Br { cond: c, then_bb: body_bb, else_bb: end });
                    self.cur = body_bb;
                    let i = self.value();
                    self.push(Instr::LoadVar { dst: i, var: iv });
                    self.push(Instr::IndexStore { ptr_var, idx: i, elem: *elem, src: init });
                    self.push(Instr::Jmp(incr));
                    self.cur = incr;
                    let i2 = self.value();
                    self.push(Instr::LoadVar { dst: i2, var: iv });
                    let one = self.value();
                    self.push(Instr::Const { dst: one, v: 1 });
                    let nv = self.value();
                    self.push(Instr::Bin { dst: nv, op: Op::Add, l: i2, r: one });
                    self.push(Instr::StoreVar { var: iv, src: nv });
                    self.push(Instr::Jmp(head));
                    self.cur = end;
                }
                let _ = len_ref;
            }
            StmtKind::IndexAssign { name, index, value } => {
                let (elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                let idx = self.lower_expr(index)?;
                self.push(Instr::BoundsCheck { idx, len, line: s.line });
                let src = self.lower_expr(value)?;
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                self.push(Instr::IndexStore { ptr_var, idx, elem, src });
            }
            StmtKind::Print { value, .. } => {
                let v = self.lower_expr(value)?;
                let boolean = self.vbool[v.0 as usize];
                self.push(Instr::Print { v, boolean });
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                };
                self.push(Instr::Ret(v));
            }
            StmtKind::Expr(e) => {
                let _ = self.lower_expr(e)?;
            }
            StmtKind::If { cond, then, els } => {
                let c = self.lower_expr(cond)?;
                let then_bb = self.new_block();
                let else_bb = self.new_block();
                let end_bb = self.new_block();
                self.push(Instr::Br { cond: c, then_bb, else_bb });
                self.cur = then_bb;
                for st in then {
                    self.lower_stmt(st)?;
                }
                self.push(Instr::Jmp(end_bb));
                self.cur = else_bb;
                for st in els {
                    self.lower_stmt(st)?;
                }
                self.push(Instr::Jmp(end_bb));
                self.cur = end_bb;
            }
            StmtKind::While { cond, body } => {
                let head = self.new_block();
                let body_bb = self.new_block();
                let end_bb = self.new_block();
                self.push(Instr::Jmp(head));
                self.cur = head;
                let c = self.lower_expr(cond)?;
                self.push(Instr::Br { cond: c, then_bb: body_bb, else_bb: end_bb });
                self.cur = body_bb;
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.push(Instr::Jmp(head));
                self.cur = end_bb;
            }
            StmtKind::For { var, start, end, body, par } => {
                if *par {
                    return Err(format!("line {}: IR: par for is not supported yet", s.line));
                }
                let sv = self.lower_expr(start)?;
                let idx = self.var(var);
                self.push(Instr::StoreVar { var: idx, src: sv });
                let head = self.new_block();
                let body_bb = self.new_block();
                let incr = self.new_block();
                let end_bb = self.new_block();
                self.push(Instr::Jmp(head));
                self.cur = head;
                let lv = self.value();
                self.push(Instr::LoadVar { dst: lv, var: idx });
                let ev = self.lower_expr(end)?;
                let cv = self.value();
                self.push(Instr::Bin { dst: cv, op: Op::Lt, l: lv, r: ev });
                self.push(Instr::Br { cond: cv, then_bb: body_bb, else_bb: end_bb });
                self.cur = body_bb;
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.push(Instr::Jmp(incr));
                self.cur = incr;
                let lv2 = self.value();
                self.push(Instr::LoadVar { dst: lv2, var: idx });
                let one = self.value();
                self.push(Instr::Const { dst: one, v: 1 });
                let nv = self.value();
                self.push(Instr::Bin { dst: nv, op: Op::Add, l: lv2, r: one });
                self.push(Instr::StoreVar { var: idx, src: nv });
                self.push(Instr::Jmp(head));
                self.cur = end_bb;
            }
            other => {
                return Err(format!(
                    "line {}: IR: statement not supported yet ({:?})",
                    s.line, other
                ))
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> IrFunc {
        IrFunc {
            name: String::new(),
            nvars: self.nvars,
            nvalues: self.nvalues,
            nparams: 0,
            blocks: std::mem::take(&mut self.blocks),
        }
    }
}

pub fn lower(prog: &Program) -> Result<IrProgram, String> {
    let mut funcs = Vec::new();
    for f in &prog.funcs {
        funcs.push(lower_func(f)?);
    }
    Ok(IrProgram { funcs })
}

fn lower_func(f: &Func) -> Result<IrFunc, String> {
    if f.ret == Ty::Struct(0) || matches!(f.ret, Ty::F64 | Ty::Str | Ty::Void) && f.ret == Ty::F64 {
        return Err(format!("IR: `{}` has an unsupported return type", f.name));
    }
    if f.ret == Ty::F64 || f.ret == Ty::Str {
        return Err(format!("IR: `{}` has an unsupported return type", f.name));
    }
    let mut b = Builder::new(f.ret);
    let mut nparams = 0usize;
    for (name, pty) in &f.params {
        match pty {
            ParamTy::Scalar(_) => {
                b.var(name);
                nparams += 1;
            }
            ParamTy::Array(elem) => {
                if *elem == Ty::Bool {
                    return Err(format!(
                        "IR: `{}` takes a bit-packed `[bool]` parameter, not supported yet",
                        f.name
                    ));
                }
                let pv = b.var(name);
                let lv = b.nvars;
                b.nvars += 1;
                b.arrays.insert(name.clone(), (*elem, LenRef::Var(lv)));
                let _ = pv;
                nparams += 2;
            }
            ParamTy::Vec(_) => {
                return Err(format!(
                    "IR: `{}` takes vec parameters, not supported yet",
                    f.name
                ))
            }
        }
    }
    for st in &f.body {
        b.lower_stmt(st)?;
    }
    if !matches!(b.blocks[b.cur].instrs.last(), Some(Instr::Ret(_))) {
        b.push(Instr::Ret(None));
    }
    let mut out = b.finish();
    out.name = f.name.clone();
    out.nparams = nparams;
    Ok(out)
}

fn voff(f: &IrFunc, v: Value) -> i64 {
    8 * (f.nvars as i64 + v.0 as i64 + 1)
}

fn varoff(i: usize) -> i64 {
    8 * (i as i64 + 1)
}

fn instr_uses_defs(ins: &Instr, uses: &mut Vec<Value>, defs: &mut Vec<Value>) {
    match ins {
        Instr::Const { dst, .. } => defs.push(*dst),
        Instr::LoadVar { dst, .. } => defs.push(*dst),
        Instr::StoreVar { src, .. } => uses.push(*src),
        Instr::Bin { dst, l, r, .. } => {
            uses.push(*l);
            uses.push(*r);
            defs.push(*dst);
        }
        Instr::Un { dst, a, .. } => {
            uses.push(*a);
            defs.push(*dst);
        }
        Instr::Trunc { dst, a, .. } => {
            uses.push(*a);
            defs.push(*dst);
        }
        Instr::Call { dst, args, .. } => {
            uses.extend(args.iter().copied());
            if let Some(d) = dst {
                defs.push(*d);
            }
        }
        Instr::Print { v, .. } => uses.push(*v),
        Instr::Ret(v) => {
            if let Some(v) = v {
                uses.push(*v);
            }
        }
        Instr::Br { cond, .. } => uses.push(*cond),
        Instr::Jmp(_) => {}
        Instr::Alloc { len, .. } => uses.push(*len),
        Instr::BoundsCheck { idx, .. } => uses.push(*idx),
        Instr::IndexLoad { dst, idx, .. } => {
            uses.push(*idx);
            defs.push(*dst);
        }
        Instr::IndexStore { idx, src, .. } => {
            uses.push(*idx);
            uses.push(*src);
        }
    }
}

fn block_successors(instrs: &[Instr]) -> Vec<usize> {
    match instrs.last() {
        Some(Instr::Br { then_bb, else_bb, .. }) => vec![*then_bb, *else_bb],
        Some(Instr::Jmp(bb)) => vec![*bb],
        _ => Vec::new(),
    }
}

/// Blocks that call out (printf/calloc/panic) or divide need the simple
/// slot-based emitter; everything else runs through the register allocator.
fn slow_block(instrs: &[Instr]) -> bool {
    instrs.iter().any(|i| {
        matches!(
            i,
            Instr::Call { .. }
                | Instr::Alloc { .. }
                | Instr::Print { .. }
                | Instr::Bin { op: Op::Div | Op::Rem, .. }
        )
    })
}

const POOL: [(&str, &str, &str); 6] = [
    ("%rdx", "%edx", "%dl"),
    ("%rsi", "%esi", "%sil"),
    ("%rdi", "%edi", "%dil"),
    ("%r8", "%r8d", "%r8b"),
    ("%r9", "%r9d", "%r9b"),
    ("%r11", "%r11d", "%r11b"),
];

struct RegAlloc<'a> {
    out: &'a mut String,
    f: &'a IrFunc,
    live_out: &'a std::collections::HashSet<Value>,
    map: HashMap<Value, (usize, bool)>,
    free: Vec<usize>,
}

impl<'a> RegAlloc<'a> {
    fn spill(&mut self) -> usize {
        // Always write a defined value back: it may still be used later in the
        // block even when it is not live out.
        let victim = *self.map.keys().next().expect("non-empty");
        let (idx, is_def) = self.map.remove(&victim).unwrap();
        if is_def {
            self.out.push_str(&format!(
                "    mov {}, -{}(%rbp)\n",
                POOL[idx].0,
                voff(self.f, victim)
            ));
        }
        idx
    }

    fn spill_excluding(&mut self, protected: &[usize]) -> usize {
        let victim = self
            .map
            .keys()
            .copied()
            .find(|v| {
                let idx = self.map[v].0;
                !protected.contains(&idx)
            })
            .expect("a register outside the protected set");
        let (idx, is_def) = self.map.remove(&victim).unwrap();
        if is_def {
            self.out.push_str(&format!(
                "    mov {}, -{}(%rbp)\n",
                POOL[idx].0,
                voff(self.f, victim)
            ));
        }
        idx
    }

    fn alloc(&mut self) -> usize {
        if self.free.is_empty() {
            let idx = self.spill();
            self.free.push(idx);
        }
        self.free.pop().unwrap()
    }

    fn alloc_excluding(&mut self, protected: &[usize]) -> usize {
        if self.free.is_empty() {
            let idx = self.spill_excluding(protected);
            self.free.push(idx);
        }
        self.free.pop().unwrap()
    }

    fn operand(&mut self, v: Value) -> usize {
        if let Some((i, _)) = self.map.get(&v) {
            return *i;
        }
        let idx = self.alloc();
        self.out.push_str(&format!(
            "    mov -{}(%rbp), {}\n",
            voff(self.f, v),
            POOL[idx].0
        ));
        self.map.insert(v, (idx, false));
        idx
    }

    fn operand_excluding(&mut self, v: Value, protected: &[usize]) -> usize {
        if let Some((i, _)) = self.map.get(&v) {
            return *i;
        }
        let idx = self.alloc_excluding(protected);
        self.out.push_str(&format!(
            "    mov -{}(%rbp), {}\n",
            voff(self.f, v),
            POOL[idx].0
        ));
        self.map.insert(v, (idx, false));
        idx
    }

    fn dst(&mut self, v: Value) -> usize {
        let idx = self.alloc();
        self.map.insert(v, (idx, true));
        idx
    }

    fn dst_excluding(&mut self, v: Value, protected: &[usize]) -> usize {
        let idx = self.alloc_excluding(protected);
        self.map.insert(v, (idx, true));
        idx
    }

    fn def_store(&mut self, v: Value, idx: usize) {
        if self.live_out.contains(&v) {
            self.out.push_str(&format!(
                "    mov {}, -{}(%rbp)\n",
                POOL[idx].0,
                voff(self.f, v)
            ));
        }
    }
}

fn emit_fast_block(
    out: &mut String,
    f: &IrFunc,
    instrs: &[Instr],
    live_out: &std::collections::HashSet<Value>,
    name: &str,
    var_regs: &HashMap<usize, &'static str>,
) {
    let mut ra = RegAlloc {
        out,
        f,
        live_out,
        map: HashMap::new(),
        free: (0..POOL.len()).rev().collect(),
    };
    for ins in instrs {
        match ins {
            Instr::Const { dst, v } => {
                let d = ra.dst(*dst);
                if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                    ra.out.push_str(&format!("    mov ${}, {}\n", v, POOL[d].0));
                } else {
                    ra.out.push_str(&format!("    movabs ${}, {}\n", v, POOL[d].0));
                }
                ra.def_store(*dst, d);
            }
            Instr::LoadVar { dst, var } => {
                let d = ra.dst(*dst);
                if let Some(reg) = var_regs.get(var) {
                    ra.out.push_str(&format!("    mov {}, {}\n", reg, POOL[d].0));
                } else {
                    ra.out.push_str(&format!(
                        "    mov -{}(%rbp), {}\n",
                        varoff(*var),
                        POOL[d].0
                    ));
                }
                ra.def_store(*dst, d);
            }
            Instr::StoreVar { var, src } => {
                let a = ra.operand(*src);
                if let Some(reg) = var_regs.get(var) {
                    ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, reg));
                } else {
                    ra.out.push_str(&format!(
                        "    mov {}, -{}(%rbp)\n",
                        POOL[a].0,
                        varoff(*var)
                    ));
                }
            }
            Instr::Bin { dst, op, l, r } => {
                let a = ra.operand(*l);
                let b = ra.operand_excluding(*r, &[a]);
                let d = ra.dst_excluding(*dst, &[a, b]);
                ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                match op {
                    Op::Add => ra.out.push_str(&format!("    add {}, {}\n", POOL[b].0, POOL[d].0)),
                    Op::Sub => ra.out.push_str(&format!("    sub {}, {}\n", POOL[b].0, POOL[d].0)),
                    Op::Mul => ra
                        .out
                        .push_str(&format!("    imul {}, {}\n", POOL[b].0, POOL[d].0)),
                    Op::And => ra.out.push_str(&format!("    and {}, {}\n", POOL[b].0, POOL[d].0)),
                    Op::Or => ra.out.push_str(&format!("    or {}, {}\n", POOL[b].0, POOL[d].0)),
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                        ra.out.push_str(&format!("    cmp {}, {}\n", POOL[b].0, POOL[d].0));
                        let set = match op {
                            Op::Eq => "sete",
                            Op::Ne => "setne",
                            Op::Lt => "setl",
                            Op::Le => "setle",
                            Op::Gt => "setg",
                            Op::Ge => "setge",
                            _ => unreachable!(),
                        };
                        ra.out.push_str(&format!("    {} %al\n", set));
                        ra.out.push_str(&format!("    movzbq %al, {}\n", POOL[d].0));
                    }
                    Op::Div | Op::Rem => unreachable!("slow block split"),
                }
                ra.def_store(*dst, d);
            }
            Instr::Un { dst, neg, a } => {
                let s = ra.operand(*a);
                let d = ra.dst_excluding(*dst, &[s]);
                ra.out.push_str(&format!("    mov {}, {}\n", POOL[s].0, POOL[d].0));
                if *neg {
                    ra.out.push_str(&format!("    neg {}\n", POOL[d].0));
                } else {
                    ra.out.push_str(&format!("    xor $1, {}\n", POOL[d].0));
                }
                ra.def_store(*dst, d);
            }
            Instr::Trunc { dst, ty, a } => {
                let s = ra.operand(*a);
                let d = ra.dst_excluding(*dst, &[s]);
                ra.out.push_str(&format!("    mov {}, {}\n", POOL[s].0, POOL[d].0));
                match ty {
                    Ty::I32 => ra
                        .out
                        .push_str(&format!("    movslq {}, {}\n", POOL[d].1, POOL[d].0)),
                    Ty::U8 => ra
                        .out
                        .push_str(&format!("    movzbq {}, {}\n", POOL[d].2, POOL[d].0)),
                    Ty::Bool => {
                        ra.out.push_str(&format!("    test {}, {}\n", POOL[d].0, POOL[d].0));
                        ra.out.push_str("    setne %al\n");
                        ra.out.push_str(&format!("    movzbq %al, {}\n", POOL[d].0));
                    }
                    _ => {}
                }
                ra.def_store(*dst, d);
            }
            Instr::BoundsCheck { idx, len, line } => {
                let a = ra.operand(*idx);
                ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                match len {
                    LenRef::Const(n) => {
                        ra.out.push_str(&format!("    mov ${}, %rcx\n", n));
                    }
                    LenRef::Var(v) => {
                        if let Some(reg) = var_regs.get(v) {
                            ra.out.push_str(&format!("    mov {}, %rcx\n", reg));
                        } else {
                            ra.out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*v)));
                        }
                    }
                }
                ra.out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                ra.out.push_str("    call .Lir_panic\n1:\n");
            }
            Instr::IndexLoad { dst, ptr_var, idx, elem } => {
                let a = ra.operand(*idx);
                ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                if let Some(reg) = var_regs.get(ptr_var) {
                    ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                } else {
                    ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                }
                match elem {
                    Ty::U8 => ra.out.push_str("    movzbq (%r10,%rax,1), %rax\n"),
                    Ty::I32 => ra.out.push_str("    movslq (%r10,%rax,4), %rax\n"),
                    _ => ra.out.push_str("    mov (%r10,%rax,8), %rax\n"),
                }
                let d = ra.dst(*dst);
                ra.out.push_str(&format!("    mov %rax, {}\n", POOL[d].0));
                ra.def_store(*dst, d);
            }
            Instr::IndexStore { ptr_var, idx, elem, src } => {
                let s = ra.operand(*src);
                ra.out.push_str(&format!("    mov {}, %rcx\n", POOL[s].0));
                let a = ra.operand(*idx);
                ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                if let Some(reg) = var_regs.get(ptr_var) {
                    ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                } else {
                    ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                }
                match elem {
                    Ty::U8 => ra.out.push_str("    movb %cl, (%r10,%rax,1)\n"),
                    Ty::I32 => ra.out.push_str("    movl %ecx, (%r10,%rax,4)\n"),
                    _ => ra.out.push_str("    movq %rcx, (%r10,%rax,8)\n"),
                }
            }
            Instr::Br { cond, then_bb, else_bb } => {
                let c = ra.operand(*cond);
                ra.out.push_str(&format!("    cmp $0, {}\n", POOL[c].0));
                ra.out.push_str(&format!(
                    "    jne .L{}_{}\n    jmp .L{}_{}\n",
                    name, then_bb, name, else_bb
                ));
            }
            Instr::Jmp(bb) => ra
                .out
                .push_str(&format!("    jmp .L{}_{}\n", name, bb)),
            Instr::Ret(v) => {
                if let Some(v) = v {
                    let a = ra.operand(*v);
                    ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                } else {
                    ra.out.push_str("    xor %eax, %eax\n");
                }
                ra.out.push_str(&format!("    jmp .L{}_ret\n", name));
            }
            Instr::Call { .. } | Instr::Alloc { .. } | Instr::Print { .. } => {
                unreachable!("slow block")
            }
        }
    }
}

pub fn emit(prog: &IrProgram) -> String {
    let mut out = String::new();
    out.push_str(".text\n");
    out.push_str(".Lfmt_ll: .asciz \"%lld\\n\"\n");
    out.push_str(".Lfmt_s: .asciz \"%s\\n\"\n");
    out.push_str(".Ltrue: .asciz \"true\"\n");
    out.push_str(".Lfalse: .asciz \"false\"\n");
    out.push_str(".Ldiv0: .asciz \"line 0: division by zero\"\n");
    out.push_str(".Lfmt_panic: .asciz \"%s\\n\"\n");
    let mut lines: Vec<usize> = Vec::new();
    for f in &prog.funcs {
        for b in &f.blocks {
            for i in &b.instrs {
                if let Instr::BoundsCheck { line, .. } = i {
                    if !lines.contains(line) {
                        lines.push(*line);
                    }
                }
            }
        }
    }
    for line in &lines {
        out.push_str(&format!(
            ".Lmsg{}: .asciz \"line {}: index out of bounds\"\n",
            line, line
        ));
    }
    out.push_str(".text\n");
    out.push_str(".Lir_panic:\n");
    out.push_str("    push %rbp\n    mov %rsp, %rbp\n");
    out.push_str("    mov %rdi, %rdx\n");
    out.push_str("    mov stderr@GOTPCREL(%rip), %rax\n    mov (%rax), %rdi\n");
    out.push_str("    lea .Lfmt_panic(%rip), %rsi\n    xor %eax, %eax\n");
    out.push_str("    call fprintf@PLT\n    mov $1, %edi\n    call exit@PLT\n");
    for f in &prog.funcs {
        // Variables used most often get a callee-saved register for the whole
        // function; the emitter never touches these registers elsewhere.
        let mut var_counts: HashMap<usize, u32> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                match ins {
                    Instr::LoadVar { var, .. } => {
                        *var_counts.entry(*var).or_insert(0) += 1;
                    }
                    Instr::StoreVar { var, .. } => {
                        *var_counts.entry(*var).or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        }
        let mut ranked: Vec<(usize, u32)> = var_counts.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let chosen: Vec<(usize, &'static str)> = ranked
            .into_iter()
            .take(5)
            .enumerate()
            .map(|(i, (var, _))| (var, ["%rbx", "%r12", "%r13", "%r14", "%r15"][i]))
            .collect();
        let var_regs: HashMap<usize, &'static str> = chosen.iter().copied().collect();

        out.push_str(&format!(".globl {}\n.type {}, @function\n{}:\n", f.name, f.name, f.name));
        out.push_str("    push %rbp\n    mov %rsp, %rbp\n");
        let frame = 8 * (f.nvars + f.nvalues);
        let frame = (frame + 15) & !15;
        let pad = if chosen.len() % 2 == 1 { 8 } else { 0 };
        if frame + pad > 0 {
            out.push_str(&format!("    sub ${}, %rsp\n", frame + pad));
        }
        for i in 0..chosen.len() {
            out.push_str(&format!("    push {}\n", ["%rbx", "%r12", "%r13", "%r14", "%r15"][i]));
        }
        let regs = ["%rdi", "%rsi", "%rdx", "%rcx", "%r8", "%r9"];
        for i in 0..f.nparams.min(6) {
            if let Some(reg) = var_regs.get(&i) {
                out.push_str(&format!("    mov {}, {}\n", regs[i], reg));
            } else {
                out.push_str(&format!("    mov {}, -{}(%rbp)\n", regs[i], varoff(i)));
            }
        }
        let lblock = |bb: usize| format!(".L{}_{}", f.name, bb);
        let nblocks = f.blocks.len();
        let mut uses: Vec<std::collections::HashSet<Value>> = vec![Default::default(); nblocks];
        let mut defs: Vec<std::collections::HashSet<Value>> = vec![Default::default(); nblocks];
        for (bi, b) in f.blocks.iter().enumerate() {
            let mut defined: std::collections::HashSet<Value> = Default::default();
            for ins in &b.instrs {
                let mut u = Vec::new();
                let mut d = Vec::new();
                instr_uses_defs(ins, &mut u, &mut d);
                for v in u {
                    if !defined.contains(&v) {
                        uses[bi].insert(v);
                    }
                }
                for v in d {
                    defs[bi].insert(v);
                    defined.insert(v);
                }
            }
        }
        let mut live_in: Vec<std::collections::HashSet<Value>> = vec![Default::default(); nblocks];
        let mut live_out: Vec<std::collections::HashSet<Value>> = vec![Default::default(); nblocks];
        loop {
            let mut changed = false;
            for bi in (0..nblocks).rev() {
                let mut out_set: std::collections::HashSet<Value> = Default::default();
                for succ in block_successors(&f.blocks[bi].instrs) {
                    if succ < nblocks {
                        for v in &live_in[succ] {
                            out_set.insert(*v);
                        }
                    }
                }
                let mut in_set = uses[bi].clone();
                for v in &out_set {
                    if !defs[bi].contains(v) {
                        in_set.insert(*v);
                    }
                }
                if out_set != live_out[bi] || in_set != live_in[bi] {
                    live_out[bi] = out_set;
                    live_in[bi] = in_set;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (bb, b) in f.blocks.iter().enumerate() {
            out.push_str(&format!("{}:\n", lblock(bb)));
            if !slow_block(&b.instrs) {
                emit_fast_block(&mut out, f, &b.instrs, &live_out[bb], &f.name, &var_regs);
                continue;
            }
            for ins in &b.instrs {
                let ld = |v: Value| format!("    mov -{}(%rbp), %rax\n", voff(f, v));
                match ins {
                    Instr::Const { dst, v } => {
                        if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                            out.push_str(&format!("    mov ${}, %rax\n", v));
                        } else {
                            out.push_str(&format!("    movabs ${}, %rax\n", v));
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::LoadVar { dst, var } => {
                        if let Some(reg) = var_regs.get(var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*var)));
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::StoreVar { var, src } => {
                        out.push_str(&ld(*src));
                        if let Some(reg) = var_regs.get(var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*var)));
                        }
                    }
                    Instr::Bin { dst, op, l, r } => {
                        out.push_str(&ld(*l));
                        out.push_str(&format!("    mov -{}(%rbp), %rcx\n", voff(f, *r)));
                        match op {
                            Op::Add => out.push_str("    add %rcx, %rax\n"),
                            Op::Sub => out.push_str("    sub %rcx, %rax\n"),
                            Op::Mul => out.push_str("    imul %rcx, %rax\n"),
                            Op::Div | Op::Rem => {
                                out.push_str("    test %rcx, %rcx\n    jne 1f\n");
                                out.push_str("    lea .Ldiv0(%rip), %rdi\n");
                                out.push_str("    call abort@PLT\n1:\n");
                                out.push_str("    cqo\n    idiv %rcx\n");
                                if *op == Op::Rem {
                                    out.push_str("    mov %rdx, %rax\n");
                                }
                            }
                            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                                out.push_str("    cmp %rcx, %rax\n");
                                let set = match op {
                                    Op::Eq => "sete",
                                    Op::Ne => "setne",
                                    Op::Lt => "setl",
                                    Op::Le => "setle",
                                    Op::Gt => "setg",
                                    Op::Ge => "setge",
                                    _ => unreachable!(),
                                };
                                out.push_str(&format!("    {} %al\n    movzbq %al, %rax\n", set));
                            }
                            Op::And => out.push_str("    and %rcx, %rax\n"),
                            Op::Or => out.push_str("    or %rcx, %rax\n"),
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::Un { dst, neg, a } => {
                        out.push_str(&ld(*a));
                        if *neg {
                            out.push_str("    neg %rax\n");
                        } else {
                            out.push_str("    xor $1, %rax\n");
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::Trunc { dst, ty, a } => {
                        out.push_str(&ld(*a));
                        match ty {
                            Ty::I32 => out.push_str("    movslq %eax, %rax\n"),
                            Ty::U8 => out.push_str("    movzbq %al, %rax\n"),
                            Ty::Bool => {
                                out.push_str("    test %rax, %rax\n    setne %al\n    movzbq %al, %rax\n")
                            }
                            _ => {}
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::Call { dst, name, args } => {
                        for (i, a) in args.iter().enumerate() {
                            out.push_str(&format!("    mov -{}(%rbp), {}\n", voff(f, *a), regs[i]));
                        }
                        out.push_str(&format!("    call {}\n", name));
                        if let Some(d) = dst {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *d)));
                        }
                    }
                    Instr::Print { v, boolean } => {
                        out.push_str(&ld(*v));
                        if *boolean {
                            out.push_str("    lea .Lfalse(%rip), %rsi\n");
                            out.push_str("    test %rax, %rax\n    je 1f\n");
                            out.push_str("    lea .Ltrue(%rip), %rsi\n1:\n");
                            out.push_str("    lea .Lfmt_s(%rip), %rdi\n");
                        } else {
                            out.push_str("    mov %rax, %rsi\n    lea .Lfmt_ll(%rip), %rdi\n");
                        }
                        out.push_str("    xor %eax, %eax\n    call printf@PLT\n");
                    }
                    Instr::Ret(v) => {
                        if let Some(v) = v {
                            out.push_str(&ld(*v));
                        } else {
                            out.push_str("    xor %eax, %eax\n");
                        }
                        out.push_str(&format!("    jmp .L{}_ret\n", f.name));
                    }
                    Instr::Br { cond, then_bb, else_bb } => {
                        out.push_str(&ld(*cond));
                        out.push_str("    cmp $0, %rax\n");
                        out.push_str(&format!("    jne {}\n    jmp {}\n", lblock(*then_bb), lblock(*else_bb)));
                    }
                    Instr::Jmp(bb) => out.push_str(&format!("    jmp {}\n", lblock(*bb))),
                    Instr::Alloc { ptr_var, elem, len } => {
                        let esz = match elem {
                            Ty::U8 => 1,
                            Ty::I32 => 4,
                            _ => 8,
                        };
                        out.push_str(&ld(*len));
                        out.push_str("    mov %rax, %rdi\n");
                        out.push_str(&format!("    mov ${}, %esi\n", esz));
                        out.push_str("    call calloc@PLT\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*ptr_var)));
                        }
                    }
                    Instr::BoundsCheck { idx, len, line } => {
                        out.push_str(&ld(*idx));
                        match len {
                            LenRef::Const(n) => out.push_str(&format!("    mov ${}, %rcx\n", n)),
                            LenRef::Var(v) => {
                                if let Some(reg) = var_regs.get(v) {
                                    out.push_str(&format!("    mov {}, %rcx\n", reg));
                                } else {
                                    out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*v)));
                                }
                            }
                        }
                        out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                        out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                        out.push_str("    call .Lir_panic\n1:\n");
                    }
                    Instr::IndexLoad { dst, ptr_var, idx, elem } => {
                        out.push_str(&ld(*idx));
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        match elem {
                            Ty::U8 => {
                                out.push_str("    movzbq (%r10,%rax,1), %rax\n");
                            }
                            Ty::I32 => {
                                out.push_str("    movslq (%r10,%rax,4), %rax\n");
                            }
                            _ => {
                                out.push_str("    mov (%r10,%rax,8), %rax\n");
                            }
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::IndexStore { ptr_var, idx, elem, src } => {
                        out.push_str(&ld(*src));
                        out.push_str("    mov %rax, %rcx\n");
                        out.push_str(&ld(*idx));
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        match elem {
                            Ty::U8 => out.push_str("    movb %cl, (%r10,%rax,1)\n"),
                            Ty::I32 => out.push_str("    movl %ecx, (%r10,%rax,4)\n"),
                            _ => out.push_str("    movq %rcx, (%r10,%rax,8)\n"),
                        }
                    }
                }
            }
        }
        out.push_str(&format!(".L{}_ret:\n", f.name));
        for i in (0..chosen.len()).rev() {
            out.push_str(&format!("    pop {}\n", ["%rbx", "%r12", "%r13", "%r14", "%r15"][i]));
        }
        out.push_str("    leave\n    ret\n");
        out.push_str(&format!(".size {}, .-{}\n", f.name, f.name));
    }
    out.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    out
}
