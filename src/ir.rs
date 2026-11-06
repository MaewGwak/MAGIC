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

use crate::ast::{BinOp, Expr, ExprKind, Func, ParamTy, Program, Stmt, StmtKind, StructDef, Ty, UnOp};
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
    Print { v: Value, boolean: bool, float: bool, string: bool },
    Ret(Option<Value>),
    Br { cond: Value, then_bb: usize, else_bb: usize },
    Jmp(usize),
    Assert { cond: Value, line: usize },
    Alloc { ptr_var: usize, elem: Ty, esz: u64, len: Value },
    Fill { ptr_var: usize, elem: Ty, esz: u64, len: Value, value: Value },
    StructInit { var_index: usize, slots: usize, fields: Vec<(u64, Ty, Value)> },
    FieldLoad { dst: Value, var_index: usize, slots: usize, off: u64, ty: Ty },
    FieldStore { var_index: usize, slots: usize, off: u64, ty: Ty, src: Value },
    IndexFieldLoad { dst: Value, ptr_var: usize, idx: Value, size: u64, off: u64, ty: Ty, len: LenRef, line: usize },
    IndexFieldStore { ptr_var: usize, idx: Value, size: u64, off: u64, ty: Ty, src: Value, len: LenRef, line: usize },
    VecNew { ptr_var: usize, len_var: usize, cap_var: usize, esz: u64 },
    VecReserve { ptr_var: usize, len_var: usize, cap_var: usize, esz: u64, need: Value },
    VecLen { dst: Value, len_var: usize },
    VecPushScalar { ptr_var: usize, len_var: usize, cap_var: usize, esz: u64, ty: Ty, src: Value },
    VecPushStruct { ptr_var: usize, len_var: usize, cap_var: usize, esz: u64, size: u64, tmp_var: usize, tmp_slots: usize },
    Argv { dst: Value, argv_var: usize, idx: Value },
    InputRead { dst: Value, buf: Value, len: Value },
    PrintBytes { buf: Value, len: Value, stdout: bool },
    Str { dst: Value, id: usize, s: String },
    FBin { dst: Value, op: Op, l: Value, r: Value },
    FNeg { dst: Value, a: Value },
    FCmp { dst: Value, op: Op, l: Value, r: Value },
    I2F { dst: Value, a: Value },
    F2I { dst: Value, a: Value },
    Sqrt { dst: Value, a: Value },
    PrintPrec { v: Value, prec: Value },
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
    pub vty: Vec<Ty>,
    pub var_ty: Vec<Ty>,
    pub argc_var: Option<usize>,
    pub argv_var: Option<usize>,
    pub extra_frame: usize,
    pub nvars: usize,
    pub nvalues: usize,
    pub nparams: usize,
    pub blocks: Vec<Block>,
}

pub struct IrProgram {
    pub funcs: Vec<IrFunc>,
}

fn struct_layout(defs: &[StructDef], id: u16) -> (u64, Vec<u64>) {
    let Some(d) = defs.get(id as usize) else {
        return (0, Vec::new());
    };
    let mut off = 0u64;
    let mut align = 1u64;
    let mut offsets = Vec::new();
    for (_, ty) in &d.fields {
        let a = crate::ty_align(*ty, defs);
        let sz = crate::ty_size(*ty, defs);
        align = align.max(a);
        off = (off + a - 1) / a * a;
        offsets.push(off);
        off += sz;
    }
    let size = (off + align - 1) / align * align;
    (size, offsets)
}

fn struct_field(defs: &[StructDef], id: u16, name: &str) -> Option<(u64, Ty)> {
    let d = defs.get(id as usize)?;
    let (_, offsets) = struct_layout(defs, id);
    d.fields
        .iter()
        .enumerate()
        .find(|(_, (n, _))| n == name)
        .map(|(i, (_, ty))| (offsets[i], *ty))
}

fn struct_path(defs: &[StructDef], id: u16, path: &[String]) -> Option<(u64, Ty)> {
    let mut total = 0u64;
    let mut ty = Ty::Struct(id);
    for part in path {
        let Ty::Struct(cid) = ty else { return None };
        let (off, fty) = struct_field(defs, cid, part)?;
        total += off;
        ty = fty;
    }
    Some((total, ty))
}

fn is_zero_expr(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(0) => true,
        ExprKind::Bool(false) => true,
        ExprKind::Float(f) => *f == 0.0,
        ExprKind::StructLit { fields, .. } => fields.iter().all(|(_, v)| is_zero_expr(v)),
        _ => false,
    }
}

struct Builder {
    blocks: Vec<Block>,
    cur: usize,
    vars: HashMap<String, usize>,
    nvars: usize,
    nvalues: usize,
    vbool: Vec<bool>,
    vty: Vec<Ty>,
    var_bool: HashMap<String, bool>,
    var_ty: HashMap<String, Ty>,
    sigs: HashMap<String, Ty>,
    arrays: HashMap<String, (Ty, LenRef)>,
    next_str: usize,
    struct_defs: Vec<StructDef>,
    struct_vars: HashMap<String, (usize, usize, u16)>,
    struct_bytes: i64,
    vecs: HashMap<String, (Ty, usize, usize, usize, u64)>,
    argc_var: Option<usize>,
    argv_var: Option<usize>,
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
            vty: Vec::new(),
            var_bool: HashMap::new(),
            var_ty: HashMap::new(),
            sigs: HashMap::new(),
            arrays: HashMap::new(),
            next_str: 0,
            struct_defs: Vec::new(),
            struct_vars: HashMap::new(),
            struct_bytes: 0,
            vecs: HashMap::new(),
            argc_var: None,
            argv_var: None,
            ret,
        }
    }

    fn value(&mut self) -> Value {
        let v = Value(self.nvalues as u32);
        self.nvalues += 1;
        self.vbool.push(false);
        self.vty.push(Ty::I32);
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

    fn lower_struct_into(
        &mut self,
        id: u16,
        base: u64,
        fields: &[(String, Expr)],
        out: &mut Vec<(u64, Ty, Value)>,
    ) -> Result<(), String> {
        for (fname, fexpr) in fields {
            let (off, ty) = {
                let defs = &self.struct_defs;
                struct_field(defs, id, fname)
            }
            .ok_or_else(|| format!("struct has no field `{}`", fname))?;
            if let ExprKind::StructLit { name: sname, fields: inner } = &fexpr.kind {
                if let Ty::Struct(cid) = ty {
                    let nested_id = self
                        .struct_defs
                        .iter()
                        .position(|d| d.name == *sname)
                        .ok_or_else(|| format!("unknown struct `{}`", sname))?
                        as u16;
                    let _ = cid;
                    self.lower_struct_into(nested_id, base + off, inner, out)?;
                    continue;
                }
            }
            let val = self.lower_expr(fexpr)?;
            out.push((base + off, ty, val));
        }
        Ok(())
    }

    fn lower_expr(&mut self, e: &Expr) -> Result<Value, String> {
        match &e.kind {
            ExprKind::Float(f) => {
                let dst = self.value();
                self.push(Instr::Const {
                    dst,
                    v: f.to_bits() as i64,
                });
                self.vty[dst.0 as usize] = Ty::F64;
                Ok(dst)
            }
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
                self.vty[dst.0 as usize] = self.var_ty.get(n).copied().unwrap_or(Ty::I32);
                Ok(dst)
            }
            ExprKind::Bin { op, l, r } => {
                if matches!(op, BinOp::And | BinOp::Or) {
                    let is_and = *op == BinOp::And;
                    let tmp = self.nvars;
                    self.nvars += 1;
                    let rblock = self.new_block();
                    let sblock = self.new_block();
                    let endb = self.new_block();
                    let lv = self.lower_expr(l)?;
                    let (then_bb, else_bb) = if is_and {
                        (rblock, sblock)
                    } else {
                        (sblock, rblock)
                    };
                    self.push(Instr::Br { cond: lv, then_bb, else_bb });
                    self.cur = rblock;
                    let rv = self.lower_expr(r)?;
                    self.push(Instr::StoreVar { var: tmp, src: rv });
                    self.push(Instr::Jmp(endb));
                    self.cur = sblock;
                    let c = self.value();
                    self.push(Instr::Const { dst: c, v: if is_and { 0 } else { 1 } });
                    self.push(Instr::StoreVar { var: tmp, src: c });
                    self.push(Instr::Jmp(endb));
                    self.cur = endb;
                    let dst = self.value();
                    self.push(Instr::LoadVar { dst, var: tmp });
                    self.vbool[dst.0 as usize] = true;
                    return Ok(dst);
                }
                let a = self.lower_expr(l)?;
                let b = self.lower_expr(r)?;
                let ta = self.vty[a.0 as usize];
                let tb = self.vty[b.0 as usize];
                if ta == Ty::F64 || tb == Ty::F64 {
                    let a = if ta == Ty::F64 {
                        a
                    } else {
                        let d = self.value();
                        self.push(Instr::I2F { dst: d, a });
                        self.vty[d.0 as usize] = Ty::F64;
                        d
                    };
                    let b = if tb == Ty::F64 {
                        b
                    } else {
                        let d = self.value();
                        self.push(Instr::I2F { dst: d, a: b });
                        self.vty[d.0 as usize] = Ty::F64;
                        d
                    };
                    let fop = match op {
                        BinOp::Add => Op::Add,
                        BinOp::Sub => Op::Sub,
                        BinOp::Mul => Op::Mul,
                        BinOp::Div => Op::Div,
                        BinOp::Eq => Op::Eq,
                        BinOp::Ne => Op::Ne,
                        BinOp::Lt => Op::Lt,
                        BinOp::Le => Op::Le,
                        BinOp::Gt => Op::Gt,
                        BinOp::Ge => Op::Ge,
                        other => {
                            return Err(format!(
                                "line {}: IR: float operator {:?} not supported",
                                e.line, other
                            ))
                        }
                    };
                    let dst = self.value();
                    if matches!(fop, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) {
                        self.push(Instr::FCmp { dst, op: fop, l: a, r: b });
                        self.vbool[dst.0 as usize] = true;
                    } else {
                        self.push(Instr::FBin { dst, op: fop, l: a, r: b });
                        self.vty[dst.0 as usize] = Ty::F64;
                    }
                    return Ok(dst);
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
                if *op == UnOp::Neg && self.vty[a.0 as usize] == Ty::F64 {
                    self.push(Instr::FNeg { dst, a });
                    self.vty[dst.0 as usize] = Ty::F64;
                    return Ok(dst);
                }
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
                if let Some((elem, ptr_var, len_var, _cap, _esz)) = self.vecs.get(name).copied() {
                    let idx = self.lower_expr(index)?;
                    let len = LenRef::Var(len_var);
                    self.push(Instr::BoundsCheck { idx, len, line: e.line });
                    let dst = self.value();
                    self.push(Instr::IndexLoad { dst, ptr_var, idx, elem });
                    self.vty[dst.0 as usize] = elem;
                    return Ok(dst);
                }
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
                self.vty[dst.0 as usize] = elem;
                Ok(dst)
            }
            ExprKind::Field { base, field } => {
                let (var_index, slots, id) = *self.struct_vars.get(base).ok_or_else(|| {
                    format!("line {}: IR: unknown struct variable `{}`", e.line, base)
                })?;
                let (off, ty) = struct_field(&self.struct_defs, id, field).ok_or_else(|| {
                    format!("line {}: IR: struct has no field `{}`", e.line, field)
                })?;
                let dst = self.value();
                self.push(Instr::FieldLoad { dst, var_index, slots, off, ty });
                self.vty[dst.0 as usize] = ty;
                Ok(dst)
            }
            ExprKind::FieldPath { base, path } => {
                let (var_index, slots, id) = *self.struct_vars.get(base).ok_or_else(|| {
                    format!("line {}: IR: unknown struct variable `{}`", e.line, base)
                })?;
                let (off, ty) = struct_path(&self.struct_defs, id, path).ok_or_else(|| {
                    format!("line {}: IR: unknown field path", e.line)
                })?;
                let dst = self.value();
                self.push(Instr::FieldLoad { dst, var_index, slots, off, ty });
                self.vty[dst.0 as usize] = ty;
                Ok(dst)
            }
            ExprKind::IndexField { name, index, field } => {
                if let Some((elem, ptr_var, len_var, _cap, _esz)) = self.vecs.get(name).copied() {
                    if let Ty::Struct(id) = elem {
                        let (off, ty) = struct_field(&self.struct_defs, id, field).ok_or_else(|| {
                            format!("line {}: IR: struct has no field `{}`", e.line, field)
                        })?;
                        let (size, _) = struct_layout(&self.struct_defs, id);
                        let idx = self.lower_expr(index)?;
                        let dst = self.value();
                        self.push(Instr::IndexFieldLoad {
                            dst,
                            ptr_var,
                            idx,
                            size,
                            off,
                            ty,
                            len: LenRef::Var(len_var),
                            line: e.line,
                        });
                        self.vty[dst.0 as usize] = ty;
                        return Ok(dst);
                    }
                }
                let (elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let Ty::Struct(id) = elem else {
                    return Err(format!("line {}: IR: `{}` is not a struct array", e.line, name));
                };
                let (off, ty) = struct_field(&self.struct_defs, id, field).ok_or_else(|| {
                    format!("line {}: IR: struct has no field `{}`", e.line, field)
                })?;
                let (size, _) = struct_layout(&self.struct_defs, id);
                let idx = self.lower_expr(index)?;
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let dst = self.value();
                self.push(Instr::IndexFieldLoad {
                    dst,
                    ptr_var,
                    idx,
                    size,
                    off,
                    ty,
                    len,
                    line: e.line,
                });
                self.vty[dst.0 as usize] = ty;
                Ok(dst)
            }
            ExprKind::IndexFieldPath { name, index, path } => {
                let (elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let Ty::Struct(id) = elem else {
                    return Err(format!("line {}: IR: `{}` is not a struct array", e.line, name));
                };
                let (off, ty) = struct_path(&self.struct_defs, id, path).ok_or_else(|| {
                    format!("line {}: IR: unknown field path", e.line)
                })?;
                let (size, _) = struct_layout(&self.struct_defs, id);
                let idx = self.lower_expr(index)?;
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", e.line, name)
                })?;
                let dst = self.value();
                self.push(Instr::IndexFieldLoad {
                    dst,
                    ptr_var,
                    idx,
                    size,
                    off,
                    ty,
                    len,
                    line: e.line,
                });
                self.vty[dst.0 as usize] = ty;
                Ok(dst)
            }
            ExprKind::Cast { e, ty } => {
                let a = self.lower_expr(e)?;
                let from = self.vty[a.0 as usize];
                if *ty == Ty::F64 {
                    if from == Ty::F64 {
                        return Ok(a);
                    }
                    let dst = self.value();
                    self.push(Instr::I2F { dst, a });
                    self.vty[dst.0 as usize] = Ty::F64;
                    return Ok(dst);
                }
                if *ty == Ty::Str {
                    return Err(format!("line {}: IR: cast to string", e.line));
                }
                if from == Ty::F64 {
                    let iv = self.value();
                    self.push(Instr::F2I { dst: iv, a });
                    self.vty[iv.0 as usize] = Ty::I64;
                    let dst = self.value();
                    self.push(Instr::Trunc { dst, ty: *ty, a: iv });
                    self.vbool[dst.0 as usize] = *ty == Ty::Bool;
                    return Ok(dst);
                }
                let dst = self.value();
                self.push(Instr::Trunc { dst, ty: *ty, a });
                self.vbool[dst.0 as usize] = *ty == Ty::Bool;
                Ok(dst)
            }
            ExprKind::Str(text) => {
                let dst = self.value();
                let id = self.next_str;
                self.next_str += 1;
                self.push(Instr::Str {
                    dst,
                    id,
                    s: text.clone(),
                });
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
                let builtin = match name.as_str() {
                    "file_open" => Some(".Lmf_open"),
                    "file_close" => Some(".Lmf_close"),
                    "file_read" => Some(".Lmf_read"),
                    "file_write" => Some(".Lmf_write"),
                    "file_seek" => Some(".Lmf_seek"),
                    "file_read_at" => Some(".Lmf_read_at"),
                    "file_write_at" => Some(".Lmf_write_at"),
                    "file_tell" => Some(".Lmf_tell"),
                    "file_size" => Some(".Lmf_size"),
                    "file_write_str" => Some(".Lmf_write_str"),
                    "str_eq" => Some(".Lmf_str_eq"),
                    "str_byte" => Some(".Lmf_str_byte"),
                    "str_byte_n" => Some(".Lmf_str_byte_n"),
                    "str_len" => Some(".Lmf_str_len"),
                    "file_pread_at" | "file_pread" => {
                        if args.len() != 5 {
                            return Err(format!(
                                "line {}: IR: `{}` with {} arguments is not supported yet (only the 5-argument form)",
                                e.line,
                                name,
                                args.len()
                            ));
                        }
                        Some(".Lmf_pread_at")
                    }
                    _ => None,
                };
                if builtin.is_none()
                    && (name.starts_with("file_") || name == "env")
                {
                    return Err(format!(
                        "line {}: IR: builtin `{}` is not supported yet",
                        e.line, name
                    ));
                }
                if name == "argc" && args.is_empty() {
                    let av = self.argc_var.ok_or_else(|| {
                        format!("line {}: IR: argc is only available in main", e.line)
                    })?;
                    let dst = self.value();
                    self.push(Instr::LoadVar { dst, var: av });
                    self.vty[dst.0 as usize] = Ty::I32;
                    return Ok(dst);
                }
                if name == "arg" && args.len() == 1 {
                    let av = self.argv_var.ok_or_else(|| {
                        format!("line {}: IR: arg is only available in main", e.line)
                    })?;
                    let idx = self.lower_expr(&args[0])?;
                    let dst = self.value();
                    self.push(Instr::Argv { dst, argv_var: av, idx });
                    self.vty[dst.0 as usize] = Ty::Str;
                    return Ok(dst);
                }
                if (name == "input_read" || name == "print_bytes") && args.len() == 2 {
                    let buf = self.lower_expr(&args[0])?;
                    let len = self.lower_expr(&args[1])?;
                    let dst = self.value();
                    if name == "input_read" {
                        self.push(Instr::InputRead { dst, buf, len });
                    } else {
                        self.push(Instr::PrintBytes { buf, len, stdout: true });
                        self.push(Instr::Const { dst, v: 0 });
                    }
                    self.vty[dst.0 as usize] = Ty::I64;
                    return Ok(dst);
                }
                if name == "len" && args.len() == 1 {
                    if let ExprKind::Var(vname) = &args[0].kind {
                        if let Some((_, _, len_var, _, _)) = self.vecs.get(vname).copied() {
                            let dst = self.value();
                            self.push(Instr::VecLen { dst, len_var });
                            self.vty[dst.0 as usize] = Ty::I32;
                            return Ok(dst);
                        }
                    }
                    return Err(format!(
                        "line {}: IR: len() of this expression is not supported yet",
                        e.line
                    ));
                }
                if name == "sqrt" && args.len() == 1 {
                    let a = self.lower_expr(&args[0])?;
                    let a = if self.vty[a.0 as usize] == Ty::F64 {
                        a
                    } else {
                        let d = self.value();
                        self.push(Instr::I2F { dst: d, a });
                        self.vty[d.0 as usize] = Ty::F64;
                        d
                    };
                    let dst = self.value();
                    self.push(Instr::Sqrt { dst, a });
                    self.vty[dst.0 as usize] = Ty::F64;
                    return Ok(dst);
                }
                if name == "print_prec" {
                    return Err(format!(
                        "line {}: IR: print_prec is only supported as a statement",
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
                    name: builtin.unwrap_or(name.as_str()).to_string(),
                    args: vals,
                });
                self.vty[dst.0 as usize] = self.sigs.get(name).copied().unwrap_or(Ty::I32);
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
            StmtKind::VecLet { name, elem } => {
                let esz = crate::ty_size(*elem, &self.struct_defs);
                let pv = self.nvars;
                self.nvars += 3;
                self.vecs
                    .insert(name.clone(), (*elem, pv, pv + 1, pv + 2, esz));
                self.push(Instr::VecNew {
                    ptr_var: pv,
                    len_var: pv + 1,
                    cap_var: pv + 2,
                    esz,
                });
                return Ok(());
            }
            StmtKind::Push { name, value } => {
                let Some((elem, ptr_var, len_var, cap_var, esz)) =
                    self.vecs.get(name).copied()
                else {
                    return Err(format!("line {}: IR: unknown vector `{}`", s.line, name));
                };
                if let Ty::Struct(id) = elem {
                    let (size, _) = struct_layout(&self.struct_defs, id);
                    let slots = ((size + 7) / 8) as usize;
                    let tmp_var = self.nvars;
                    self.nvars += slots;
                    let mut inits = Vec::new();
                    if let ExprKind::StructLit { fields, .. } = &value.kind {
                        self.lower_struct_into(id, 0, fields, &mut inits)?;
                    } else {
                        return Err(format!(
                            "line {}: IR: vector push of a non-literal struct is not supported yet",
                            s.line
                        ));
                    }
                    self.push(Instr::StructInit {
                        var_index: tmp_var,
                        slots,
                        fields: inits,
                    });
                    self.push(Instr::VecPushStruct {
                        ptr_var,
                        len_var,
                        cap_var,
                        esz,
                        size,
                        tmp_var,
                        tmp_slots: slots,
                    });
                } else {
                    let src = self.lower_expr(value)?;
                    self.push(Instr::VecPushScalar {
                        ptr_var,
                        len_var,
                        cap_var,
                        esz,
                        ty: elem,
                        src,
                    });
                }
                return Ok(());
            }
            StmtKind::Arena { body } => {
                for st in body {
                    self.lower_stmt(st)?;
                }
                return Ok(());
            }
            StmtKind::Assert { cond } => {
                let c = self.lower_expr(cond)?;
                self.push(Instr::Assert { cond: c, line: s.line });
                return Ok(());
            }
            StmtKind::Let { name, init, .. } => {
                if let ExprKind::StructLit { name: sname, fields } = &init.kind {
                    let id = self
                        .struct_defs
                        .iter()
                        .position(|d| d.name == *sname)
                        .ok_or_else(|| format!("line {}: IR: unknown struct `{}`", s.line, sname))?
                        as u16;
                    let (size, _) = struct_layout(&self.struct_defs, id);
                    let slots = ((size + 7) / 8) as usize;
                    let var_index = self.nvars;
                    self.nvars += slots;
                    self.struct_vars.insert(name.clone(), (var_index, slots, id));
                    let mut inits = Vec::new();
                    self.lower_struct_into(id, 0, fields, &mut inits)?;
                    self.push(Instr::StructInit {
                        var_index,
                        slots,
                        fields: inits,
                    });
                    return Ok(());
                }
                let v = self.lower_expr(init)?;
                let is_bool = self.vbool[v.0 as usize];
                let var = self.var(name);
                self.push(Instr::StoreVar { var, src: v });
                self.var_bool.insert(name.clone(), is_bool);
                self.var_ty.insert(name.clone(), self.vty[v.0 as usize]);
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
                self.var_ty.insert(name.clone(), self.vty[v.0 as usize]);
            }
            StmtKind::ArrayLet { name, elem, value, count } => {
                let lenv = self.lower_expr(count)?;
                let ptr_var = self.var(name);
                let esz = crate::ty_size(*elem, &self.struct_defs);
                self.push(Instr::Alloc {
                    ptr_var,
                    elem: *elem,
                    esz,
                    len: lenv,
                });
                let len_var = self.nvars;
                self.nvars += 1;
                self.push(Instr::StoreVar { var: len_var, src: lenv });
                let len_ref = LenRef::Var(len_var);
                self.arrays.insert(name.clone(), (*elem, len_ref));
                let zero = is_zero_expr(value);
                if !zero {
                    if matches!(elem, Ty::Struct(_)) {
                        return Err(format!(
                            "line {}: IR: struct-array fill with a non-zero value is not supported yet",
                            s.line
                        ));
                    }
                    let init = self.lower_expr(value)?;
                    self.push(Instr::Fill {
                        ptr_var,
                        elem: *elem,
                        esz,
                        len: lenv,
                        value: init,
                    });
                }
                let _ = len_ref;
            }
            StmtKind::FieldAssign { base, field, value } => {
                let (var_index, slots, id) = *self.struct_vars.get(base).ok_or_else(|| {
                    format!("line {}: IR: unknown struct variable `{}`", s.line, base)
                })?;
                let (off, ty) = struct_field(&self.struct_defs, id, field).ok_or_else(|| {
                    format!("line {}: IR: struct has no field `{}`", s.line, field)
                })?;
                let src = self.lower_expr(value)?;
                self.push(Instr::FieldStore { var_index, slots, off, ty, src });
            }
            StmtKind::FieldPathAssign { base, path, value } => {
                let (var_index, slots, id) = *self.struct_vars.get(base).ok_or_else(|| {
                    format!("line {}: IR: unknown struct variable `{}`", s.line, base)
                })?;
                let (off, ty) = struct_path(&self.struct_defs, id, path).ok_or_else(|| {
                    format!("line {}: IR: unknown field path", s.line)
                })?;
                let src = self.lower_expr(value)?;
                self.push(Instr::FieldStore { var_index, slots, off, ty, src });
            }
            StmtKind::IndexFieldAssign { name, index, field, value } => {
                if let Some((elem, ptr_var, len_var, _cap, _esz)) = self.vecs.get(name).copied() {
                    if let Ty::Struct(id) = elem {
                        let (off, ty) = struct_field(&self.struct_defs, id, field).ok_or_else(|| {
                            format!("line {}: IR: struct has no field `{}`", s.line, field)
                        })?;
                        let (size, _) = struct_layout(&self.struct_defs, id);
                        let idx = self.lower_expr(index)?;
                        let src = self.lower_expr(value)?;
                        self.push(Instr::IndexFieldStore {
                            ptr_var,
                            idx,
                            size,
                            off,
                            ty,
                            src,
                            len: LenRef::Var(len_var),
                            line: s.line,
                        });
                        return Ok(());
                    }
                }
                let (elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                let Ty::Struct(id) = elem else {
                    return Err(format!("line {}: IR: `{}` is not a struct array", s.line, name));
                };
                let (off, ty) = struct_field(&self.struct_defs, id, field).ok_or_else(|| {
                    format!("line {}: IR: struct has no field `{}`", s.line, field)
                })?;
                let (size, _) = struct_layout(&self.struct_defs, id);
                let idx = self.lower_expr(index)?;
                let src = self.lower_expr(value)?;
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                self.push(Instr::IndexFieldStore {
                    ptr_var,
                    idx,
                    size,
                    off,
                    ty,
                    src,
                    len,
                    line: s.line,
                });
            }
            StmtKind::IndexFieldPathAssign { name, index, path, value } => {
                let (elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                let Ty::Struct(id) = elem else {
                    return Err(format!("line {}: IR: `{}` is not a struct array", s.line, name));
                };
                let (off, ty) = struct_path(&self.struct_defs, id, path).ok_or_else(|| {
                    format!("line {}: IR: unknown field path", s.line)
                })?;
                let (size, _) = struct_layout(&self.struct_defs, id);
                let idx = self.lower_expr(index)?;
                let src = self.lower_expr(value)?;
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                self.push(Instr::IndexFieldStore {
                    ptr_var,
                    idx,
                    size,
                    off,
                    ty,
                    src,
                    len,
                    line: s.line,
                });
            }
            StmtKind::IndexAssign { name, index, value } => {
                if let Some((elem, ptr_var, len_var, _cap, _esz)) = self.vecs.get(name).copied() {
                    let idx = self.lower_expr(index)?;
                    self.push(Instr::BoundsCheck { idx, len: LenRef::Var(len_var), line: s.line });
                    let src = self.lower_expr(value)?;
                    self.push(Instr::IndexStore { ptr_var, idx, elem, src });
                    return Ok(());
                }
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
            StmtKind::Print { value, ty } => {
                let v = self.lower_expr(value)?;
                let boolean = self.vbool[v.0 as usize];
                let vt = ty.or(Some(self.vty[v.0 as usize]));
                let float = vt == Some(Ty::F64);
                let string = vt == Some(Ty::Str);
                self.push(Instr::Print { v, boolean, float, string });
            }
            StmtKind::Return(e) => {
                let v = match e {
                    Some(e) => Some(self.lower_expr(e)?),
                    None => None,
                };
                self.push(Instr::Ret(v));
            }
            StmtKind::Expr(e) => {
                if let ExprKind::Call { name, args } = &e.kind {
                    if name == "reserve" && args.len() == 2 {
                        if let ExprKind::Var(vname) = &args[0].kind {
                            if let Some((_, ptr_var, len_var, cap_var, esz)) =
                                self.vecs.get(vname).copied()
                            {
                                let need = self.lower_expr(&args[1])?;
                                self.push(Instr::VecReserve {
                                    ptr_var,
                                    len_var,
                                    cap_var,
                                    esz,
                                    need,
                                });
                                return Ok(());
                            }
                        }
                    }
                    if name == "print_prec" && args.len() == 2 {
                        let v = self.lower_expr(&args[0])?;
                        let prec = self.lower_expr(&args[1])?;
                        self.push(Instr::PrintPrec { v, prec });
                        return Ok(());
                    }
                }
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
                let _ = par;
                let sv = self.lower_expr(start)?;
                let idx = self.var(var);
                self.push(Instr::StoreVar { var: idx, src: sv });
                let head = self.new_block();
                let body_bb = self.new_block();
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
            vty: self.vty.clone(),
            var_ty: {
                let mut out = vec![Ty::I32; self.nvars];
                for (name, var) in &self.vars {
                    if let Some(ty) = self.var_ty.get(name) {
                        out[*var] = *ty;
                    }
                }
                out
            },
            argc_var: None,
            argv_var: None,
            extra_frame: self.struct_bytes.max(0) as usize,
            nvars: self.nvars,
            nvalues: self.nvalues,
            nparams: 0,
            blocks: std::mem::take(&mut self.blocks),
        }
    }
}

pub fn lower(prog: &Program) -> Result<IrProgram, String> {
    let mut sigs: HashMap<String, Ty> = HashMap::new();
    for f in &prog.funcs {
        sigs.insert(f.name.clone(), f.ret);
    }
    let mut funcs = Vec::new();
    for f in &prog.funcs {
        funcs.push(lower_func(f, &sigs, &prog.structs)?);
    }
    Ok(IrProgram { funcs })
}

fn lower_func(f: &Func, sigs: &HashMap<String, Ty>, structs: &[StructDef]) -> Result<IrFunc, String> {
    if f.ret == Ty::Str {
        return Err(format!("IR: `{}` returns a string, not supported yet", f.name));
    }
    let mut b = Builder::new(f.ret);
    b.sigs = sigs.clone();
    b.struct_defs = structs.to_vec();
    let mut nparams = 0usize;
    for (name, pty) in &f.params {
        match pty {
            ParamTy::Scalar(ty) => {
                b.var(name);
                b.var_ty.insert(name.clone(), *ty);
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
    let (argc_var, argv_var) = if f.name == "main" {
        let a = b.nvars;
        b.nvars += 2;
        (Some(a), Some(a + 1))
    } else {
        (None, None)
    };
    b.argc_var = argc_var;
    b.argv_var = argv_var;
    for st in &f.body {
        b.lower_stmt(st)?;
    }
    if !matches!(b.blocks[b.cur].instrs.last(), Some(Instr::Ret(_))) {
        b.push(Instr::Ret(None));
    }
    let mut out = b.finish();
    out.name = f.name.clone();
    out.nparams = nparams;
    out.argc_var = argc_var;
    out.argv_var = argv_var;
    Ok(out)
}

fn compute_value_consts(f: &IrFunc, var_consts: &HashMap<usize, i64>) -> HashMap<Value, i64> {
    let mut vals: HashMap<Value, i64> = HashMap::new();
    for _ in 0..4 {
        let mut changed = false;
        for b in &f.blocks {
            for ins in &b.instrs {
                let known = match ins {
                    Instr::Const { dst, v } => Some((*dst, *v)),
                    Instr::LoadVar { dst, var } => var_consts.get(var).map(|v| (*dst, *v)),
                    Instr::Bin { dst, op, l, r } => match (vals.get(l), vals.get(r)) {
                        (Some(a), Some(b)) => {
                            let v = match op {
                                Op::Add => a.checked_add(*b),
                                Op::Sub => a.checked_sub(*b),
                                Op::Mul => a.checked_mul(*b),
                                Op::Div if *b != 0 => a.checked_div(*b),
                                Op::Rem if *b != 0 => a.checked_rem(*b),
                                Op::And => Some(*a & *b),
                                Op::Or => Some(*a | *b),
                                _ => None,
                            };
                            v.map(|v| (*dst, v))
                        }
                        _ => None,
                    },
                    Instr::Un { dst, neg, a } => {
                        vals.get(a).map(|a| (*dst, if *neg { -*a } else { *a ^ 1 }))
                    }
                    Instr::Trunc { dst, ty, a } => vals.get(a).map(|a| {
                        let v = match ty {
                            Ty::I32 => *a as i32 as i64,
                            Ty::U8 => *a as u8 as i64,
                            Ty::Bool => (*a != 0) as i64,
                            _ => *a,
                        };
                        (*dst, v)
                    }),
                    _ => None,
                };
                if let Some((dst, v)) = known {
                    if vals.get(&dst) != Some(&v) {
                        vals.insert(dst, v);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    vals
}

fn compute_var_consts(f: &IrFunc, vals: &HashMap<Value, i64>) -> HashMap<usize, i64> {
    let mut stores: HashMap<usize, Vec<Value>> = HashMap::new();
    for b in &f.blocks {
        for ins in &b.instrs {
            if let Instr::StoreVar { var, src } = ins {
                stores.entry(*var).or_default().push(*src);
            }
        }
    }
    let mut out = HashMap::new();
    for (var, srcs) in stores {
        if srcs.is_empty() {
            continue;
        }
        if let Some(first) = srcs.iter().map(|v| vals.get(v)).collect::<Option<Vec<_>>>() {
            let v = *first[0];
            if first.iter().all(|x| **x == v) {
                out.insert(var, v);
            }
        }
    }
    out
}

type Affine = (HashMap<usize, i64>, i64);

fn affine_of(
    v: Value,
    defs: &HashMap<Value, (DefKind, usize)>,
    instrs: &[&Instr],
    consts: &HashMap<Value, i64>,
    vars: &HashMap<usize, i64>,
) -> Option<Affine> {
    if let Some(c) = consts.get(&v) {
        return Some((HashMap::new(), *c));
    }
    let d = defs.get(&v)?.0;
    match d {
        DefKind::Const(c) => Some((HashMap::new(), c)),
        DefKind::LoadVar(var) => {
            if let Some(c) = vars.get(&var) {
                Some((HashMap::new(), *c))
            } else {
                let mut m = HashMap::new();
                m.insert(var, 1);
                Some((m, 0))
            }
        }
        DefKind::Other => {
            // Find the defining instruction; only Add/Sub/Mul(Const) are affine.
            let ins = instrs.iter().find(|i| def_kind(i).map(|(d, _)| d) == Some(v))?;
            match ins {
                Instr::Bin { op, l, r, .. } => match op {
                    Op::Add | Op::Sub => {
                        let (mut a, ca) = affine_of(*l, defs, instrs, consts, vars)?;
                        let (b, cb) = affine_of(*r, defs, instrs, consts, vars)?;
                        let sign = if *op == Op::Sub { -1 } else { 1 };
                        for (k, val) in b {
                            *a.entry(k).or_insert(0) += sign * val;
                        }
                        Some((a, ca + sign * cb))
                    }
                    Op::Mul => {
                        let (la, lc) = affine_of(*l, defs, instrs, consts, vars)?;
                        let (ra, rc) = affine_of(*r, defs, instrs, consts, vars)?;
                        if la.is_empty() && ra.is_empty() {
                            Some((HashMap::new(), lc * rc))
                        } else if la.is_empty() {
                            let k = lc;
                            Some((ra.into_iter().map(|(v, c)| (v, c * k)).collect(), rc * k))
                        } else if ra.is_empty() {
                            let k = rc;
                            Some((la.into_iter().map(|(v, c)| (v, c * k)).collect(), lc * k))
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                Instr::Un { neg: true, a, .. } => {
                    let (m, c) = affine_of(*a, defs, instrs, consts, vars)?;
                    Some((m.into_iter().map(|(v, c)| (v, -c)).collect(), -c))
                }
                _ => None,
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DefKind {
    Const(i64),
    LoadVar(usize),
    Other,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Bound {
    Const(i64),
    Var(usize),
}

fn def_kind(ins: &Instr) -> Option<(Value, DefKind)> {
    match ins {
        Instr::Const { dst, v } => Some((*dst, DefKind::Const(*v))),
        Instr::LoadVar { dst, var } => Some((*dst, DefKind::LoadVar(*var))),
        Instr::Bin { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::Un { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::Trunc { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::IndexLoad { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::Call { dst: Some(d), .. } => Some((*d, DefKind::Other)),
        Instr::Str { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::FBin { dst, .. } | Instr::FNeg { dst, .. } | Instr::FCmp { dst, .. } => {
            Some((*dst, DefKind::Other))
        }
        Instr::I2F { dst, .. } | Instr::F2I { dst, .. } | Instr::Sqrt { dst, .. } => {
            Some((*dst, DefKind::Other))
        }
        Instr::VecLen { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::Argv { dst, .. } | Instr::InputRead { dst, .. } => Some((*dst, DefKind::Other)),
        Instr::FieldLoad { dst, .. } | Instr::IndexFieldLoad { dst, .. } => {
            Some((*dst, DefKind::Other))
        }
        _ => None,
    }
}

/// Remove bounds checks that are implied by a `for i in 0..len` loop:
/// inside such a loop `i < len` holds, and if the emitted check compares the
/// same `i` against the same length (constant or variable), it is redundant.
/// Only loops whose index is modified solely by `i = i + 1` are trusted.
fn remove_redundant_bounds(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let n = f.blocks.len();
        let mut defs: HashMap<Value, (DefKind, usize)> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for ins in &b.instrs {
                if let Some((v, k)) = def_kind(ins) {
                    defs.insert(v, (k, bi));
                }
            }
        }
        let mut remove: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        let mut var_consts: HashMap<usize, i64> = HashMap::new();
        let mut vals: HashMap<Value, i64> = HashMap::new();
        for _ in 0..6 {
            vals = compute_value_consts(f, &var_consts);
            let next = compute_var_consts(f, &vals);
            if next == var_consts {
                break;
            }
            var_consts = next;
        }
        let all_instrs: Vec<&Instr> = f.blocks.iter().flat_map(|b| b.instrs.iter()).collect();
        let mut constraints: Vec<HashMap<usize, Bound>> = vec![HashMap::new(); n];
        let mut unchecked: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        for h in 0..n {
            let last = match f.blocks[h].instrs.last() {
                Some(Instr::Br { then_bb, else_bb, .. }) => (*then_bb, *else_bb),
                _ => continue,
            };
            let (body, exit) = last;
            // Natural-loop region: DFS from the body, not crossing the header.
            let mut region = std::collections::HashSet::new();
            let mut stack = vec![body];
            let mut reaches = false;
            while let Some(bb) = stack.pop() {
                if bb == h {
                    reaches = true;
                    continue;
                }
                if bb == exit || !region.insert(bb) {
                    continue;
                }
                for succ in block_successors(&f.blocks[bb].instrs) {
                    if succ < n {
                        stack.push(succ);
                    }
                }
            }
            if !reaches {
                continue;
            }
            // Head layout: LoadVar iv; ...; Bin Lt(l=iv', r=end) -> cond.
            let mut iv: Option<usize> = None;
            let mut bound: Option<Bound> = None;
            for ins in &f.blocks[h].instrs {
                if let Instr::Bin { op: Op::Lt, l, r, .. } = ins {
                    if let Some((DefKind::LoadVar(v), hb)) = defs.get(l).copied() {
                        if hb != h {
                            continue;
                        }
                        match defs.get(r).copied() {
                            Some((DefKind::Const(k), rb)) if rb == h => {
                                iv = Some(v);
                                bound = Some(Bound::Const(k));
                            }
                            Some((DefKind::LoadVar(w), rb)) if rb == h => {
                                iv = Some(v);
                                bound = Some(match var_consts.get(&w) {
                                    Some(c) => Bound::Const(*c),
                                    None => Bound::Var(w),
                                });
                            }
                            Some((DefKind::Other, _)) => {
                                // `for i in 0..len(vec)` lowers the bound as a
                                // VecLen (possibly wrapped in a cast).
                                let mut defv = *r;
                                let mut len_var = None;
                                for _ in 0..3 {
                                    let ins = all_instrs.iter().find(|ins| {
                                        def_kind(ins).map(|(d, _)| d) == Some(defv)
                                    });
                                    match ins {
                                        Some(Instr::VecLen { len_var: lv, .. }) => {
                                            len_var = Some(*lv);
                                            break;
                                        }
                                        Some(Instr::Trunc { a, .. }) => defv = *a,
                                        _ => break,
                                    }
                                }
                                if let Some(lv) = len_var {
                                    iv = Some(v);
                                    bound = Some(Bound::Var(lv));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            let (Some(iv), Some(bound)) = (iv, bound) else { continue };
            // The index may only be modified by `i = i + 1` inside the region.
            let mut ok = true;
            for bb in &region {
                for ins in &f.blocks[*bb].instrs {
                    if let Instr::StoreVar { var, src } = ins {
                        if *var == iv {
                            let good = match defs.get(src).copied() {
                                Some((DefKind::Other, _)) => {
                                    // find the defining Bin and check Add(i, const 1)
                                    let bin = f.blocks.iter().flat_map(|b| &b.instrs).find_map(|i| {
                                        if let Instr::Bin { dst, op: Op::Add, l, r } = i {
                                            if dst == src {
                                                return Some((*l, *r));
                                            }
                                        }
                                        None
                                    });
                                    match bin {
                                        Some((l, r)) => {
                                            let one = |v: Value| {
                                                matches!(defs.get(&v), Some((DefKind::Const(1), _)))
                                            };
                                            let is_iv = |v: Value| {
                                                matches!(defs.get(&v), Some((DefKind::LoadVar(x), _)) if *x == iv)
                                            };
                                            (is_iv(l) && one(r)) || (is_iv(r) && one(l))
                                        }
                                        None => false,
                                    }
                                }
                                _ => false,
                            };
                            if !good {
                                ok = false;
                            }
                        }
                        if let Bound::Var(w) = bound {
                            if *var == w {
                                ok = false;
                            }
                        }
                    }
                }
            }
            if !ok {
                continue;
            }
            for bb in &region {
                constraints[*bb].insert(iv, bound);
            }
            // Mark redundant checks.
            for bb in &region {
                for (ii, ins) in f.blocks[*bb].instrs.iter().enumerate() {
                    if let Instr::BoundsCheck { idx, len, .. } = ins {
                        let idx_is_iv = matches!(
                            defs.get(idx),
                            Some((DefKind::LoadVar(x), db)) if *x == iv && *db == *bb
                        );
                        let len_matches = match (bound, len) {
                            (Bound::Const(a), LenRef::Const(b)) => a == *b,
                            (Bound::Var(a), LenRef::Var(b)) => a == *b,
                            _ => false,
                        };
                        if idx_is_iv && len_matches {
                            remove.insert((*bb, ii));
                        }
                    }
                    match ins {
                        Instr::IndexFieldLoad { idx, len, .. }
                        | Instr::IndexFieldStore { idx, len, .. } => {
                            let idx_is_iv = matches!(
                                defs.get(idx),
                                Some((DefKind::LoadVar(x), db)) if *x == iv && *db == *bb
                            );
                            let len_matches = match (bound, len) {
                                (Bound::Const(a), LenRef::Const(b)) => a == *b,
                                (Bound::Var(a), LenRef::Var(b)) => a == *b,
                                _ => false,
                            };
                            if idx_is_iv && len_matches {
                                unchecked.insert((*bb, ii));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // Constant indexes that are known in range.
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, ins) in b.instrs.iter().enumerate() {
                if let Instr::BoundsCheck { idx, len, .. } = ins {
                    if let Some((DefKind::Const(k), _)) = defs.get(idx) {
                        let len_ok = match len {
                            LenRef::Const(n) => *k >= 0 && *k < *n,
                            LenRef::Var(_) => false,
                        };
                        if len_ok {
                            remove.insert((bi, ii));
                        }
                    }
                }
            }
        }
        // Affine indexes: prove sum(coef * i) + c < len using loop bounds.
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, ins) in b.instrs.iter().enumerate() {
                if remove.contains(&(bi, ii)) {
                    continue;
                }
                let Instr::BoundsCheck { idx, len, .. } = ins else { continue };
                let len_const = match len {
                    LenRef::Const(c) => Some(*c),
                    LenRef::Var(v) => var_consts.get(v).copied(),
                };
                let Some(len_const) = len_const else { continue };
                let Some((coefs, c0)) = affine_of(*idx, &defs, &all_instrs, &vals, &var_consts) else {
                    continue;
                };
                if c0 < 0 || coefs.values().any(|c| *c < 0) {
                    continue;
                }
                let mut max = c0;
                let mut proved = true;
                for (var, coef) in &coefs {
                    match constraints[bi].get(var) {
                        Some(Bound::Const(b)) => max += coef * (b - 1),
                        _ => {
                            proved = false;
                            break;
                        }
                    }
                }
                if proved && max < len_const {
                    remove.insert((bi, ii));
                }
            }
        }
        if remove.is_empty() && unchecked.is_empty() {
            continue;
        }
        for (bi, ii) in &unchecked {
            if let Some(ins) = f.blocks[*bi].instrs.get_mut(*ii) {
                match ins {
                    Instr::IndexFieldLoad { line, .. } | Instr::IndexFieldStore { line, .. } => {
                        *line = 0;
                    }
                    _ => {}
                }
            }
        }
        for (bi, b) in f.blocks.iter_mut().enumerate() {
            let mut new_instrs = Vec::new();
            for (ii, ins) in b.instrs.drain(..).enumerate() {
                if !remove.contains(&(bi, ii)) {
                    new_instrs.push(ins);
                }
            }
            b.instrs = new_instrs;
        }
    }
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
        Instr::Assert { cond, .. } => uses.push(*cond),
        Instr::Alloc { len, .. } => uses.push(*len),
        Instr::Str { dst, .. } => defs.push(*dst),
        Instr::FBin { dst, l, r, .. } | Instr::FCmp { dst, l, r, .. } => {
            uses.push(*l);
            uses.push(*r);
            defs.push(*dst);
        }
        Instr::FNeg { dst, a } | Instr::I2F { dst, a } | Instr::F2I { dst, a } | Instr::Sqrt { dst, a } => {
            uses.push(*a);
            defs.push(*dst);
        }
        Instr::PrintPrec { v, prec } => {
            uses.push(*v);
            uses.push(*prec);
        }
        Instr::Fill { len, value, .. } => {
            uses.push(*len);
            uses.push(*value);
        }
        Instr::StructInit { fields, .. } => {
            for (_, _, v) in fields {
                uses.push(*v);
            }
        }
        Instr::FieldLoad { dst, .. } => defs.push(*dst),
        Instr::FieldStore { src, .. } => uses.push(*src),
        Instr::IndexFieldLoad { dst, idx, .. } => {
            uses.push(*idx);
            defs.push(*dst);
        }
        Instr::Argv { dst, idx, .. } => {
            uses.push(*idx);
            defs.push(*dst);
        }
        Instr::InputRead { dst, buf, len } => {
            uses.push(*buf);
            uses.push(*len);
            defs.push(*dst);
        }
        Instr::PrintBytes { buf, len, .. } => {
            uses.push(*buf);
            uses.push(*len);
        }
        Instr::VecNew { .. } => {}
        Instr::VecReserve { need, .. } => uses.push(*need),
        Instr::VecLen { dst, .. } => defs.push(*dst),
        Instr::VecPushScalar { src, .. } => uses.push(*src),
        Instr::VecPushStruct { .. } => {}
        Instr::IndexFieldStore { idx, src, .. } => {
            uses.push(*idx);
            uses.push(*src);
        }
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
                | Instr::Fill { .. }
                | Instr::Print { .. }
                | Instr::Str { .. }
                | Instr::PrintPrec { .. }
                | Instr::Print { float: true, .. }
                | Instr::StructInit { .. }
                | Instr::VecNew { .. }
                | Instr::VecReserve { .. }
                | Instr::VecLen { .. }
                | Instr::VecPushScalar { .. }
                | Instr::VecPushStruct { .. }
                | Instr::Argv { .. }
                | Instr::InputRead { .. }
                | Instr::PrintBytes { .. }
                | Instr::Assert { .. }
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
    pool: &'a [usize],
    last_use: &'a HashMap<Value, usize>,
    consts: &'a HashMap<Value, i64>,
    deferred: &'a HashMap<Value, (Op, Value, Value, usize)>,
    xmap: HashMap<Value, (usize, bool)>,
    xfree: Vec<usize>,
}

const XMM: [&str; 14] = [
    "%xmm2", "%xmm3", "%xmm4", "%xmm5", "%xmm6", "%xmm7", "%xmm8", "%xmm9", "%xmm10",
    "%xmm11", "%xmm12", "%xmm13", "%xmm14", "%xmm15",
];

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

    fn dead_reg(&mut self, v: Value, at: usize) -> Option<usize> {
        if self.last_use.get(&v).copied() == Some(at) {
            self.map.remove(&v).map(|(i, _)| i)
        } else {
            None
        }
    }

    fn dst_at(&mut self, v: Value, idx: usize) -> usize {
        self.map.insert(v, (idx, true));
        idx
    }

    fn is_float(&self, v: Value) -> bool {
        self.f.vty.get(v.0 as usize) == Some(&Ty::F64)
    }

    fn xspill_excluding(&mut self, protected: &[usize]) -> usize {
        let victim = self
            .xmap
            .keys()
            .copied()
            .find(|v| !protected.contains(&self.xmap[v].0))
            .expect("a float register outside the protected set");
        let (idx, is_def) = self.xmap.remove(&victim).unwrap();
        if is_def {
            self.out.push_str(&format!(
                "    movsd {}, -{}(%rbp)\n",
                XMM[idx],
                voff(self.f, victim)
            ));
        }
        idx
    }

    fn xalloc_excluding(&mut self, protected: &[usize]) -> usize {
        if self.xfree.is_empty() {
            let idx = self.xspill_excluding(protected);
            self.xfree.push(idx);
        }
        self.xfree.pop().unwrap()
    }

    fn xoperand(&mut self, v: Value) -> usize {
        if let Some((i, _)) = self.xmap.get(&v) {
            return *i;
        }
        let idx = self.xalloc_excluding(&[]);
        self.out.push_str(&format!(
            "    movsd -{}(%rbp), {}\n",
            voff(self.f, v),
            XMM[idx]
        ));
        self.xmap.insert(v, (idx, false));
        idx
    }

    fn xoperand_excluding(&mut self, v: Value, protected: &[usize]) -> usize {
        if let Some((i, _)) = self.xmap.get(&v) {
            return *i;
        }
        let idx = self.xalloc_excluding(protected);
        self.out.push_str(&format!(
            "    movsd -{}(%rbp), {}\n",
            voff(self.f, v),
            XMM[idx]
        ));
        self.xmap.insert(v, (idx, false));
        idx
    }

    fn xdst(&mut self, v: Value) -> usize {
        let idx = self.xalloc_excluding(&[]);
        self.xmap.insert(v, (idx, true));
        idx
    }

    fn xdst_excluding(&mut self, v: Value, protected: &[usize]) -> usize {
        let idx = self.xalloc_excluding(protected);
        self.xmap.insert(v, (idx, true));
        idx
    }

    fn xdef_store(&mut self, v: Value, idx: usize) {
        if self.live_out.contains(&v) {
            self.out.push_str(&format!(
                "    movsd {}, -{}(%rbp)\n",
                XMM[idx],
                voff(self.f, v)
            ));
        }
    }

    fn x_dead(&mut self, v: Value, at: usize) -> Option<usize> {
        if self.last_use.get(&v).copied() == Some(at) {
            self.xmap.remove(&v).map(|(i, _)| i)
        } else {
            None
        }
    }

    /// Release registers whose value is never used again in this block. Live
    /// out values were already written at their definition, so they are safe
    /// to drop from the register file too.
    fn free_dead(&mut self, at: usize) {
        let dead: Vec<Value> = self
            .map
            .keys()
            .copied()
            .filter(|v| self.last_use.get(v).copied() == Some(at))
            .collect();
        for v in dead {
            if let Some((idx, _)) = self.map.remove(&v) {
                self.free.push(idx);
            }
            if let Some((idx, _)) = self.xmap.remove(&v) {
                self.xfree.push(idx);
            }
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
    pool: &[usize],
    last_use: &HashMap<Value, usize>,
    consts: &HashMap<Value, i64>,
    deferred: &HashMap<Value, (Op, Value, Value, usize)>,
) {
    let mut ra = RegAlloc {
        out,
        f,
        live_out,
        map: HashMap::new(),
        free: pool.iter().rev().copied().collect(),
        pool,
        last_use,
        consts,
        deferred,
        xmap: HashMap::new(),
        xfree: (0..XMM.len()).rev().collect(),
    };
    for (ii, ins) in instrs.iter().enumerate() {
        match ins {
            Instr::Const { dst, v } => {
                if ra.is_float(*dst) {
                    let d = ra.xdst(*dst);
                    ra.out.push_str(&format!("    movabs ${}, %rax\n    movq %rax, {}\n", v, XMM[d]));
                    ra.xdef_store(*dst, d);
                } else {
                    let d = ra.dst(*dst);
                    if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                        ra.out.push_str(&format!("    mov ${}, {}\n", v, POOL[d].0));
                    } else {
                        ra.out.push_str(&format!("    movabs ${}, {}\n", v, POOL[d].0));
                    }
                    ra.def_store(*dst, d);
                }
            }
            Instr::LoadVar { dst, var } => {
                if ra.is_float(*dst) {
                    let d = ra.xdst(*dst);
                    if let Some(reg) = var_regs.get(var) {
                        ra.out.push_str(&format!("    movq {}, {}\n", reg, XMM[d]));
                    } else {
                        ra.out.push_str(&format!("    movsd -{}(%rbp), {}\n", varoff(*var), XMM[d]));
                    }
                    ra.xdef_store(*dst, d);
                } else {
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
            }
            Instr::StoreVar { var, src } => {
                if ra.is_float(*src) {
                    let x = ra.xoperand(*src);
                    if let Some(reg) = var_regs.get(var) {
                        ra.out.push_str(&format!("    movq {}, {}\n", XMM[x], reg));
                    } else {
                        ra.out.push_str(&format!("    movsd {}, -{}(%rbp)\n", XMM[x], varoff(*var)));
                    }
                } else {
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
            }
            Instr::Bin { dst, op, l, r } => {
                if ra.deferred.contains_key(dst) {
                    // Emitted together with the branch that consumes it.
                    continue;
                }
                let commutative = matches!(op, Op::Add | Op::Mul | Op::And | Op::Or);
                let imm_r = ra.consts.get(r).copied().filter(|c| *c >= i32::MIN as i64 && *c <= i32::MAX as i64);
                let imm_l = ra.consts.get(l).copied().filter(|c| *c >= i32::MIN as i64 && *c <= i32::MAX as i64);
                if !matches!(op, Op::Div | Op::Rem) {
                    let is_cmp_op = matches!(
                        op,
                        Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                    );
                    let (imm, reg, swapped) = match (imm_r, imm_l) {
                        (Some(c), _) => (c, *l, false),
                        (None, Some(c)) if commutative || is_cmp_op => (c, *r, true),
                        _ => (0, *l, false),
                    };
                    let has_imm = imm_r.is_some() || imm_l.is_some();
                    if has_imm && imm_r.is_none() && matches!(op, Op::Sub) {
                        let imm = imm_l.unwrap();
                        let a = ra.operand(*r);
                        let d = match ra.dead_reg(*r, ii) {
                            Some(i) => ra.dst_at(*dst, i),
                            None => ra.dst_excluding(*dst, &[a]),
                        };
                        if d != a {
                            ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                        }
                        ra.out.push_str(&format!("    mov ${}, %rax\n", imm));
                        ra.out.push_str(&format!("    sub %rax, {}\n", POOL[d].0));
                        ra.def_store(*dst, d);
                        ra.free_dead(ii);
                        continue;
                    }
                    if has_imm {
                        let a = ra.operand(reg);
                        let d = match ra.dead_reg(reg, ii) {
                            Some(i) => ra.dst_at(*dst, i),
                            None => ra.dst_excluding(*dst, &[a]),
                        };
                        if !swapped {
                            match op {
                                Op::Add => {
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    ra.out.push_str(&format!("    add ${}, {}\n", imm, POOL[d].0));
                                }
                                Op::Sub => {
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    ra.out.push_str(&format!("    sub ${}, {}\n", imm, POOL[d].0));
                                }
                                Op::Mul => {
                                    if d == a {
                                        ra.out.push_str(&format!("    imul ${}, {}\n", imm, POOL[d].0));
                                    } else {
                                        ra.out.push_str(&format!("    imul ${}, {}, {}\n", imm, POOL[a].0, POOL[d].0));
                                    }
                                }
                                Op::And => {
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    ra.out.push_str(&format!("    and ${}, {}\n", imm, POOL[d].0));
                                }
                                Op::Or => {
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    ra.out.push_str(&format!("    or ${}, {}\n", imm, POOL[d].0));
                                }
                                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                                    ra.out.push_str(&format!("    cmp ${}, {}\n", imm, POOL[a].0));
                                    let set = match op {
                                        Op::Eq => "sete",
                                        Op::Ne => "setne",
                                        Op::Lt => "setl",
                                        Op::Le => "setle",
                                        Op::Gt => "setg",
                                        Op::Ge => "setge",
                                        _ => unreachable!(),
                                    };
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    ra.out.push_str(&format!("    {} %al\n    movzbq %al, {}\n", set, POOL[d].0));
                                }
                                _ => unreachable!(),
                            }
                        } else {
                            // constant on the left; comparisons need the flipped cond
                            match op {
                                Op::Add | Op::Mul | Op::And | Op::Or => {
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    let m = match op {
                                        Op::Add => "add",
                                        Op::Mul => "imul",
                                        Op::And => "and",
                                        _ => "or",
                                    };
                                    ra.out.push_str(&format!("    {} ${}, {}\n", m, imm, POOL[d].0));
                                }
                                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                                    ra.out.push_str(&format!("    cmp ${}, {}\n", imm, POOL[a].0));
                                    let set = match op {
                                        Op::Eq => "sete",
                                        Op::Ne => "setne",
                                        Op::Lt => "setg",
                                        Op::Le => "setge",
                                        Op::Gt => "setl",
                                        Op::Ge => "setle",
                                        _ => unreachable!(),
                                    };
                                    if d != a {
                                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                                    }
                                    ra.out.push_str(&format!("    {} %al\n    movzbq %al, {}\n", set, POOL[d].0));
                                }
                                _ => unreachable!(),
                            }
                        }
                        ra.def_store(*dst, d);
                        ra.free_dead(ii);
                        continue;
                    }
                }
                let a = ra.operand(*l);
                let b = ra.operand_excluding(*r, &[a]);
                let dead_a = ra.dead_reg(*l, ii);
                let dead_b = if commutative && dead_a.is_none() {
                    ra.dead_reg(*r, ii)
                } else {
                    None
                };
                let d = match (dead_a, dead_b) {
                    (Some(i), _) => ra.dst_at(*dst, i),
                    (None, Some(i)) => ra.dst_at(*dst, i),
                    (None, None) => ra.dst_excluding(*dst, &[a, b]),
                };
                let is_cmp = matches!(
                    op,
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                );
                if is_cmp {
                    // setcc needs the operands untouched and writes %al.
                    if d != a {
                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                    }
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
                } else {
                    // When the destination reuses a dead operand the copy is
                    // unnecessary; for a dead right operand only commutative
                    // operations may swap the roles.
                    if d == a || d == b {
                        // result already starts in the reused register
                    } else {
                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                    }
                    let rhs = if d == a {
                        POOL[b].0
                    } else if d == b && commutative {
                        POOL[a].0
                    } else {
                        POOL[b].0
                    };
                    match op {
                        Op::Add => ra.out.push_str(&format!("    add {}, {}\n", rhs, POOL[d].0)),
                        Op::Sub => ra.out.push_str(&format!("    sub {}, {}\n", rhs, POOL[d].0)),
                        Op::Mul => ra.out.push_str(&format!("    imul {}, {}\n", rhs, POOL[d].0)),
                        Op::And => ra.out.push_str(&format!("    and {}, {}\n", rhs, POOL[d].0)),
                        Op::Or => ra.out.push_str(&format!("    or {}, {}\n", rhs, POOL[d].0)),
                        _ => unreachable!(),
                    }
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
                if ra.is_float(*dst) {
                    let a = ra.operand(*idx);
                    ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                    if let Some(reg) = var_regs.get(ptr_var) {
                        ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                    } else {
                        ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                    }
                    let d = ra.xdst(*dst);
                    ra.out.push_str(&format!("    movsd (%r10,%rax,8), {}\n", XMM[d]));
                    ra.xdef_store(*dst, d);
                    ra.free_dead(ii);
                    continue;
                }
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
                    Ty::Bool => {
                        ra.out.push_str("    mov %rax, %rsi\n    shr $6, %rsi\n");
                        ra.out.push_str("    and $63, %rax\n    mov %rax, %rcx\n");
                        ra.out.push_str("    mov (%r10,%rsi,8), %rax\n");
                        ra.out.push_str("    shr %cl, %rax\n    and $1, %rax\n");
                    }
                    _ => ra.out.push_str("    mov (%r10,%rax,8), %rax\n"),
                }
                let d = ra.dst(*dst);
                ra.out.push_str(&format!("    mov %rax, {}\n", POOL[d].0));
                ra.def_store(*dst, d);
            }
            Instr::IndexStore { ptr_var, idx, elem, src } => {
                if ra.is_float(*src) {
                    let x = ra.xoperand(*src);
                    let a = ra.operand(*idx);
                    ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                    if let Some(reg) = var_regs.get(ptr_var) {
                        ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                    } else {
                        ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                    }
                    ra.out.push_str(&format!("    movsd {}, (%r10,%rax,8)\n", XMM[x]));
                    ra.free_dead(ii);
                    continue;
                }
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
                    Ty::Bool => {
                        ra.out.push_str("    mov %rcx, %rdi\n");
                        ra.out.push_str("    mov %rax, %rsi\n    shr $6, %rsi\n");
                        ra.out.push_str("    and $63, %rax\n    mov %rax, %rcx\n");
                        ra.out.push_str("    mov $1, %rax\n    shl %cl, %rax\n");
                        ra.out.push_str("    mov (%r10,%rsi,8), %rcx\n");
                        ra.out.push_str("    test %rdi, %rdi\n    je 2f\n");
                        ra.out.push_str("    or %rax, %rcx\n    jmp 3f\n");
                        ra.out.push_str("2:\n    not %rax\n    and %rax, %rcx\n3:\n");
                        ra.out.push_str("    mov %rcx, (%r10,%rsi,8)\n");
                    }
                    _ => ra.out.push_str("    movq %rcx, (%r10,%rax,8)\n"),
                }
            }
            Instr::FieldLoad { dst, var_index, slots, off, ty } => {
                let addr = -8 * (*var_index + *slots) as i64 + *off as i64;
                if *ty == Ty::F64 {
                    let d = ra.xdst(*dst);
                    ra.out.push_str(&format!("    movsd {}(%rbp), {}\n", addr, XMM[d]));
                    ra.xdef_store(*dst, d);
                    ra.free_dead(ii);
                    continue;
                }
                let d = ra.dst(*dst);
                match ty {
                    Ty::U8 | Ty::Bool => ra.out.push_str(&format!("    movzbq {}(%rbp), {}\n", addr, POOL[d].0)),
                    Ty::I32 => ra.out.push_str(&format!("    movslq {}(%rbp), {}\n", addr, POOL[d].0)),
                    _ => ra.out.push_str(&format!("    mov {}(%rbp), {}\n", addr, POOL[d].0)),
                }
                ra.def_store(*dst, d);
            }
            Instr::FieldStore { var_index, slots, off, ty, src } => {
                let addr = -8 * (*var_index + *slots) as i64 + *off as i64;
                if *ty == Ty::F64 {
                    let x = ra.xoperand(*src);
                    ra.out.push_str(&format!("    movsd {}, {}(%rbp)\n", XMM[x], addr));
                    ra.free_dead(ii);
                    continue;
                }
                let a = ra.operand(*src);
                match ty {
                    Ty::U8 | Ty::Bool => ra.out.push_str(&format!("    mov {}, {}(%rbp)\n", POOL[a].2, addr)),
                    Ty::I32 => ra.out.push_str(&format!("    mov {}, {}(%rbp)\n", POOL[a].1, addr)),
                    _ => ra.out.push_str(&format!("    mov {}, {}(%rbp)\n", POOL[a].0, addr)),
                }
            }
            Instr::IndexFieldLoad { dst, ptr_var, idx, size, off, ty, len, line } => {
                if ra.is_float(*dst) {
                    let i = ra.operand(*idx);
                    ra.out.push_str(&format!("    mov {}, %rax\n", POOL[i].0));
                    match len {
                        LenRef::Const(n) => ra.out.push_str(&format!("    mov ${}, %rcx\n", n)),
                        LenRef::Var(v) => {
                            if let Some(reg) = var_regs.get(v) {
                                ra.out.push_str(&format!("    mov {}, %rcx\n", reg));
                            } else {
                                ra.out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*v)));
                            }
                        }
                    }
                    if *line != 0 {
                        ra.out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                        ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                        ra.out.push_str("    call .Lir_panic\n1:\n");
                    }
                    if let Some(reg) = var_regs.get(ptr_var) {
                        ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                    } else {
                        ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                    }
                    let t = ra.alloc_excluding(&[i]);
                    ra.out.push_str(&format!("    mov {}, {}\n", POOL[i].0, POOL[t].0));
                    ra.out.push_str(&format!("    imul ${}, {}\n", size, POOL[t].0));
                    if *off > 0 {
                        ra.out.push_str(&format!("    add ${}, {}\n", off, POOL[t].0));
                    }
                    let d = ra.xdst_excluding(*dst, &[]);
                    ra.out.push_str(&format!("    movsd (%r10,{}), {}\n", POOL[t].0, XMM[d]));
                    ra.free.push(t);
                    ra.xdef_store(*dst, d);
                    ra.free_dead(ii);
                    continue;
                }
                let i = ra.operand(*idx);
                ra.out.push_str(&format!("    mov {}, %rax\n", POOL[i].0));
                match len {
                    LenRef::Const(n) => ra.out.push_str(&format!("    mov ${}, %rcx\n", n)),
                    LenRef::Var(v) => {
                        if let Some(reg) = var_regs.get(v) {
                            ra.out.push_str(&format!("    mov {}, %rcx\n", reg));
                        } else {
                            ra.out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*v)));
                        }
                    }
                }
                if *line != 0 {
                    ra.out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                    ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                    ra.out.push_str("    call .Lir_panic\n1:\n");
                }
                let ptr_slot = var_regs.get(ptr_var).copied();
                let d = ra.dst_excluding(*dst, &[i]);
                let base = match ptr_slot {
                    Some(reg) => reg.to_string(),
                    None => {
                        let p = ra.alloc_excluding(&[i, d]);
                        ra.out.push_str(&format!("    mov -{}(%rbp), {}\n", varoff(*ptr_var), POOL[p].0));
                        let b = POOL[p].0.to_string();
                        // keep p reserved by inserting a dummy live value keyed by dst? simpler: reload later
                        ra.free.push(p);
                        b
                    }
                };
                ra.out.push_str(&format!("    mov {}, {}\n", POOL[i].0, POOL[d].0));
                ra.out.push_str(&format!("    imul ${}, {}\n", size, POOL[d].0));
                if *off > 0 {
                    ra.out.push_str(&format!("    add ${}, {}\n", off, POOL[d].0));
                }
                match ty {
                    Ty::U8 | Ty::Bool => ra.out.push_str(&format!("    movzbq ({},{},1), {}\n", base, POOL[d].0, POOL[d].0)),
                    Ty::I32 => ra.out.push_str(&format!("    movslq ({},{},1), {}\n", base, POOL[d].0, POOL[d].0)),
                    _ => ra.out.push_str(&format!("    mov ({},{},1), {}\n", base, POOL[d].0, POOL[d].0)),
                }
                ra.def_store(*dst, d);
            }
            Instr::IndexFieldStore { ptr_var, idx, size, off, ty, src, len, line } => {
                if ra.is_float(*src) {
                    let x = ra.xoperand(*src);
                    let i = ra.operand(*idx);
                    ra.out.push_str(&format!("    mov {}, %rax\n", POOL[i].0));
                    match len {
                        LenRef::Const(n) => ra.out.push_str(&format!("    mov ${}, %r10\n", n)),
                        LenRef::Var(v) => {
                            if let Some(reg) = var_regs.get(v) {
                                ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                            } else {
                                ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*v)));
                            }
                        }
                    }
                    if *line != 0 {
                        ra.out.push_str("    cmp %r10, %rax\n    jb 1f\n");
                        ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                        ra.out.push_str("    call .Lir_panic\n1:\n");
                    }
                    if let Some(reg) = var_regs.get(ptr_var) {
                        ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                    } else {
                        ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                    }
                    let t = ra.alloc_excluding(&[x, i]);
                    ra.out.push_str(&format!("    mov {}, {}\n", POOL[i].0, POOL[t].0));
                    ra.out.push_str(&format!("    imul ${}, {}\n", size, POOL[t].0));
                    if *off > 0 {
                        ra.out.push_str(&format!("    add ${}, {}\n", off, POOL[t].0));
                    }
                    ra.out.push_str(&format!("    movsd {}, (%r10,{})\n", XMM[x], POOL[t].0));
                    ra.free.push(t);
                    ra.free_dead(ii);
                    continue;
                }
                let sreg = ra.operand(*src);
                ra.out.push_str(&format!("    mov {}, %rcx\n", POOL[sreg].0));
                let i = ra.operand_excluding(*idx, &[sreg]);
                ra.out.push_str(&format!("    mov {}, %rax\n", POOL[i].0));
                match len {
                    LenRef::Const(n) => ra.out.push_str(&format!("    mov ${}, %r10\n", n)),
                    LenRef::Var(v) => {
                        if let Some(reg) = var_regs.get(v) {
                            ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*v)));
                        }
                    }
                }
                if *line != 0 {
                    ra.out.push_str("    cmp %r10, %rax\n    jb 1f\n");
                    ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                    ra.out.push_str("    call .Lir_panic\n1:\n");
                }
                if let Some(reg) = var_regs.get(ptr_var) {
                    ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                } else {
                    ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                }
                ra.out.push_str(&format!("    imul ${}, %rax\n", size));
                if *off > 0 {
                    ra.out.push_str(&format!("    add ${}, %rax\n", off));
                }
                match ty {
                    Ty::U8 | Ty::Bool => ra.out.push_str("    movb %cl, (%r10,%rax)\n"),
                    Ty::I32 => ra.out.push_str("    movl %ecx, (%r10,%rax)\n"),
                    _ => ra.out.push_str("    movq %rcx, (%r10,%rax)\n"),
                }
            }
            Instr::FBin { dst, op, l, r } => {
                let a = ra.xoperand(*l);
                let b = ra.xoperand_excluding(*r, &[a]);
                let d = ra.xdst_excluding(*dst, &[a, b]);
                let m = match op {
                    Op::Add => "addsd",
                    Op::Sub => "subsd",
                    Op::Mul => "mulsd",
                    Op::Div => "divsd",
                    _ => "addsd",
                };
                if d == a {
                    ra.out.push_str(&format!("    {} {}, {}\n", m, XMM[b], XMM[d]));
                } else if d == b {
                    // commutative l op r == r op l; for sub/div move l first
                    if matches!(op, Op::Sub | Op::Div) {
                        ra.out.push_str(&format!("    movapd {}, {}\n", XMM[a], XMM[d]));
                        ra.out.push_str(&format!("    {} {}, {}\n", m, XMM[b], XMM[d]));
                    } else {
                        ra.out.push_str(&format!("    {} {}, {}\n", m, XMM[a], XMM[d]));
                    }
                } else {
                    ra.out.push_str(&format!("    movapd {}, {}\n", XMM[a], XMM[d]));
                    ra.out.push_str(&format!("    {} {}, {}\n", m, XMM[b], XMM[d]));
                }
                ra.xdef_store(*dst, d);
            }
            Instr::FNeg { dst, a } => {
                let s = ra.xoperand(*a);
                let d = ra.xdst_excluding(*dst, &[s]);
                ra.out.push_str(&format!("    movapd {}, {}\n", XMM[s], XMM[d]));
                ra.out.push_str("    movsd .Lf64sign(%rip), %xmm0\n");
                ra.out.push_str(&format!("    xorpd %xmm0, {}\n", XMM[d]));
                ra.xdef_store(*dst, d);
            }
            Instr::FCmp { dst, op, l, r } => {
                let a = ra.xoperand(*l);
                let b = ra.xoperand_excluding(*r, &[a]);
                let d = ra.dst_excluding(*dst, &[a, b]);
                let (pred, swap) = match op {
                    Op::Eq => (0, false),
                    Op::Ne => (4, false),
                    Op::Lt => (1, false),
                    Op::Le => (2, false),
                    Op::Gt => (1, true),
                    Op::Ge => (2, true),
                    _ => (0, false),
                };
                if swap {
                    ra.out.push_str(&format!("    cmpsd ${}, {}, {}\n    movq {}, %rax\n", pred, XMM[a], XMM[b], XMM[b]));
                } else {
                    ra.out.push_str(&format!("    cmpsd ${}, {}, {}\n    movq {}, %rax\n", pred, XMM[b], XMM[a], XMM[a]));
                }
                ra.out.push_str("    and $1, %rax\n");
                ra.out.push_str(&format!("    mov %rax, {}\n", POOL[d].0));
                ra.def_store(*dst, d);
            }
            Instr::I2F { dst, a } => {
                let s = ra.operand(*a);
                let d = ra.xdst(*dst);
                ra.out.push_str(&format!("    cvtsi2sd {}, {}\n", POOL[s].0, XMM[d]));
                ra.xdef_store(*dst, d);
            }
            Instr::F2I { dst, a } => {
                let s = ra.xoperand(*a);
                let d = ra.dst(*dst);
                ra.out.push_str(&format!("    cvttsd2si {}, {}\n", XMM[s], POOL[d].0));
                ra.def_store(*dst, d);
            }
            Instr::Sqrt { dst, a } => {
                let s = ra.xoperand(*a);
                let d = ra.xdst_excluding(*dst, &[s]);
                ra.out.push_str(&format!("    movapd {}, {}\n", XMM[s], XMM[d]));
                ra.out.push_str(&format!("    sqrtsd {}, {}\n", XMM[d], XMM[d]));
                ra.xdef_store(*dst, d);
            }
            Instr::Br { cond, then_bb, else_bb } => {
                if let Some((op, l, r, _)) = ra.deferred.get(cond).copied() {
                    let a = ra.operand(l);
                    let b = ra.operand_excluding(r, &[a]);
                    ra.out.push_str(&format!("    cmp {}, {}\n", POOL[b].0, POOL[a].0));
                    let set = match op {
                        Op::Eq => "je",
                        Op::Ne => "jne",
                        Op::Lt => "jl",
                        Op::Le => "jle",
                        Op::Gt => "jg",
                        Op::Ge => "jge",
                        _ => "jne",
                    };
                    ra.out.push_str(&format!(
                        "    {} .L{}_{}\n    jmp .L{}_{}\n",
                        set, name, then_bb, name, else_bb
                    ));
                } else {
                    let c = ra.operand(*cond);
                    ra.out.push_str(&format!("    cmp $0, {}\n", POOL[c].0));
                    ra.out.push_str(&format!(
                        "    jne .L{}_{}\n    jmp .L{}_{}\n",
                        name, then_bb, name, else_bb
                    ));
                }
            }
            Instr::Jmp(bb) => ra
                .out
                .push_str(&format!("    jmp .L{}_{}\n", name, bb)),
            Instr::Ret(v) => {
                if let Some(v) = v {
                    if ra.is_float(*v) {
                        let x = ra.xoperand(*v);
                        ra.out.push_str(&format!("    movq {}, %rax\n", XMM[x]));
                    } else {
                        let a = ra.operand(*v);
                        ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                    }
                } else {
                    ra.out.push_str("    xor %eax, %eax\n");
                }
                ra.out.push_str(&format!("    jmp .L{}_ret\n", name));
            }
            Instr::Call { .. }
            | Instr::Alloc { .. }
            | Instr::Fill { .. }
            | Instr::Str { .. }
            | Instr::Print { .. }
            | Instr::PrintPrec { .. }
            | Instr::StructInit { .. }
            | Instr::VecNew { .. }
            | Instr::VecReserve { .. }
            | Instr::VecLen { .. }
            | Instr::VecPushScalar { .. }
            | Instr::VecPushStruct { .. }
            | Instr::Argv { .. }
            | Instr::InputRead { .. }
            | Instr::PrintBytes { .. }
            | Instr::Assert { .. } => {
                unreachable!("slow block")
            }
        }
        ra.free_dead(ii);
    }
}

pub fn optimize(prog: &mut IrProgram) {
    remove_redundant_bounds(prog);
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
    out.push_str(".Lfmt_g: .asciz \"%g\\n\"\n");
    out.push_str(".Lfmt_pf: .asciz \"%.*f\\n\"\n");
    out.push_str(".p2align 3\n.Lf64sign: .quad 0x8000000000000000\n");
    let mut lines: Vec<usize> = Vec::new();
    for f in &prog.funcs {
        for b in &f.blocks {
            for i in &b.instrs {
                let line = match i {
                    Instr::BoundsCheck { line, .. } => Some(*line),
                    Instr::IndexFieldLoad { line, .. } => Some(*line),
                    Instr::IndexFieldStore { line, .. } => Some(*line),
                    _ => None,
                };
                if let Some(line) = line {
                    if line != 0 && !lines.contains(&line) {
                        lines.push(line);
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
    let mut assert_lines: Vec<usize> = Vec::new();
    for f in &prog.funcs {
        for b in &f.blocks {
            for i in &b.instrs {
                if let Instr::Assert { line, .. } = i {
                    if !assert_lines.contains(line) {
                        assert_lines.push(*line);
                    }
                }
            }
        }
    }
    for line in &assert_lines {
        out.push_str(&format!(
            ".Lamsg{}: .asciz \"line {}: assertion failed\"\n",
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
                    Instr::Alloc { ptr_var, .. }
                    | Instr::Fill { ptr_var, .. }
                    | Instr::IndexLoad { ptr_var, .. }
                    | Instr::IndexStore { ptr_var, .. } => {
                        *var_counts.entry(*ptr_var).or_insert(0) += 3;
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
        let has_bool = f.blocks.iter().any(|b| {
            b.instrs.iter().any(|i| {
                matches!(
                    i,
                    Instr::IndexLoad { elem: Ty::Bool, .. } | Instr::IndexStore { elem: Ty::Bool, .. }
                )
            })
        });
        let mut fn_var_consts: HashMap<usize, i64> = HashMap::new();
        let mut fn_vals: HashMap<Value, i64> = HashMap::new();
        for _ in 0..6 {
            fn_vals = compute_value_consts(f, &fn_var_consts);
            let next = compute_var_consts(f, &fn_vals);
            if next == fn_var_consts {
                break;
            }
            fn_var_consts = next;
        }
        let pool: Vec<usize> = if has_bool {
            vec![0, 3, 4, 5]
        } else {
            (0..POOL.len()).collect()
        };

        out.push_str(".p2align 4\n");
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
        if let (Some(ac), Some(av)) = (f.argc_var, f.argv_var) {
            out.push_str("    movslq %edi, %rax\n");
            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(ac)));
            out.push_str(&format!("    mov %rsi, -{}(%rbp)\n", varoff(av)));
        }
        for i in 0..f.nparams {
            let src = if i < 6 {
                regs[i].to_string()
            } else {
                out.push_str(&format!(
                    "    mov {}(%rbp), %rax\n",
                    16 + 8 * (i - 6)
                ));
                "%rax".to_string()
            };
            if let Some(reg) = var_regs.get(&i) {
                out.push_str(&format!("    mov {}, {}\n", src, reg));
            } else {
                out.push_str(&format!("    mov {}, -{}(%rbp)\n", src, varoff(i)));
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
            out.push_str(".p2align 4\n");
            out.push_str(&format!("{}:\n", lblock(bb)));
            if !slow_block(&b.instrs) {
                let mut last_use: HashMap<Value, usize> = HashMap::new();
                for (ii, ins) in b.instrs.iter().enumerate() {
                    let mut u = Vec::new();
                    let mut d = Vec::new();
                    instr_uses_defs(ins, &mut u, &mut d);
                    for v in u {
                        last_use.insert(v, ii);
                    }
                }
                // A comparison whose only use is the block terminator can be
                // emitted as cmp+jcc, skipping the setcc/movzbq/cmp sequence.
                let mut use_count: HashMap<Value, u32> = HashMap::new();
                for ins in &b.instrs {
                    let mut u = Vec::new();
                    let mut d = Vec::new();
                    instr_uses_defs(ins, &mut u, &mut d);
                    for v in u {
                        *use_count.entry(v).or_insert(0) += 1;
                    }
                }
                let mut deferred: HashMap<Value, (Op, Value, Value, usize)> = HashMap::new();
                let mut last_bin: HashMap<Value, (Op, Value, Value)> = HashMap::new();
                for ins in &b.instrs {
                    if let Instr::Bin { dst, op, l, r } = ins {
                        if matches!(
                            op,
                            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge
                        ) {
                            last_bin.insert(*dst, (*op, *l, *r));
                        }
                    }
                }
                if let Some(Instr::Br { cond, .. }) = b.instrs.last() {
                    if use_count.get(cond).copied().unwrap_or(0) == 1 {
                        if let Some((op, l, r)) = last_bin.get(cond).copied() {
                            let bi = b.instrs.len() - 1;
                            deferred.insert(*cond, (op, l, r, bi));
                            for v in [l, r] {
                                let e = last_use.entry(v).or_insert(0);
                                if *e < bi {
                                    *e = bi;
                                }
                            }
                        }
                    }
                }
                emit_fast_block(
                    &mut out,
                    f,
                    &b.instrs,
                    &live_out[bb],
                    &f.name,
                    &var_regs,
                    &pool,
                    &last_use,
                    &fn_vals,
                    &deferred,
                );
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
                        let nstack = args.len().saturating_sub(6);
                        let mut cleanup = 0usize;
                        if nstack > 0 {
                            let pad = if nstack % 2 == 1 { 8 } else { 0 };
                            if pad > 0 {
                                out.push_str("    sub $8, %rsp\n");
                                cleanup += 8;
                            }
                            for a in args.iter().skip(6).rev() {
                                out.push_str(&format!("    push -{}(%rbp)\n", voff(f, *a)));
                                cleanup += 8;
                            }
                        }
                        for (i, a) in args.iter().take(6).enumerate() {
                            out.push_str(&format!("    mov -{}(%rbp), {}\n", voff(f, *a), regs[i]));
                        }
                        out.push_str(&format!("    call {}\n", name));
                        if cleanup > 0 {
                            out.push_str(&format!("    add ${}, %rsp\n", cleanup));
                        }
                        if let Some(d) = dst {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *d)));
                        }
                    }
                    Instr::Str { dst, id, .. } => {
                        out.push_str(&format!("    lea .Lstr{}(%rip), %rax\n", id));
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::VecNew { ptr_var, len_var, cap_var, esz } => {
                        out.push_str(&format!("    mov ${}, %rdi\n", esz * 8));
                        out.push_str("    call malloc@PLT\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    movq $0, -{}(%rbp)\n", varoff(*len_var)));
                        out.push_str(&format!("    movq $8, -{}(%rbp)\n", varoff(*cap_var)));
                    }
                    Instr::VecReserve { ptr_var, len_var: _, cap_var, esz, need } => {
                        out.push_str(&ld(*need));
                        out.push_str("    mov %rax, %r11\n");
                        out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*cap_var)));
                        out.push_str("    cmp %r11, %rcx\n    jae 4f\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %rdi\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rdi\n", varoff(*ptr_var)));
                        }
                        out.push_str("    mov %r11, %rsi\n");
                        out.push_str(&format!("    imul ${}, %rsi\n", esz));
                        out.push_str("    push %r11\n    sub $8, %rsp\n");
                        out.push_str("    call realloc@PLT\n");
                        out.push_str("    add $8, %rsp\n    pop %r11\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    mov %r11, -{}(%rbp)\n", varoff(*cap_var)));
                        out.push_str("4:\n");
                    }
                    Instr::VecLen { dst, len_var } => {
                        out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*len_var)));
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::VecPushScalar { ptr_var, len_var, cap_var, esz, ty, src } => {
                        out.push_str(&ld(*src));
                        out.push_str("    mov %rax, %r8\n");
                        out.push_str(&format!("    mov -{}(%rbp), %r11\n", varoff(*len_var)));
                        out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*cap_var)));
                        out.push_str("    cmp %rcx, %r11\n    jb 5f\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %rdi\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rdi\n", varoff(*ptr_var)));
                        }
                        out.push_str("    lea (%rcx,%rcx), %rsi\n");
                        out.push_str(&format!("    imul ${}, %rsi\n", esz));
                        out.push_str("    push %r8\n    push %r11\n");
                        out.push_str("    call realloc@PLT\n");
                        out.push_str("    pop %r11\n    pop %r8\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*cap_var)));
                        out.push_str("    add %rcx, %rcx\n");
                        out.push_str(&format!("    mov %rcx, -{}(%rbp)\n", varoff(*cap_var)));
                        out.push_str("5:\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str("    mov %r8, %rax\n");
                        out.push_str(&format!("    imul ${}, %r11\n", esz));
                        match ty {
                            Ty::U8 | Ty::Bool => out.push_str("    movb %al, (%r10,%r11)\n"),
                            Ty::I32 => out.push_str("    movl %eax, (%r10,%r11)\n"),
                            _ => out.push_str("    movq %rax, (%r10,%r11)\n"),
                        }
                        out.push_str(&format!("    incq -{}(%rbp)\n", varoff(*len_var)));
                    }
                    Instr::VecPushStruct { ptr_var, len_var, cap_var, esz, size, tmp_var, tmp_slots } => {
                        out.push_str(&format!("    mov -{}(%rbp), %r11\n", varoff(*len_var)));
                        out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*cap_var)));
                        out.push_str("    cmp %rcx, %r11\n    jb 6f\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %rdi\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rdi\n", varoff(*ptr_var)));
                        }
                        out.push_str("    lea (%rcx,%rcx), %rsi\n");
                        out.push_str(&format!("    imul ${}, %rsi\n", esz));
                        out.push_str("    push %r11\n    sub $8, %rsp\n");
                        out.push_str("    call realloc@PLT\n");
                        out.push_str("    add $8, %rsp\n    pop %r11\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*cap_var)));
                        out.push_str("    add %rcx, %rcx\n");
                        out.push_str(&format!("    mov %rcx, -{}(%rbp)\n", varoff(*cap_var)));
                        out.push_str("6:\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    imul ${}, %r11, %r11\n", esz));
                        out.push_str("    add %r11, %r10\n");
                        let tmp_base = -8 * (*tmp_var + *tmp_slots) as i64;
                        let mut done = 0u64;
                        while done + 8 <= *size {
                            out.push_str(&format!(
                                "    movq {}(%rbp), %rax\n    mov %rax, {}(%r10)\n",
                                tmp_base + done as i64,
                                done
                            ));
                            done += 8;
                        }
                        while done < *size {
                            out.push_str(&format!(
                                "    movb {}(%rbp), %al\n    mov %al, {}(%r10)\n",
                                tmp_base + done as i64,
                                done
                            ));
                            done += 1;
                        }
                        out.push_str(&format!("    incq -{}(%rbp)\n", varoff(*len_var)));
                    }
                    Instr::Assert { cond, line } => {
                        out.push_str(&ld(*cond));
                        out.push_str("    test %rax, %rax\n    jne 7f\n");
                        out.push_str(&format!("    lea .Lamsg{}(%rip), %rdi\n", line));
                        out.push_str("    call .Lir_panic\n7:\n");
                    }
                    Instr::Argv { dst, argv_var, idx } => {
                        out.push_str(&ld(*idx));
                        out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*argv_var)));
                        out.push_str("    mov (%r10,%rax,8), %rax\n");
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::InputRead { dst, buf, len } => {
                        out.push_str(&ld(*buf));
                        out.push_str("    mov %rax, %rdi\n");
                        out.push_str(&ld(*len));
                        out.push_str("    mov %rax, %rdx\n    mov $1, %esi\n");
                        out.push_str("    mov stdin@GOTPCREL(%rip), %rax\n    mov (%rax), %rcx\n");
                        out.push_str("    call fread@PLT\n");
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::PrintBytes { buf, len, stdout } => {
                        out.push_str(&ld(*buf));
                        out.push_str("    mov %rax, %rdi\n");
                        out.push_str(&ld(*len));
                        out.push_str("    mov %rax, %rdx\n    mov $1, %esi\n");
                        if *stdout {
                            out.push_str("    mov stdout@GOTPCREL(%rip), %rax\n    mov (%rax), %rcx\n");
                        } else {
                            out.push_str("    mov stdin@GOTPCREL(%rip), %rax\n    mov (%rax), %rcx\n");
                        }
                        out.push_str("    call fwrite@PLT\n");
                    }
                    Instr::StructInit { var_index, slots, fields } => {
                        let base = -8 * (*var_index + *slots) as i64;
                        for (off, ty, v) in fields {
                            out.push_str(&ld(*v));
                            let addr = base + *off as i64;
                            match ty {
                                Ty::U8 | Ty::Bool => {
                                    out.push_str(&format!("    movb %al, {}(%rbp)\n", addr))
                                }
                                Ty::I32 => out.push_str(&format!("    movl %eax, {}(%rbp)\n", addr)),
                                _ => out.push_str(&format!("    movq %rax, {}(%rbp)\n", addr)),
                            }
                        }
                    }
                    Instr::FieldLoad { dst, var_index, slots, off, ty } => {
                        let base = -8 * (*var_index + *slots) as i64;
                        let addr = base + *off as i64;
                        match ty {
                            Ty::U8 | Ty::Bool => {
                                out.push_str(&format!("    movzbq {}(%rbp), %rax\n", addr))
                            }
                            Ty::I32 => out.push_str(&format!("    movslq {}(%rbp), %rax\n", addr)),
                            _ => out.push_str(&format!("    movq {}(%rbp), %rax\n", addr)),
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::FieldStore { var_index, slots, off, ty, src } => {
                        let base = -8 * (*var_index + *slots) as i64;
                        let addr = base + *off as i64;
                        out.push_str(&ld(*src));
                        match ty {
                            Ty::U8 | Ty::Bool => {
                                out.push_str(&format!("    movb %al, {}(%rbp)\n", addr))
                            }
                            Ty::I32 => out.push_str(&format!("    movl %eax, {}(%rbp)\n", addr)),
                            _ => out.push_str(&format!("    movq %rax, {}(%rbp)\n", addr)),
                        }
                    }
                    Instr::IndexFieldLoad { dst, ptr_var, idx, size, off, ty, len, line } => {
                        out.push_str(&ld(*idx));
                        out.push_str("    mov %rax, %r11\n");
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
                        out.push_str("    cmp %rcx, %r11\n    jb 1f\n");
                        out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                        out.push_str("    call .Lir_panic\n1:\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    imul ${}, %r11, %r11\n", size));
                        out.push_str(&format!("    add ${}, %r11\n", off));
                        match ty {
                            Ty::U8 | Ty::Bool => out.push_str("    movzbq (%r10,%r11), %rax\n"),
                            Ty::I32 => out.push_str("    movslq (%r10,%r11), %rax\n"),
                            _ => out.push_str("    mov (%r10,%r11), %rax\n"),
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::IndexFieldStore { ptr_var, idx, size, off, ty, src, len, line } => {
                        out.push_str(&ld(*src));
                        out.push_str("    mov %rax, %r8\n");
                        out.push_str(&ld(*idx));
                        out.push_str("    mov %rax, %r11\n");
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
                        out.push_str("    cmp %rcx, %r11\n    jb 1f\n");
                        out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                        out.push_str("    call .Lir_panic\n1:\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str(&format!("    imul ${}, %r11, %r11\n", size));
                        out.push_str(&format!("    add ${}, %r11\n", off));
                        match ty {
                            Ty::U8 | Ty::Bool => out.push_str("    movb %r8b, (%r10,%r11)\n"),
                            Ty::I32 => out.push_str("    movl %r8d, (%r10,%r11)\n"),
                            _ => out.push_str("    movq %r8, (%r10,%r11)\n"),
                        }
                    }
                    Instr::FBin { dst, op, l, r } => {
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *l)));
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm1\n", voff(f, *r)));
                        let m = match op {
                            Op::Add => "addsd",
                            Op::Sub => "subsd",
                            Op::Mul => "mulsd",
                            Op::Div => "divsd",
                            _ => "addsd",
                        };
                        out.push_str(&format!("    {} %xmm1, %xmm0\n", m));
                        out.push_str(&format!("    movsd %xmm0, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::FNeg { dst, a } => {
                        out.push_str(&ld(*a));
                        out.push_str("    btc $63, %rax\n");
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::FCmp { dst, op, l, r } => {
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *l)));
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm1\n", voff(f, *r)));
                        let (pred, swap) = match op {
                            Op::Eq => (0, false),
                            Op::Ne => (4, false),
                            Op::Lt => (1, false),
                            Op::Le => (2, false),
                            Op::Gt => (1, true),
                            Op::Ge => (2, true),
                            _ => (0, false),
                        };
                        if swap {
                            out.push_str(&format!("    cmpsd ${}, %xmm0, %xmm1\n", pred));
                            out.push_str("    movq %xmm1, %rax\n");
                        } else {
                            out.push_str(&format!("    cmpsd ${}, %xmm1, %xmm0\n", pred));
                            out.push_str("    movq %xmm0, %rax\n");
                        }
                        out.push_str("    and $1, %rax\n");
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::I2F { dst, a } => {
                        out.push_str(&ld(*a));
                        out.push_str("    cvtsi2sd %rax, %xmm0\n");
                        out.push_str(&format!("    movsd %xmm0, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::F2I { dst, a } => {
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *a)));
                        out.push_str("    cvttsd2si %xmm0, %rax\n");
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::Sqrt { dst, a } => {
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *a)));
                        out.push_str("    sqrtsd %xmm0, %xmm0\n");
                        out.push_str(&format!("    movsd %xmm0, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::PrintPrec { v, prec } => {
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *v)));
                        out.push_str(&ld(*prec));
                        out.push_str("    mov %rax, %rsi\n");
                        out.push_str("    lea .Lfmt_pf(%rip), %rdi\n    mov $1, %eax\n");
                        out.push_str("    call printf@PLT\n");
                    }
                    Instr::Print { v, string: true, .. } => {
                        out.push_str(&ld(*v));
                        out.push_str("    mov %rax, %rsi\n    lea .Lfmt_s(%rip), %rdi\n");
                        out.push_str("    xor %eax, %eax\n    call printf@PLT\n");
                    }
                    Instr::Print { v, boolean: _, float: true, .. } => {
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *v)));
                        out.push_str("    lea .Lfmt_g(%rip), %rdi\n    mov $1, %eax\n");
                        out.push_str("    call printf@PLT\n");
                    }
                    Instr::Print { v, boolean, float: false, string: false } => {
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
                    Instr::Alloc { ptr_var, elem, esz, len } => {
                        out.push_str(&ld(*len));
                        if *elem == Ty::Bool {
                            out.push_str("    add $63, %rax\n    shr $6, %rax\n");
                        }
                        out.push_str("    mov %rax, %rdi\n");
                        let esz = if *elem == Ty::Bool { 8 } else { *esz };
                        out.push_str(&format!("    mov ${}, %esi\n", esz));
                        out.push_str("    call calloc@PLT\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov %rax, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(*ptr_var)));
                        }
                    }
                    Instr::Fill { ptr_var, elem, esz, len, value } => {
                        out.push_str(&ld(*value));
                        out.push_str("    mov %rax, %r8\n");
                        out.push_str(&ld(*len));
                        out.push_str("    mov %rax, %r11\n");
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        if *elem == Ty::Bool {
                            // all bits set: len is in elements, memset whole words
                            out.push_str("    add $63, %r11\n    shr $6, %r11\n");
                            out.push_str("    lea 0(,%r11,8), %rdx\n");
                            out.push_str("    mov %r10, %rdi\n    mov $255, %esi\n");
                            out.push_str("    call memset@PLT\n");
                        } else {
                            let esz = *esz as i64;
                            out.push_str(&format!(
                                "    lea (%r10,%r11,{}), %r9\n",
                                esz
                            ));
                            out.push_str("1:\n");
                            out.push_str(&format!("    lea {}(%r10), %rcx\n", esz * 4));
                            out.push_str("    cmp %r9, %rcx\n    ja 3f\n");
                            for k in 0..4 {
                                match esz {
                                    1 => out.push_str(&format!("    mov %r8b, {}(%r10)\n", k)),
                                    4 => out.push_str(&format!("    mov %r8d, {}(%r10)\n", k * 4)),
                                    _ => out.push_str(&format!("    mov %r8, {}(%r10)\n", k * 8)),
                                }
                            }
                            out.push_str("    mov %rcx, %r10\n    jmp 1b\n");
                            out.push_str("3:\n");
                            out.push_str("    cmp %r9, %r10\n    jae 2f\n");
                            match esz {
                                1 => out.push_str("    mov %r8b, (%r10)\n"),
                                4 => out.push_str("    mov %r8d, (%r10)\n"),
                                _ => out.push_str("    mov %r8, (%r10)\n"),
                            }
                            out.push_str(&format!("    add ${}, %r10\n", esz));
                            out.push_str("    jmp 3b\n");
                            out.push_str("2:\n");
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
                            Ty::Bool => {
                                out.push_str("    mov %rax, %r11\n    shr $6, %r11\n");
                                out.push_str("    and $63, %rax\n    mov %rax, %rcx\n");
                                out.push_str("    mov (%r10,%r11,8), %rax\n");
                                out.push_str("    shr %cl, %rax\n    and $1, %rax\n");
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
                            Ty::Bool => {
                                out.push_str("    mov %rcx, %rdx\n");
                                out.push_str("    mov %rax, %r11\n    shr $6, %r11\n");
                                out.push_str("    and $63, %rax\n    mov %rax, %rcx\n");
                                out.push_str("    mov $1, %rsi\n    shl %cl, %rsi\n");
                                out.push_str("    mov (%r10,%r11,8), %rax\n");
                                out.push_str("    test %rdx, %rdx\n    je 2f\n");
                                out.push_str("    or %rsi, %rax\n    jmp 3f\n");
                                out.push_str("2:\n    not %rsi\n    and %rsi, %rax\n3:\n");
                                out.push_str("    mov %rax, (%r10,%r11,8)\n");
                            }
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
    // File builtins: thin wrappers with the same argument order as the AST
    // native backend. They call libc directly.
    out.push_str(".p2align 4\n.Lmf_open:\n    jmp fopen@PLT\n");
    out.push_str(".p2align 4\n.Lmf_close:\n    jmp fclose@PLT\n");
    out.push_str(".p2align 4\n.Lmf_read:\n    mov %rdi, %rcx\n    mov %rsi, %rdi\n");
    out.push_str("    mov $1, %esi\n    sub $8, %rsp\n    call fread@PLT\n");
    out.push_str("    add $8, %rsp\n    ret\n");
    out.push_str(".p2align 4\n.Lmf_write:\n    mov %rdi, %rcx\n    mov %rsi, %rdi\n");
    out.push_str("    mov $1, %esi\n    sub $8, %rsp\n    call fwrite@PLT\n");
    out.push_str("    add $8, %rsp\n    ret\n");
    out.push_str(".p2align 4\n.Lmf_seek:\n    jmp fseek@PLT\n");
    out.push_str(".p2align 4\n.Lmf_tell:\n    jmp ftell@PLT\n");
    out.push_str(".p2align 4\n.Lmf_write_str:\n    xchg %rdi, %rsi\n    jmp fputs@PLT\n");
    // read_at / write_at: fseek(fd, off, SEEK_SET) then fread/fwrite(buf, 1, len, fd)
    // read_at/write_at(fd, buf, off_in_buf, len): the native backend reads into
    // buf + off at the current file position, so mirror that exactly.
    for (label, lib) in [(".Lmf_read_at", "fread"), (".Lmf_write_at", "fwrite")] {
        out.push_str(&format!(".p2align 4\n{}:\n", label));
        out.push_str("    push %rbx\n    push %r12\n    sub $8, %rsp\n");
        out.push_str("    mov %rdi, %rbx\n    mov %rsi, %r12\n");
        out.push_str("    lea (%r12,%rdx), %rdi\n    mov %rcx, %rdx\n");
        out.push_str("    mov $1, %esi\n    mov %rbx, %rcx\n");
        out.push_str(&format!("    call {}@PLT\n", lib));
        out.push_str("    add $8, %rsp\n    pop %r12\n    pop %rbx\n    ret\n");
    }
    // String helpers with the native backend's NULL-safe semantics.
    out.push_str(".p2align 4\n.Lmf_str_len:\n    test %rdi, %rdi\n    je 2f\n");
    out.push_str("    jmp strlen@PLT\n2:\n    xor %eax, %eax\n    ret\n");
    out.push_str(".p2align 4\n.Lmf_str_byte:\n    test %rdi, %rdi\n    je 2f\n");
    out.push_str("    movzbq (%rdi,%rsi), %rax\n    ret\n2:\n    xor %eax, %eax\n    ret\n");
    out.push_str(".p2align 4\n.Lmf_str_byte_n:\n    test %rdi, %rdi\n    je 2f\n");
    out.push_str("    cmp %rdx, %rsi\n    jae 2f\n    movzbq (%rdi,%rsi), %rax\n    ret\n");
    out.push_str("2:\n    xor %eax, %eax\n    ret\n");
    out.push_str(".p2align 4\n.Lmf_str_eq:\n    test %rdi, %rdi\n    jne 1f\n");
    out.push_str("    test %rsi, %rsi\n    sete %al\n    movzbq %al, %rax\n    ret\n");
    out.push_str("1:\n    test %rsi, %rsi\n    jne 2f\n    xor %eax, %eax\n    ret\n");
    out.push_str("2:\n    sub $8, %rsp\n    call strcmp@PLT\n    add $8, %rsp\n");
    out.push_str("    test %eax, %eax\n    sete %al\n    movzbq %al, %rax\n    ret\n");
    // pread_at(fd, buf, off_in_buf, len, file_off): seek then read into the
    // buffer fragment, matching the native backend exactly.
    out.push_str(".p2align 4\n.Lmf_pread_at:\n");
    out.push_str("    push %rbx\n    push %r12\n    push %r13\n    sub $8, %rsp\n");
    out.push_str("    mov %rdi, %rbx\n    lea (%rsi,%rdx), %r12\n    mov %rcx, %r13\n");
    out.push_str("    mov %r8, %rsi\n    mov %rbx, %rdi\n    xor %edx, %edx\n");
    out.push_str("    call fseek@PLT\n");
    out.push_str("    mov %r12, %rdi\n    mov $1, %esi\n    mov %r13, %rdx\n    mov %rbx, %rcx\n");
    out.push_str("    call fread@PLT\n");
    out.push_str("    add $8, %rsp\n    pop %r13\n    pop %r12\n    pop %rbx\n    ret\n");
    // file_size: save position, seek to end, tell, restore.
    out.push_str(".p2align 4\n.Lmf_size:\n    push %rbx\n    push %r12\n    push %r13\n    sub $8, %rsp\n    mov %rdi, %rbx\n");
    out.push_str("    call ftell@PLT\n    mov %rax, %r13\n");
    out.push_str("    mov %rbx, %rdi\n    xor %esi, %esi\n    mov $1, %edx\n    call fseek@PLT\n");
    out.push_str("    mov %rbx, %rdi\n    call ftell@PLT\n    mov %rax, %r12\n");
    out.push_str("    mov %rbx, %rdi\n    mov %r13, %rsi\n    xor %edx, %edx\n    call fseek@PLT\n");
    out.push_str("    mov %r12, %rax\n    add $8, %rsp\n    pop %r13\n    pop %r12\n    pop %rbx\n    ret\n");
    // String literals.
    let mut strs: Vec<(usize, String)> = Vec::new();
    for f in &prog.funcs {
        for b in &f.blocks {
            for i in &b.instrs {
                if let Instr::Str { id, s, .. } = i {
                    strs.push((*id, s.clone()));
                }
            }
        }
    }
    if !strs.is_empty() {
        out.push_str(".section .rodata\n");
        for (id, text) in &strs {
            out.push_str(&format!(".Lstr{}: .asciz \"{}\"\n", id, asm_escape(text)));
        }
    }
    out.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    peephole_asm(out)
}

/// Small assembly cleanups: remove self-moves and use `test` for comparisons
/// against zero. (Dropping a `jmp` to the next label was tried too: it helped
/// nbody slightly but cost matmul/sieve more, so it stays out.)
fn peephole_asm(asm: String) -> String {
    let lines: Vec<&str> = asm.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        // jmp to the next label (skipping alignment directives)
        if line.starts_with("    mov %") {
            if let Some((a, b)) = line.trim().strip_prefix("mov ").and_then(|rest| {
                let mut parts = rest.split(", ");
                Some((parts.next()?, parts.next()?))
            }) {
                if a == b {
                    i += 1;
                    continue;
                }
            }
        }
        if let Some(rest) = line.trim().strip_prefix("cmp $0, ") {
            out.push(format!("    test {}, {}", rest, rest));
            i += 1;
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }
    let mut result = out.join("\n");
    result.push('\n');
    result
}

fn asm_escape(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            b'\r' => out.push_str("\\r"),
            32..=126 => out.push(byte as char),
            other => out.push_str(&format!("\\{:03o}", other)),
        }
    }
    out
}
