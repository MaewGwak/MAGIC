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

#[derive(Clone, Copy, Debug, PartialEq)]
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
    Shr,
    Shl,
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
    Alloc { ptr_var: usize, elem: Ty, esz: u64, len: Value, arena: bool },
    ArenaBegin { dst: Value },
    ArenaEnd { mark: Value },
    Fill { ptr_var: usize, elem: Ty, esz: u64, len: Value, value: Value },
    StructInit { var_index: usize, slots: usize, fields: Vec<(u64, Ty, Value)> },
    AddrVar { dst: Value, var_index: usize, slots: usize },
    CopySlots { ptr_var: usize, var_index: usize, slots: usize },
    CopySlotsTo { dst_ptr_var: usize, src: Value, slots: usize },
    Select { dst: Value, cond: Value, a: Value, b: Value, float: bool },
    LoadCtx { dst_var: usize, ctx_var: usize, off: i64 },
    FuncAddr { dst: Value, name: String },
    IndexStoreBoolAtomic { ptr_var: usize, idx: Value, src: Value },
    FastDivPow2 { dst: Value, a: Value, k: u32, d: i64, rem: bool },
    Aos2Step { dst: Value, tmp: Value, ptr_var: usize, idx: Value, size: u64, off: u64, k: Value, len: LenRef, line: usize },
    SaxpyI32 { c_ptr: usize, c_len: LenRef, b_ptr: usize, b_len: LenRef, c_idx: Value, b_idx: Value, count: Value, av: Value, start: Value, limit: Value, line: usize },
    StoreVec { src_ptr_var: usize, ptr_var: usize, len_var: usize, cap_var: usize },
    LoadPtr { dst_var: usize, src_var: usize },
    StorePtr { dst_ptr_var: usize, src_var: usize },
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
    pub array_len_args: Vec<(usize, usize)>,
    pub param_vars: Vec<usize>,
    pub blocks: Vec<Block>,
    pub narrow_index: std::collections::HashSet<Value>,
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
    narrow: Vec<bool>,
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
    inlinable: HashMap<String, (Vec<String>, Expr)>,
    subst_stack: Vec<HashMap<String, Value>>,
    arena_depth: usize,
    ret: Ty,
    loops: Vec<(usize, usize)>,
    sret_var: Option<usize>,
    sret_hint: Option<(String, usize, usize)>,
    fname: String,
    par_counter: u32,
    par_body: bool,
    byte_bool: std::collections::HashSet<String>,
    load_cache: HashMap<usize, (Value, usize)>,
    suppress_unroll: u32,
    ast_consts: HashMap<String, i64>,
    subst_consts: Vec<HashMap<String, i64>>,
    local_arrays: std::collections::HashSet<String>,
    seen_lets: std::collections::HashSet<String>,
    var_init_const: HashMap<usize, i64>,
    extra: Vec<IrFunc>,
}

fn collect_par_array_names(stmts: &[Stmt], out: &mut std::collections::HashSet<String>) {
    for st in stmts {
        match &st.kind {
            StmtKind::For { par, body, .. } => {
                if *par {
                    let mut reads = std::collections::HashSet::new();
                    let mut declared = std::collections::HashSet::new();
                    for s in body {
                        let _ = par_collect_stmt(s, &mut reads, &mut declared);
                    }
                    out.extend(reads);
                }
                collect_par_array_names(body, out);
            }
            StmtKind::While { body, .. } | StmtKind::Arena { body } => {
                collect_par_array_names(body, out)
            }
            StmtKind::If { then, els, .. } => {
                collect_par_array_names(then, out);
                collect_par_array_names(els, out);
            }
            _ => {}
        }
    }
}

fn par_collect_expr(
    e: &Expr,
    reads: &mut std::collections::HashSet<String>,
    declared: &std::collections::HashSet<String>,
) -> Result<(), String> {
    match &e.kind {
        ExprKind::Var(n) => {
            if !declared.contains(n) {
                reads.insert(n.clone());
            }
            Ok(())
        }
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => Ok(()),
        ExprKind::Bin { l, r, .. } => {
            par_collect_expr(l, reads, declared)?;
            par_collect_expr(r, reads, declared)
        }
        ExprKind::Un { e, .. } | ExprKind::Cast { e, .. } => par_collect_expr(e, reads, declared),
        ExprKind::Index { name, index } => {
            if !declared.contains(name) {
                reads.insert(name.clone());
            }
            par_collect_expr(index, reads, declared)
        }
        ExprKind::ArrayLen(name) => {
            if !declared.contains(name) {
                reads.insert(name.clone());
            }
            Ok(())
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                par_collect_expr(a, reads, declared)?;
            }
            Ok(())
        }
        ExprKind::IndexField { name, index, .. } => {
            if !declared.contains(name) {
                reads.insert(name.clone());
            }
            par_collect_expr(index, reads, declared)
        }
        ExprKind::Field { .. } | ExprKind::FieldPath { .. } | ExprKind::IndexFieldPath { .. } => {
            Err("par for body cannot use struct fields yet".to_string())
        }
        ExprKind::StructLit { .. } => Err("par for body cannot build structs yet".to_string()),
        other => Err(format!("par for body does not support this expression yet ({:?})", other)),
    }
}

fn par_collect_stmt(
    st: &Stmt,
    reads: &mut std::collections::HashSet<String>,
    declared: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    match &st.kind {
        StmtKind::Let { name, init, .. } => {
            par_collect_expr(init, reads, declared)?;
            declared.insert(name.clone());
            Ok(())
        }
        StmtKind::Assign { name, value } => {
            if !declared.contains(name) {
                reads.insert(name.clone());
            }
            par_collect_expr(value, reads, declared)
        }
        StmtKind::If { cond, then, els } => {
            par_collect_expr(cond, reads, declared)?;
            for s in then.iter().chain(els.iter()) {
                par_collect_stmt(s, reads, declared)?;
            }
            Ok(())
        }
        StmtKind::While { cond, body } => {
            par_collect_expr(cond, reads, declared)?;
            for s in body {
                par_collect_stmt(s, reads, declared)?;
            }
            Ok(())
        }
        StmtKind::For { var, start, end, body, par } => {
            if *par {
                return Err("nested par for is not supported".to_string());
            }
            par_collect_expr(start, reads, declared)?;
            par_collect_expr(end, reads, declared)?;
            declared.insert(var.clone());
            for s in body {
                par_collect_stmt(s, reads, declared)?;
            }
            Ok(())
        }
        StmtKind::Print { value, .. } => par_collect_expr(value, reads, declared),
        StmtKind::ArrayLet { name, value, count, .. } => {
            par_collect_expr(value, reads, declared)?;
            par_collect_expr(count, reads, declared)?;
            declared.insert(name.clone());
            Ok(())
        }
        StmtKind::IndexAssign { name, index, value } => {
            if !declared.contains(name) {
                reads.insert(name.clone());
            }
            par_collect_expr(index, reads, declared)?;
            par_collect_expr(value, reads, declared)
        }
        StmtKind::IndexFieldAssign { name, index, value, .. } => {
            if !declared.contains(name) {
                reads.insert(name.clone());
            }
            par_collect_expr(index, reads, declared)?;
            par_collect_expr(value, reads, declared)
        }
        StmtKind::Expr(e) => par_collect_expr(e, reads, declared),
        StmtKind::Assert { cond } => par_collect_expr(cond, reads, declared),
        StmtKind::Return(_) => Err("par for body cannot return".to_string()),
        StmtKind::Break | StmtKind::Continue => {
            Err("break/continue inside par for is not supported".to_string())
        }
        StmtKind::Arena { .. } => Err("arena inside par for is not supported".to_string()),
        other => Err(format!("par for body does not support this statement yet ({:?})", other)),
    }
}

fn par_writes_scalar(st: &Stmt, name: &str) -> bool {
    match &st.kind {
        StmtKind::Assign { name: n, .. } => n == name,
        StmtKind::If { then, els, .. } => {
            then.iter().any(|s| par_writes_scalar(s, name))
                || els.iter().any(|s| par_writes_scalar(s, name))
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } | StmtKind::Arena { body } => {
            body.iter().any(|s| par_writes_scalar(s, name))
        }
        _ => false,
    }
}

fn stmt_has_break(st: &Stmt) -> bool {
    match &st.kind {
        StmtKind::Break => true,
        StmtKind::If { then, els, .. } => {
            then.iter().any(stmt_has_break) || els.iter().any(stmt_has_break)
        }
        StmtKind::Arena { body } => body.iter().any(stmt_has_break),
        StmtKind::While { .. } | StmtKind::For { .. } => false,
        _ => false,
    }
}

fn stmt_has_loop(st: &Stmt) -> bool {
    match &st.kind {
        StmtKind::While { .. } | StmtKind::For { .. } => true,
        StmtKind::If { then, els, .. } => {
            then.iter().any(stmt_has_loop) || els.iter().any(stmt_has_loop)
        }
        StmtKind::Arena { body } => body.iter().any(stmt_has_loop),
        _ => false,
    }
}

fn stmt_declares_name(st: &Stmt, name: &str) -> bool {
    match &st.kind {
        StmtKind::Let { name: n, .. } | StmtKind::ArrayLet { name: n, .. } => n == name,
        StmtKind::If { then, els, .. } => {
            then.iter().any(|s| stmt_declares_name(s, name))
                || els.iter().any(|s| stmt_declares_name(s, name))
        }
        StmtKind::While { body, .. } | StmtKind::Arena { body } => {
            body.iter().any(|s| stmt_declares_name(s, name))
        }
        StmtKind::For { var: fvar, body, .. } => {
            fvar == name || body.iter().any(|s| stmt_declares_name(s, name))
        }
        _ => false,
    }
}

fn stmt_has_continue(st: &Stmt) -> bool {
    match &st.kind {
        StmtKind::Continue => true,
        StmtKind::If { then, els, .. } => {
            then.iter().any(stmt_has_continue) || els.iter().any(stmt_has_continue)
        }
        StmtKind::Arena { body } => body.iter().any(stmt_has_continue),
        StmtKind::While { .. } | StmtKind::For { .. } => false,
        _ => false,
    }
}

fn expr_mentions(e: &Expr, name: &str) -> bool {
    match &e.kind {
        ExprKind::Var(n) => n == name,
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => false,
        ExprKind::Bin { l, r, .. } => expr_mentions(l, name) || expr_mentions(r, name),
        ExprKind::Un { e, .. } | ExprKind::Cast { e, .. } => expr_mentions(e, name),
        ExprKind::Index { index, .. } => expr_mentions(index, name),
        ExprKind::Field { .. } | ExprKind::FieldPath { .. } | ExprKind::ArrayLen(_) => false,
        ExprKind::IndexField { index, .. } | ExprKind::IndexFieldPath { index, .. } => {
            expr_mentions(index, name)
        }
        ExprKind::Call { args, .. } => args.iter().any(|a| expr_mentions(a, name)),
        ExprKind::StructLit { fields, .. } => fields.iter().any(|(_, v)| expr_mentions(v, name)),
        _ => false,
    }
}

fn expr_pure(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::ArrayLen(_) => true,
        ExprKind::Bin { l, r, .. } => expr_pure(l) && expr_pure(r),
        ExprKind::Un { e, .. } | ExprKind::Cast { e, .. } => expr_pure(e),
        _ => false,
    }
}

fn expr_same(a: &Expr, b: &Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Int(x), ExprKind::Int(y)) => x == y,
        (ExprKind::Float(x), ExprKind::Float(y)) => x == y,
        (ExprKind::Bool(x), ExprKind::Bool(y)) => x == y,
        (ExprKind::Str(x), ExprKind::Str(y)) => x == y,
        (ExprKind::Var(x), ExprKind::Var(y)) => x == y,
        (ExprKind::Bin { op: o1, l: l1, r: r1 }, ExprKind::Bin { op: o2, l: l2, r: r2 }) => {
            o1 == o2 && expr_same(l1, l2) && expr_same(r1, r2)
        }
        (ExprKind::Un { op: o1, e: e1 }, ExprKind::Un { op: o2, e: e2 }) => {
            o1 == o2 && expr_same(e1, e2)
        }
        (ExprKind::Cast { e: e1, ty: t1 }, ExprKind::Cast { e: e2, ty: t2 }) => {
            t1 == t2 && expr_same(e1, e2)
        }
        (ExprKind::Index { name: n1, index: i1 }, ExprKind::Index { name: n2, index: i2 }) => {
            n1 == n2 && expr_same(i1, i2)
        }
        _ => false,
    }
}

/// Split an index expression into `coeff * j + rest`; `rest` is cloned and
/// must be loop-invariant. Only integer literals and casts appear on the
/// coefficient path, so the result is linear.
fn split_affine_j(e: &Expr, j: &str) -> Option<(i64, Expr)> {
    match &e.kind {
        ExprKind::Var(n) if n == j => Some((1, Expr { kind: ExprKind::Int(0), line: e.line })),
        ExprKind::Int(k) => Some((0, Expr { kind: ExprKind::Int(*k), line: e.line })),
        ExprKind::Var(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {
            if expr_mentions(e, j) { None } else { Some((0, e.clone())) }
        }
        ExprKind::Un { op: UnOp::Neg, e: inner } => {
            let (c, r) = split_affine_j(inner, j)?;
            Some((-c, Expr { kind: ExprKind::Un { op: UnOp::Neg, e: Box::new(r) }, line: e.line }))
        }
        ExprKind::Cast { e: inner, ty } => {
            let (c, r) = split_affine_j(inner, j)?;
            Some((c, Expr { kind: ExprKind::Cast { e: Box::new(r), ty: *ty }, line: e.line }))
        }
        ExprKind::Bin { op: BinOp::Add, l, r } => {
            let (a, ra) = split_affine_j(l, j)?;
            let (b, rb) = split_affine_j(r, j)?;
            Some((a + b, Expr { kind: ExprKind::Bin { op: BinOp::Add, l: Box::new(ra), r: Box::new(rb) }, line: e.line }))
        }
        ExprKind::Bin { op: BinOp::Sub, l, r } => {
            let (a, ra) = split_affine_j(l, j)?;
            let (b, rb) = split_affine_j(r, j)?;
            Some((a - b, Expr { kind: ExprKind::Bin { op: BinOp::Sub, l: Box::new(ra), r: Box::new(rb) }, line: e.line }))
        }
        ExprKind::Bin { op: BinOp::Mul, l, r } => {
            let (lc, lr) = split_affine_j(l, j)?;
            let (rc, rr) = split_affine_j(r, j)?;
            if lc == 0 && rc == 0 {
                if expr_mentions(e, j) { return None; }
                return Some((0, e.clone()));
            }
            if lc == 0 {
                let k = match lr.kind { ExprKind::Int(k) => k, _ => return None };
                Some((rc * k, Expr { kind: ExprKind::Bin { op: BinOp::Mul, l: Box::new(rr), r: Box::new(Expr { kind: ExprKind::Int(k), line: e.line }) }, line: e.line }))
            } else if rc == 0 {
                let k = match rr.kind { ExprKind::Int(k) => k, _ => return None };
                Some((lc * k, Expr { kind: ExprKind::Bin { op: BinOp::Mul, l: Box::new(lr), r: Box::new(Expr { kind: ExprKind::Int(k), line: e.line }) }, line: e.line }))
            } else {
                None
            }
        }
        _ if !expr_mentions(e, j) => Some((0, e.clone())),
        _ => None,
    }
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
            narrow: Vec::new(),
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
            inlinable: HashMap::new(),
            subst_stack: Vec::new(),
            arena_depth: 0,
            loops: Vec::new(),
            sret_var: None,
            sret_hint: None,
            fname: String::new(),
            par_counter: 0,
            par_body: false,
            byte_bool: Default::default(),
            load_cache: HashMap::new(),
            suppress_unroll: 0,
            ast_consts: HashMap::new(),
            subst_consts: Vec::new(),
            local_arrays: Default::default(),
            seen_lets: Default::default(),
            var_init_const: HashMap::new(),
            extra: Vec::new(),
            ret,
        }
    }

    fn value(&mut self) -> Value {
        let v = Value(self.nvalues as u32);
        self.nvalues += 1;
        self.vbool.push(false);
        self.vty.push(Ty::I32);
        self.narrow.push(false);
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

    fn const_eval(&self, e: &Expr) -> Option<i64> {
        match &e.kind {
            ExprKind::Int(k) => Some(*k),
            ExprKind::Bool(b) => Some(if *b { 1 } else { 0 }),
            ExprKind::Var(n) => self
                .subst_consts
                .iter()
                .rev()
                .find_map(|m| m.get(n).copied())
                .or_else(|| self.ast_consts.get(n).copied()),
            ExprKind::Un { op: UnOp::Neg, e } => self.const_eval(e)?.checked_neg(),
            ExprKind::Un { op: UnOp::Not, e } => {
                Some(if self.const_eval(e)? == 0 { 1 } else { 0 })
            }
            ExprKind::Cast { e, ty } => {
                let v = self.const_eval(e)?;
                match ty {
                    Ty::I32 => Some((v as i32) as i64),
                    Ty::I64 => Some(v),
                    Ty::U8 => Some(v & 0xff),
                    Ty::Bool => Some(if v == 0 { 0 } else { 1 }),
                    _ => None,
                }
            }
            ExprKind::Bin { op, l, r } => {
                let a = self.const_eval(l)?;
                let b = self.const_eval(r)?;
                match op {
                    BinOp::Add => a.checked_add(b),
                    BinOp::Sub => a.checked_sub(b),
                    BinOp::Mul => a.checked_mul(b),
                    BinOp::Div => a.checked_div(b),
                    BinOp::Rem => a.checked_rem(b),
                    BinOp::Eq => Some(if a == b { 1 } else { 0 }),
                    BinOp::Ne => Some(if a != b { 1 } else { 0 }),
                    BinOp::Lt => Some(if a < b { 1 } else { 0 }),
                    BinOp::Le => Some(if a <= b { 1 } else { 0 }),
                    BinOp::Gt => Some(if a > b { 1 } else { 0 }),
                    BinOp::Ge => Some(if a >= b { 1 } else { 0 }),
                    BinOp::And => Some(a & b),
                    BinOp::Or => Some(a | b),
                }
            }
            _ => None,
        }
    }

    fn push(&mut self, i: Instr) {
        if let Instr::StoreVar { var, .. } = &i {
            self.load_cache.remove(var);
        }
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
                let fits = *v >= i32::MIN as i64 && *v <= i32::MAX as i64;
                self.vty[dst.0 as usize] = if fits { Ty::I32 } else { Ty::I64 };
                self.narrow[dst.0 as usize] = fits;
                Ok(dst)
            }
            ExprKind::Bool(b) => {
                let dst = self.value();
                self.push(Instr::Const { dst, v: if *b { 1 } else { 0 } });
                self.vbool[dst.0 as usize] = true;
                Ok(dst)
            }
            ExprKind::VecArgPtr(n) | ExprKind::VecParamArgPtr(n) => {
                let (_, pv, _, _, _) = *self
                    .vecs
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, n))?;
                let dst = self.value();
                self.push(Instr::AddrVar { dst, var_index: pv, slots: 1 });
                self.vty[dst.0 as usize] = Ty::I64;
                Ok(dst)
            }
            ExprKind::VecArgLen(n) | ExprKind::VecParamArgLen(n) => {
                let (_, _, lv, _, _) = *self
                    .vecs
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, n))?;
                let dst = self.value();
                self.push(Instr::AddrVar { dst, var_index: lv, slots: 1 });
                self.vty[dst.0 as usize] = Ty::I64;
                Ok(dst)
            }
            ExprKind::VecArgCap(n) | ExprKind::VecParamArgCap(n) => {
                let (_, _, _, cv, _) = *self
                    .vecs
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, n))?;
                let dst = self.value();
                self.push(Instr::AddrVar { dst, var_index: cv, slots: 1 });
                self.vty[dst.0 as usize] = Ty::I64;
                Ok(dst)
            }
            ExprKind::VecParamLen(n) => {
                let (_, _, lv, _, _) = *self
                    .vecs
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, n))?;
                let dst = self.value();
                self.push(Instr::LoadVar { dst, var: lv });
                self.vty[dst.0 as usize] = Ty::I32;
                self.narrow[dst.0 as usize] = true;
                Ok(dst)
            }
            ExprKind::VecParamIndex { name, index } => {
                let (elem, pv, lv, _, _) = *self
                    .vecs
                    .get(name)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, name))?;
                let idx = self.lower_expr(index)?;
                self.push(Instr::BoundsCheck { idx, len: LenRef::Var(lv), line: e.line });
                let dst = self.value();
                self.push(Instr::IndexLoad { dst, ptr_var: pv, idx, elem });
                self.vty[dst.0 as usize] = elem;
                self.vbool[dst.0 as usize] = elem == Ty::Bool;
                self.narrow[dst.0 as usize] = matches!(elem, Ty::I32 | Ty::U8 | Ty::Bool);
                Ok(dst)
            }
            ExprKind::VecParamPop(n) => {
                let (elem, pv, lv, _, _) = *self
                    .vecs
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, n))?;
                let lenv = self.value();
                self.push(Instr::LoadVar { dst: lenv, var: lv });
                self.vty[lenv.0 as usize] = Ty::I32;
                let zero = self.value();
                self.push(Instr::Const { dst: zero, v: 0 });
                self.vty[zero.0 as usize] = Ty::I32;
                let cond = self.value();
                self.push(Instr::Bin { dst: cond, op: Op::Gt, l: lenv, r: zero });
                self.vty[cond.0 as usize] = Ty::Bool;
                self.vbool[cond.0 as usize] = true;
                self.narrow[cond.0 as usize] = true;
                self.push(Instr::Assert { cond, line: e.line });
                let one = self.value();
                self.push(Instr::Const { dst: one, v: 1 });
                self.vty[one.0 as usize] = Ty::I32;
                let newlen = self.value();
                self.push(Instr::Bin { dst: newlen, op: Op::Sub, l: lenv, r: one });
                self.vty[newlen.0 as usize] = Ty::I32;
                self.push(Instr::StoreVar { var: lv, src: newlen });
                let dst = self.value();
                self.push(Instr::IndexLoad { dst, ptr_var: pv, idx: newlen, elem });
                self.vty[dst.0 as usize] = elem;
                self.vbool[dst.0 as usize] = elem == Ty::Bool;
                self.narrow[dst.0 as usize] = matches!(elem, Ty::I32 | Ty::U8 | Ty::Bool);
                Ok(dst)
            }
            ExprKind::VecParamClear(n) => {
                let (_, _, lv, _, _) = *self
                    .vecs
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, n))?;
                let zero = self.value();
                self.push(Instr::Const { dst: zero, v: 0 });
                self.vty[zero.0 as usize] = Ty::I32;
                self.push(Instr::StoreVar { var: lv, src: zero });
                let dst = self.value();
                self.push(Instr::Const { dst, v: 0 });
                self.vty[dst.0 as usize] = Ty::I32;
                self.narrow[dst.0 as usize] = true;
                Ok(dst)
            }
            ExprKind::VecParamReserve { name, value } => {
                let (_, pv, lv, cv, esz) = *self
                    .vecs
                    .get(name)
                    .ok_or_else(|| format!("line {}: IR: unknown vector `{}`", e.line, name))?;
                let need = self.lower_expr(value)?;
                self.push(Instr::VecReserve {
                    ptr_var: pv,
                    len_var: lv,
                    cap_var: cv,
                    esz,
                    need,
                });
                let dst = self.value();
                self.push(Instr::Const { dst, v: 0 });
                self.vty[dst.0 as usize] = Ty::I32;
                self.narrow[dst.0 as usize] = true;
                Ok(dst)
            }
            ExprKind::StructLit { name: sname, fields } => {
                let id = self
                    .struct_defs
                    .iter()
                    .position(|d| d.name == *sname)
                    .ok_or_else(|| format!("line {}: IR: unknown struct `{}`", e.line, sname))?
                    as u16;
                let (size, _) = struct_layout(&self.struct_defs, id);
                let slots = ((size + 7) / 8) as usize;
                let var_index = self.nvars;
                self.nvars += slots;
                let mut inits = Vec::new();
                self.lower_struct_into(id, 0, fields, &mut inits)?;
                self.push(Instr::StructInit { var_index, slots, fields: inits });
                let dst = self.value();
                self.push(Instr::AddrVar { dst, var_index, slots });
                self.vty[dst.0 as usize] = Ty::I64;
                Ok(dst)
            }
            ExprKind::Var(n) => {
                if let Some(v) = self
                    .subst_stack
                    .iter()
                    .rev()
                    .find_map(|m| m.get(n).copied())
                {
                    return Ok(v);
                }
                if let Some((_, pv, _, _, _)) = self.vecs.get(n).copied() {
                    let dst = self.value();
                    self.push(Instr::AddrVar { dst, var_index: pv, slots: 3 });
                    self.vty[dst.0 as usize] = Ty::I64;
                    return Ok(dst);
                }
                if let Some((var_index, slots, _)) = self.struct_vars.get(n).copied() {
                    let dst = self.value();
                    self.push(Instr::AddrVar { dst, var_index, slots });
                    self.vty[dst.0 as usize] = Ty::I64;
                    return Ok(dst);
                }
                let var = *self
                    .vars
                    .get(n)
                    .ok_or_else(|| format!("line {}: IR: unknown variable `{}`", e.line, n))?;
                if let Some((v, b)) = self.load_cache.get(&var).copied() {
                    if b == self.cur {
                        return Ok(v);
                    }
                }
                let dst = self.value();
                self.push(Instr::LoadVar { dst, var });
                self.load_cache.insert(var, (dst, self.cur));
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
                    self.vty[dst.0 as usize] = Ty::Bool;
                    self.narrow[dst.0 as usize] = true;
                    return Ok(dst);
                }
                let ta = self.vty[a.0 as usize];
                let tb = self.vty[b.0 as usize];
                let rt = if ta == Ty::I64 || tb == Ty::I64 {
                    Ty::I64
                } else {
                    Ty::I32
                };
                self.vty[dst.0 as usize] = rt;
                if rt == Ty::I32 {
                    self.narrow[dst.0 as usize] = false;
                    return self.truncate_value(dst, Ty::I32);
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
                    self.vty[dst.0 as usize] = Ty::Bool;
                    self.narrow[dst.0 as usize] = true;
                    return Ok(dst);
                }
                let ta = self.vty[a.0 as usize];
                self.vty[dst.0 as usize] = ta;
                if ta == Ty::I32 {
                    self.narrow[dst.0 as usize] = false;
                    return self.truncate_value(dst, Ty::I32);
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
                if self.byte_bool.contains(name) && elem == Ty::U8 {
                    self.vty[dst.0 as usize] = Ty::Bool;
                    self.vbool[dst.0 as usize] = true;
                } else {
                    self.vty[dst.0 as usize] = elem;
                    self.narrow[dst.0 as usize] = matches!(elem, Ty::I32 | Ty::U8 | Ty::Bool);
                }
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
                self.narrow[dst.0 as usize] = matches!(ty, Ty::I32 | Ty::U8 | Ty::Bool);
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
                self.narrow[dst.0 as usize] = matches!(ty, Ty::I32 | Ty::U8 | Ty::Bool);
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
                self.narrow[dst.0 as usize] = matches!(ty, Ty::I32 | Ty::U8 | Ty::Bool);
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
                self.narrow[dst.0 as usize] = true;
                Ok(dst)
            }
            ExprKind::Call { name, args } => {
                if let Some((params, body)) = self.inlinable.get(name).cloned() {
                    if params.len() == args.len() {
                        let mut vals = Vec::new();
                        for a in args {
                            vals.push(self.lower_expr(a)?);
                        }
                        let map: HashMap<String, Value> =
                            params.into_iter().zip(vals).collect();
                        self.subst_stack.push(map);
                        let r = self.lower_expr(&body);
                        self.subst_stack.pop();
                        return r;
                    }
                }
                if (name == "net_read_at" || name == "net_write_at") && args.len() == 4 {
                    let fd = self.lower_expr(&args[0])?;
                    let buf = self.lower_expr(&args[1])?;
                    let off = self.lower_expr(&args[2])?;
                    let len = self.lower_expr(&args[3])?;
                    let cap = match &args[1].kind {
                        ExprKind::Var(vn) => {
                            if let Some((_, l)) = self.arrays.get(vn).copied() {
                                let v = self.value();
                                match l {
                                    LenRef::Const(n) => self.push(Instr::Const { dst: v, v: n }),
                                    LenRef::Var(var) => self.push(Instr::LoadVar { dst: v, var }),
                                }
                                self.vty[v.0 as usize] = Ty::I32;
                                v
                            } else if let Some((_, _, lv, _, _)) = self.vecs.get(vn).copied() {
                                let v = self.value();
                                self.push(Instr::LoadVar { dst: v, var: lv });
                                self.vty[v.0 as usize] = Ty::I32;
                                v
                            } else {
                                return Err(format!("line {}: IR: net_read_at buffer must be an array or vector", e.line));
                            }
                        }
                        _ => {
                            return Err(format!(
                                "line {}: IR: net_read_at buffer must be a variable",
                                e.line
                            ))
                        }
                    };
                    let zero = self.value();
                    self.push(Instr::Const { dst: zero, v: 0 });
                    self.vty[zero.0 as usize] = Ty::I32;
                    for (l, r, op) in [(off, zero, Op::Ge), (len, zero, Op::Ge), (off, cap, Op::Le)] {
                        let c = self.value();
                        self.push(Instr::Bin { dst: c, op, l, r });
                        self.vty[c.0 as usize] = Ty::Bool;
                        self.vbool[c.0 as usize] = true;
                        self.push(Instr::Assert { cond: c, line: e.line });
                    }
                    let room = self.value();
                    self.push(Instr::Bin { dst: room, op: Op::Sub, l: cap, r: off });
                    self.vty[room.0 as usize] = Ty::I32;
                    let c = self.value();
                    self.push(Instr::Bin { dst: c, op: Op::Le, l: len, r: room });
                    self.vty[c.0 as usize] = Ty::Bool;
                    self.vbool[c.0 as usize] = true;
                    self.push(Instr::Assert { cond: c, line: e.line });
                    let dst = self.value();
                    let label = if name == "net_read_at" { ".Lnet_read_at" } else { ".Lnet_write_at" };
                    self.push(Instr::Call {
                        dst: Some(dst),
                        name: label.to_string(),
                        args: vec![fd, buf, off, len],
                    });
                    self.vty[dst.0 as usize] = Ty::I32;
                    return Ok(dst);
                }
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
                    "flush" => Some(".Lmf_flush"),
                    "env" => Some(".Lmf_env"),
                    "net_listen" => Some(".Lnet_listen"),
                    "net_port" => Some(".Lnet_port"),
                    "net_accept" => Some(".Lnet_accept"),
                    "net_connect" => Some(".Lnet_connect"),
                    "net_read" => Some(".Lnet_read"),
                    "net_write" => Some(".Lnet_write"),
                    "net_close" => Some(".Lnet_close"),
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
                    && (name.starts_with("file_")
                        || name.starts_with("net_"))
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
                        if let Some((_, len)) = self.arrays.get(vname).copied() {
                            let dst = self.value();
                            match len {
                                LenRef::Const(n) => self.push(Instr::Const { dst, v: n }),
                                LenRef::Var(var) => self.push(Instr::LoadVar { dst, var }),
                            }
                            self.vty[dst.0 as usize] = Ty::I32;
                            self.narrow[dst.0 as usize] = true;
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
                if (name == "min" || name == "max") && args.len() == 2 {
                    let a = self.lower_expr(&args[0])?;
                    let b = self.lower_expr(&args[1])?;
                    let float = self.vty[a.0 as usize] == Ty::F64
                        || self.vty[b.0 as usize] == Ty::F64;
                    let cmp = if name == "min" { Op::Lt } else { Op::Gt };
                    let cond = self.value();
                    if float {
                        self.push(Instr::FCmp { dst: cond, op: cmp, l: a, r: b });
                        self.vbool[cond.0 as usize] = true;
                        self.vty[cond.0 as usize] = Ty::Bool;
                    } else {
                        self.push(Instr::Bin { dst: cond, op: cmp, l: a, r: b });
                        self.vbool[cond.0 as usize] = true;
                        self.vty[cond.0 as usize] = Ty::Bool;
                    }
                    let dst = self.value();
                    self.push(Instr::Select { dst, cond, a, b, float });
                    self.vty[dst.0 as usize] = if float {
                        Ty::F64
                    } else {
                        self.vty[a.0 as usize]
                    };
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
                let ret = self.sigs.get(name).copied();
                if let Some(Ty::Struct(id)) = ret {
                    let (size, _) = struct_layout(&self.struct_defs, id);
                    let slots = ((size + 7) / 8) as usize;
                    let vi = self.nvars;
                    self.nvars += slots;
                    let addr = self.value();
                    self.push(Instr::AddrVar { dst: addr, var_index: vi, slots });
                    self.vty[addr.0 as usize] = Ty::I64;
                    vals.push(addr);
                    self.push(Instr::Call {
                        dst: None,
                        name: builtin.unwrap_or(name.as_str()).to_string(),
                        args: vals,
                    });
                    return Ok(addr);
                }
                let dst = self.value();
                self.push(Instr::Call {
                    dst: Some(dst),
                    name: builtin.unwrap_or(name.as_str()).to_string(),
                    args: vals,
                });
                self.vty[dst.0 as usize] = if name == "env" {
                    Ty::Str
                } else {
                    ret.unwrap_or(Ty::I32)
                };
                Ok(dst)
            }
            other => Err(format!(
                "line {}: IR: expression not supported yet ({:?})",
                e.line, other
            )),
        }
    }

    fn coerce_value(&mut self, v: Value, target: Ty) -> Result<Value, String> {
        if target == Ty::F64 {
            if self.vty[v.0 as usize] == Ty::F64 {
                return Ok(v);
            }
            let dst = self.value();
            self.push(Instr::I2F { dst, a: v });
            self.vty[dst.0 as usize] = Ty::F64;
            return Ok(dst);
        }
        if self.vty[v.0 as usize] == Ty::F64 {
            let iv = self.value();
            self.push(Instr::F2I { dst: iv, a: v });
            self.vty[iv.0 as usize] = Ty::I64;
            return self.truncate_value(iv, target);
        }
        match target {
            Ty::I64 | Ty::Str | Ty::Struct(_) | Ty::Void => Ok(v),
            _ => self.truncate_value(v, target),
        }
    }

    fn truncate_value(&mut self, v: Value, ty: Ty) -> Result<Value, String> {
        let from = self.vty[v.0 as usize];
        if from == Ty::F64 || ty == Ty::F64 || ty == Ty::I64 || ty == Ty::Str {
            return Ok(v);
        }
        if self.narrow[v.0 as usize] && from == ty {
            return Ok(v);
        }
        let dst = self.value();
        self.push(Instr::Trunc { dst, ty, a: v });
        self.vty[dst.0 as usize] = ty;
        self.narrow[dst.0 as usize] = true;
        self.vbool[dst.0 as usize] = ty == Ty::Bool;
        Ok(dst)
    }

    fn lower_vec_push(&mut self, name: &str, value: &Expr, line: usize) -> Result<(), String> {
        let _ = line;
        let Some((elem, ptr_var, len_var, cap_var, esz)) = self.vecs.get(name).copied() else {
            return Err(format!("line {}: IR: unknown vector `{}`", line, name));
        };
                let Some((elem, ptr_var, len_var, cap_var, esz)) =
        self.vecs.get(name).copied()
                else {
        return Err(format!("line {}: IR: unknown vector `{}`", line, name));
                };
                if let Ty::Struct(id) = elem {
        let (size, _) = struct_layout(&self.struct_defs, id);
        let slots = ((size + 7) / 8) as usize;
        let tmp_var = self.nvars;
        self.nvars += slots;
        let mut inits = Vec::new();
        if let ExprKind::StructLit { fields, .. } = &value.kind {
            self.lower_struct_into(id, 0, fields, &mut inits)?;
        } else if let ExprKind::Var(vn) = &value.kind {
            let Some((src_idx, src_slots, _)) = self.struct_vars.get(vn).copied() else {
                return Err(format!(
                    "line {}: IR: vector push of an unknown struct `{}`",
                    line, vn
                ));
            };
            let addr = self.value();
            self.push(Instr::AddrVar { dst: addr, var_index: src_idx, slots: src_slots });
            self.vty[addr.0 as usize] = Ty::I64;
            let scratch = self.nvars;
            self.nvars += 1;
            self.push(Instr::StoreVar { var: scratch, src: addr });
            self.push(Instr::CopySlots { ptr_var: scratch, var_index: tmp_var, slots });
            self.push(Instr::VecPushStruct {
                ptr_var,
                len_var,
                cap_var,
                esz,
                size,
                tmp_var,
                tmp_slots: slots,
            });
            return Ok(());
        } else {
            return Err(format!(
                "line {}: IR: vector push of a non-literal struct is not supported yet",
                line
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
        Ok(())
    }

    fn lower_par_for(
        &mut self,
        var: &str,
        start: &Expr,
        end: &Expr,
        body: &[Stmt],
        line: usize,
    ) -> Result<(), String> {
        let mut reads: std::collections::HashSet<String> = Default::default();
        let mut declared: std::collections::HashSet<String> = Default::default();
        for st in body {
            par_collect_stmt(st, &mut reads, &mut declared)?;
        }
        reads.remove(var);
        let mut caps: Vec<(String, Option<(Ty, LenRef)>)> = Vec::new();
        let mut names: Vec<String> = reads.into_iter().collect();
        names.sort();
        for n in names {
            if let Some((elem, len)) = self.arrays.get(&n).copied() {
                caps.push((n, Some((elem, len))));
            } else if self.vecs.contains_key(&n) {
                return Err(format!("line {}: par for cannot capture vector `{}`", line, n));
            } else if self.struct_vars.contains_key(&n) {
                return Err(format!("line {}: par for cannot capture struct `{}`", line, n));
            } else if self.vars.contains_key(&n) {
                if body.iter().any(|s| par_writes_scalar(s, &n)) {
                    return Err(format!(
                        "line {}: par for cannot write captured scalar `{}` (use an array)",
                        line, n
                    ));
                }
                caps.push((n, None));
            } else {
                return Err(format!("line {}: par for uses unknown variable `{}`", line, n));
            }
        }
        let sv = self.lower_expr(start)?;
        let ev = self.lower_expr(end)?;
        let total_slots: usize = caps
            .iter()
            .map(|(_, a)| if a.is_some() { 2 } else { 1 })
            .sum();
        let ctx_base = self.nvars;
        self.nvars += total_slots;
        let mut slot = 0usize;
        for (name, arr) in &caps {
            if let Some((_elem, len)) = arr {
                let pv = *self.vars.get(name).unwrap();
                let v = self.value();
                self.push(Instr::LoadVar { dst: v, var: pv });
                self.vty[v.0 as usize] = Ty::I64;
                self.push(Instr::StoreVar { var: ctx_base + slot, src: v });
                let lv = self.value();
                match len {
                    LenRef::Const(c) => self.push(Instr::Const { dst: lv, v: *c }),
                    LenRef::Var(vv) => self.push(Instr::LoadVar { dst: lv, var: *vv }),
                }
                self.vty[lv.0 as usize] = Ty::I64;
                self.push(Instr::StoreVar { var: ctx_base + slot + 1, src: lv });
                slot += 2;
            } else {
                let vv = *self.vars.get(name).unwrap();
                let v = self.value();
                self.push(Instr::LoadVar { dst: v, var: vv });
                self.push(Instr::StoreVar { var: ctx_base + slot, src: v });
                slot += 1;
            }
        }
        let mut cb = Builder::new(Ty::Void);
        self.par_counter += 1;
        let fname = format!("{}__par{}", self.fname, self.par_counter);
        cb.fname = fname.clone();
        cb.par_body = true;
        cb.sigs = self.sigs.clone();
        cb.struct_defs = self.struct_defs.clone();
        cb.inlinable = self.inlinable.clone();
        cb.nvars = 3;
        for (i, nm) in ["#ctx", "#lo", "#hi"].iter().enumerate() {
            cb.vars.insert(nm.to_string(), i);
            cb.var_ty.insert(nm.to_string(), Ty::I64);
        }
        let mut slot = 0usize;
        for (name, arr) in &caps {
            if let Some((elem, _)) = arr {
                let pv = cb.nvars;
                cb.nvars += 1;
                cb.push(Instr::LoadCtx { dst_var: pv, ctx_var: 0, off: -((slot * 8) as i64) });
                let lv = cb.nvars;
                cb.nvars += 1;
                cb.push(Instr::LoadCtx { dst_var: lv, ctx_var: 0, off: -(((slot + 1) * 8) as i64) });
                cb.vars.insert(name.clone(), pv);
                cb.var_ty.insert(name.clone(), Ty::I64);
                let byte_bool = *elem == Ty::Bool && self.byte_bool.contains(name);
                if byte_bool {
                    cb.byte_bool.insert(name.clone());
                }
                let elem_eff = if byte_bool { Ty::U8 } else { *elem };
                cb.arrays.insert(name.clone(), (elem_eff, LenRef::Var(lv)));
                slot += 2;
            } else {
                let v = cb.nvars;
                cb.nvars += 1;
                cb.push(Instr::LoadCtx { dst_var: v, ctx_var: 0, off: -((slot * 8) as i64) });
                cb.vars.insert(name.clone(), v);
                let ty = self.var_ty.get(name).copied().unwrap_or(Ty::I32);
                cb.var_ty.insert(name.clone(), ty);
                if ty == Ty::Bool {
                    cb.var_bool.insert(name.clone(), true);
                }
                slot += 1;
            }
        }
        let idx = cb.var(var);
        let ity = self.var_ty.get(var).copied().unwrap_or(Ty::I32);
        cb.var_ty.insert(var.to_string(), ity);
        let lo = cb.value();
        cb.push(Instr::LoadVar { dst: lo, var: 1 });
        cb.vty[lo.0 as usize] = Ty::I64;
        cb.push(Instr::StoreVar { var: idx, src: lo });
        let head = cb.new_block();
        let body_bb = cb.new_block();
        let end_bb = cb.new_block();
        cb.push(Instr::Jmp(head));
        cb.cur = head;
        let iv = cb.value();
        cb.push(Instr::LoadVar { dst: iv, var: idx });
        cb.vty[iv.0 as usize] = Ty::I32;
        let hv = cb.value();
        cb.push(Instr::LoadVar { dst: hv, var: 2 });
        cb.vty[hv.0 as usize] = Ty::I64;
        let cv = cb.value();
        cb.push(Instr::Bin { dst: cv, op: Op::Lt, l: iv, r: hv });
        cb.vty[cv.0 as usize] = Ty::Bool;
        cb.vbool[cv.0 as usize] = true;
        cb.push(Instr::Br { cond: cv, then_bb: body_bb, else_bb: end_bb });
        cb.cur = body_bb;
        for st in body {
            cb.lower_stmt(st)?;
        }
        let lv2 = cb.value();
        cb.push(Instr::LoadVar { dst: lv2, var: idx });
        cb.vty[lv2.0 as usize] = Ty::I32;
        let one = cb.value();
        cb.push(Instr::Const { dst: one, v: 1 });
        cb.vty[one.0 as usize] = Ty::I32;
        let nv = cb.value();
        cb.push(Instr::Bin { dst: nv, op: Op::Add, l: lv2, r: one });
        cb.push(Instr::StoreVar { var: idx, src: nv });
        cb.push(Instr::Jmp(head));
        cb.cur = end_bb;
        cb.push(Instr::Ret(None));
        let mut bf = cb.finish();
        bf.name = fname.clone();
        bf.nparams = 3;
        self.extra.push(bf);
        let fnaddr = self.value();
        self.push(Instr::FuncAddr { dst: fnaddr, name: fname });
        self.vty[fnaddr.0 as usize] = Ty::I64;
        let ctxaddr = self.value();
        self.push(Instr::AddrVar { dst: ctxaddr, var_index: ctx_base, slots: total_slots });
        self.vty[ctxaddr.0 as usize] = Ty::I64;
        self.push(Instr::Call {
            dst: None,
            name: ".Lpar_for".to_string(),
            args: vec![fnaddr, ctxaddr, sv, ev],
        });
        Ok(())
    }

    fn try_lower_saxpy(&mut self, cond: &Expr, body: &[Stmt], _line: usize) -> Result<bool, String> {
        if body.len() != 2 {
            return Ok(false);
        }
        let (j, limit) = match &cond.kind {
            ExprKind::Bin { op: BinOp::Lt, l, r } => match &l.kind {
                ExprKind::Var(j) => (j.clone(), r),
                _ => return Ok(false),
            },
            _ => return Ok(false),
        };
        if expr_mentions(limit, &j) || !expr_pure(limit) {
            return Ok(false);
        }
        let Some(c) = self.const_eval(limit) else { return Ok(false) };
        if !(0..=i32::MAX as i64).contains(&c) {
            return Ok(false);
        }
        match &body[1].kind {
            StmtKind::Assign { name, value } if *name == j => match &value.kind {
                ExprKind::Bin { op: BinOp::Add, l, r } => {
                    let ok = (matches!(&l.kind, ExprKind::Var(n) if *n == j)
                        && matches!(&r.kind, ExprKind::Int(1)))
                        || (matches!(&r.kind, ExprKind::Var(n) if *n == j)
                            && matches!(&l.kind, ExprKind::Int(1)));
                    if !ok {
                        return Ok(false);
                    }
                }
                _ => return Ok(false),
            },
            _ => return Ok(false),
        }
        let StmtKind::IndexAssign { name: cname, index: cidx, value } = &body[0].kind else {
            return Ok(false);
        };
        let ExprKind::Bin { op: BinOp::Add, l, r } = &value.kind else { return Ok(false) };
        let is_c = |e: &Expr| matches!(&e.kind, ExprKind::Index { name, index } if name == cname && expr_same(index, cidx));
        let mul = if is_c(l) { r } else if is_c(r) { l } else { return Ok(false) };
        let ExprKind::Bin { op: BinOp::Mul, l, r } = &mul.kind else { return Ok(false) };
        let (av, bidx, bname) = match (&l.kind, &r.kind) {
            (_, ExprKind::Index { name, index }) => (l, index, name),
            (ExprKind::Index { name, index }, _) => (r, index, name),
            _ => return Ok(false),
        };
        if bname == cname || expr_mentions(av, &j) || !expr_pure(av) {
            return Ok(false);
        }
        if !expr_pure(cidx) || !expr_pure(bidx) {
            return Ok(false);
        }
        if !self.local_arrays.contains(cname) || !self.local_arrays.contains(bname) {
            return Ok(false);
        }
        let Some((cc, _)) = split_affine_j(cidx, &j) else { return Ok(false) };
        let Some((bc, _)) = split_affine_j(bidx, &j) else { return Ok(false) };
        if cc != 1 || bc != 1 {
            return Ok(false);
        }
        let Some((celem, clen)) = self.arrays.get(cname).copied() else { return Ok(false) };
        let Some((belem, blen)) = self.arrays.get(bname).copied() else { return Ok(false) };
        if celem != Ty::I32 || belem != Ty::I32 {
            return Ok(false);
        }
        let Some(cptr) = self.vars.get(cname).copied() else { return Ok(false) };
        let Some(bptr) = self.vars.get(bname).copied() else { return Ok(false) };
        let Some(jv) = self.vars.get(&j).copied() else { return Ok(false) };
        if self.var_init_const.get(&jv).copied() != Some(0) {
            return Ok(false);
        }
        let c_line = body[0].line;
        if c == 0 {
            return Ok(true);
        }
        let zero = self.value();
        self.push(Instr::Const { dst: zero, v: 0 });
        self.vty[zero.0 as usize] = Ty::I32;
        let mut sm = HashMap::new();
        sm.insert(j.clone(), zero);
        self.subst_stack.push(sm);
        let cbase = self.lower_expr(cidx)?;
        let bbase = self.lower_expr(bidx)?;
        self.subst_stack.pop();
        let avv = self.lower_expr(av)?;
        if self.vty[avv.0 as usize] != Ty::I32 {
            return Ok(false);
        }
        let count = self.value();
        self.push(Instr::Const { dst: count, v: c });
        self.vty[count.0 as usize] = Ty::I32;
        let start = self.value();
        self.push(Instr::Const { dst: start, v: 0 });
        self.vty[start.0 as usize] = Ty::I32;
        self.push(Instr::SaxpyI32 {
            c_ptr: cptr,
            c_len: clen,
            b_ptr: bptr,
            b_len: blen,
            c_idx: cbase,
            b_idx: bbase,
            count,
            av: avv,
            start,
            limit: count,
            line: c_line,
        });
        self.push(Instr::StoreVar { var: jv, src: count });
        self.var_init_const.insert(jv, c);
        Ok(true)
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
                return self.lower_vec_push(name, value, s.line);
            }
            StmtKind::VecParamPush { name, value } => {
                return self.lower_vec_push(name, value, s.line);
            }
            StmtKind::Arena { body } => {
                let mark = self.value();
                self.push(Instr::ArenaBegin { dst: mark });
                self.arena_depth += 1;
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.arena_depth -= 1;
                self.push(Instr::ArenaEnd { mark });
                return Ok(());
            }
            StmtKind::Assert { cond } => {
                let c = self.lower_expr(cond)?;
                self.push(Instr::Assert { cond: c, line: s.line });
                return Ok(());
            }
            StmtKind::Let { name, ty: declared, mutable, init } => {
                if !*mutable {
                    if let Some(c) = self.const_eval(init) {
                        self.ast_consts.insert(name.clone(), c);
                    }
                }
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
                if let ExprKind::Call { name: cname, .. } = &init.kind {
                    if let Some(Ty::Struct(id)) = self.sigs.get(cname).copied() {
                        let (size, _) = struct_layout(&self.struct_defs, id);
                        let slots = ((size + 7) / 8) as usize;
                        let var_index = self.nvars;
                        self.nvars += slots;
                        self.struct_vars.insert(name.clone(), (var_index, slots, id));
                        let ptr = self.lower_expr(init)?;
                        let scratch = self.nvars;
                        self.nvars += 1;
                        self.push(Instr::StoreVar { var: scratch, src: ptr });
                        self.push(Instr::CopySlots { ptr_var: scratch, var_index, slots });
                        return Ok(());
                    }
                }
                let v = self.lower_expr(init)?;
                let is_bool = self.vbool[v.0 as usize];
                let ty = declared.unwrap_or(self.vty[v.0 as usize]);
                let var = self.var(name);
                let v = self.coerce_value(v, ty)?;
                self.push(Instr::StoreVar { var, src: v });
                self.var_bool.insert(name.clone(), ty == Ty::Bool);
                self.var_ty.insert(name.clone(), ty);
            }
            StmtKind::Assign { name, value } => {
                let v = self.lower_expr(value)?;
                let var = *self
                    .vars
                    .get(name)
                    .ok_or_else(|| format!("line {}: IR: unknown variable `{}`", s.line, name))?;
                let target = self.var_ty.get(name).copied().unwrap_or(Ty::I64);
                let v = self.coerce_value(v, target)?;
                self.push(Instr::StoreVar { var, src: v });
                self.var_bool.insert(name.clone(), target == Ty::Bool);
                self.var_ty.insert(name.clone(), target);
            }
            StmtKind::ArrayLet { name, elem, value, count } => {
                self.local_arrays.insert(name.clone());
                let lenv = self.lower_expr(count)?;
                let ptr_var = self.var(name);
                let byte_bool = *elem == Ty::Bool && self.byte_bool.contains(name);
                let elem_eff = if byte_bool { Ty::U8 } else { *elem };
                let esz = if byte_bool { 1 } else { crate::ty_size(*elem, &self.struct_defs) };
                self.push(Instr::Alloc {
                    ptr_var,
                    elem: elem_eff,
                    esz,
                    len: lenv,
                    arena: self.arena_depth > 0,
                });
                let len_var = self.nvars;
                self.nvars += 1;
                self.push(Instr::StoreVar { var: len_var, src: lenv });
                let len_ref = LenRef::Var(len_var);
                self.arrays.insert(name.clone(), (elem_eff, len_ref));
                let zero = is_zero_expr(value);
                if !zero {
                    if let Ty::Struct(id) = *elem {
                        let (size, _) = struct_layout(&self.struct_defs, id);
                        let slots = ((size + 7) / 8) as usize;
                        let tmp_var = self.nvars;
                        self.nvars += slots;
                        let mut inits = Vec::new();
                        match &value.kind {
                            ExprKind::StructLit { fields, .. } => {
                                self.lower_struct_into(id, 0, fields, &mut inits)?;
                            }
                            ExprKind::Var(vn) => {
                                let Some((src_idx, _, _)) = self.struct_vars.get(vn).copied() else {
                                    return Err(format!(
                                        "line {}: IR: unknown struct `{}` for array fill",
                                        s.line, vn
                                    ));
                                };
                                let addr = self.value();
                                self.push(Instr::AddrVar { dst: addr, var_index: src_idx, slots });
                                self.vty[addr.0 as usize] = Ty::I64;
                                let scratch = self.nvars;
                                self.nvars += 1;
                                self.push(Instr::StoreVar { var: scratch, src: addr });
                                self.push(Instr::CopySlots { ptr_var: scratch, var_index: tmp_var, slots });
                            }
                            _ => {
                                return Err(format!(
                                    "line {}: IR: struct-array fill needs a literal or struct variable",
                                    s.line
                                ))
                            }
                        }
                        if !inits.is_empty() {
                            self.push(Instr::StructInit { var_index: tmp_var, slots, fields: inits });
                        }
                        let idx_var = self.nvars;
                        self.nvars += 1;
                        let zero = self.value();
                        self.push(Instr::Const { dst: zero, v: 0 });
                        self.vty[zero.0 as usize] = Ty::I32;
                        self.push(Instr::StoreVar { var: idx_var, src: zero });
                        let head = self.new_block();
                        let body = self.new_block();
                        let end = self.new_block();
                        self.push(Instr::Jmp(head));
                        self.cur = head;
                        let iv = self.value();
                        self.push(Instr::LoadVar { dst: iv, var: idx_var });
                        self.vty[iv.0 as usize] = Ty::I32;
                        let cv = self.value();
                        self.push(Instr::Bin { dst: cv, op: Op::Lt, l: iv, r: lenv });
                        self.push(Instr::Br { cond: cv, then_bb: body, else_bb: end });
                        self.cur = body;
                        let def = self
                            .struct_defs
                            .get(id as usize)
                            .cloned()
                            .ok_or_else(|| format!("line {}: IR: unknown struct", s.line))?;
                        for (fname, _fty) in &def.fields {
                            let (off, ty) = struct_field(&self.struct_defs, id, fname)
                                .ok_or_else(|| format!("line {}: IR: bad struct field", s.line))?;
                            let fv = self.value();
                            self.push(Instr::FieldLoad {
                                dst: fv,
                                var_index: tmp_var,
                                slots,
                                off,
                                ty,
                            });
                            self.vty[fv.0 as usize] = ty;
                            self.push(Instr::IndexFieldStore {
                                ptr_var,
                                idx: iv,
                                size,
                                off,
                                ty,
                                src: fv,
                                len: LenRef::Var(len_var),
                                line: s.line,
                            });
                        }
                        let one = self.value();
                        self.push(Instr::Const { dst: one, v: 1 });
                        self.vty[one.0 as usize] = Ty::I32;
                        let nv = self.value();
                        self.push(Instr::Bin { dst: nv, op: Op::Add, l: iv, r: one });
                        self.push(Instr::StoreVar { var: idx_var, src: nv });
                        self.push(Instr::Jmp(head));
                        self.cur = end;
                        return Ok(());
                    }
                    let init = self.lower_expr(value)?;
                    self.push(Instr::Fill {
                        ptr_var,
                        elem: elem_eff,
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
                    if self.par_body && elem == Ty::Bool {
                        self.push(Instr::IndexStoreBoolAtomic { ptr_var, idx, src });
                    } else {
                        self.push(Instr::IndexStore { ptr_var, idx, elem, src });
                    }
                    return Ok(());
                }
                let (elem, len) = match self.arrays.get(name) {
                    Some(x) => *x,
                    None => {
                        return Err(format!("line {}: IR: unknown array `{}`", s.line, name));
                    }
                };
                let idx = self.lower_expr(index)?;
                self.push(Instr::BoundsCheck { idx, len, line: s.line });
                let src = self.lower_expr(value)?;
                let ptr_var = *self.vars.get(name).ok_or_else(|| {
                    format!("line {}: IR: unknown array `{}`", s.line, name)
                })?;
                if self.par_body && elem == Ty::Bool {
                    self.push(Instr::IndexStoreBoolAtomic { ptr_var, idx, src });
                } else {
                    self.push(Instr::IndexStore { ptr_var, idx, elem, src });
                }
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
                if let Ty::Struct(id) = self.ret {
                    let (size, _) = struct_layout(&self.struct_defs, id);
                    let slots = ((size + 7) / 8) as usize;
                    let Some(sret) = self.sret_var else {
                        return Err(format!("line {}: IR: missing sret slot", s.line));
                    };
                    let src = match e {
                        Some(e) => self.lower_expr(e)?,
                        None => return Err(format!("line {}: IR: struct return needs a value", s.line)),
                    };
                    self.push(Instr::CopySlotsTo { dst_ptr_var: sret, src, slots });
                    self.push(Instr::Ret(None));
                    return Ok(());
                }
                let v = match e {
                    Some(e) => {
                        let val = self.lower_expr(e)?;
                        Some(self.truncate_value(val, self.ret)?)
                    }
                    None => None,
                };
                self.push(Instr::Ret(v));
            }
            StmtKind::Expr(e) => {
                if let ExprKind::Call { name, args } = &e.kind {
                    if name == "print" && args.len() == 1 {
                        let v = self.lower_expr(&args[0])?;
                        let boolean = self.vbool[v.0 as usize];
                        let vt = self.vty[v.0 as usize];
                        self.push(Instr::Print {
                            v,
                            boolean,
                            float: vt == Ty::F64,
                            string: vt == Ty::Str,
                        });
                        return Ok(());
                    }
                    if name == "print_prec" && args.len() == 2 {
                        let v = self.lower_expr(&args[0])?;
                        let prec = self.lower_expr(&args[1])?;
                        self.push(Instr::PrintPrec { v, prec });
                        return Ok(());
                    }
                    if name == "assert" && args.len() == 1 {
                        let c = self.lower_expr(&args[0])?;
                        self.push(Instr::Assert { cond: c, line: e.line });
                        return Ok(());
                    }
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
                // Blocked on lexical scoping: the IR builder reuses a variable
                // slot for a shadowing declaration, so a guarded saxpy cannot
                // distinguish sibling re-declarations from real shadowing yet.
                let head = self.new_block();
                let body_bb = self.new_block();
                let end_bb = self.new_block();
                self.push(Instr::Jmp(head));
                self.cur = head;
                let c = self.lower_expr(cond)?;
                self.push(Instr::Br { cond: c, then_bb: body_bb, else_bb: end_bb });
                self.cur = body_bb;
                self.loops.push((end_bb, head));
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.loops.pop();
                self.push(Instr::Jmp(head));
                self.cur = end_bb;
            }
            StmtKind::For { var, start, end, body, par } => {
                if *par {
                    return self.lower_par_for(var, start, end, body, s.line);
                }
                // Fully unroll constant, small ranges. This removes the loop
                // machinery and, more importantly, turns every index into a
                // constant so the bounds prover can drop its checks (n-body's
                // four-body nests are the motivating case).
                if true {
                if let (Some(a), Some(b)) = (self.const_eval(start), self.const_eval(end)) {
                    let n = b.saturating_sub(a);
                    if (0..=8).contains(&n)
                        && body.len() <= 64
                        && !body.iter().any(|st| {
                            stmt_has_break(st) || stmt_has_continue(st) || stmt_declares_name(st, var) || stmt_has_loop(st)
                        })
                    {
                        for k in a..b {
                            let cv = self.value();
                            self.push(Instr::Const { dst: cv, v: k });
                            let mut sm = HashMap::new();
                            sm.insert(var.clone(), cv);
                            self.subst_stack.push(sm);
                            let mut cm = HashMap::new();
                            cm.insert(var.clone(), k);
                            self.subst_consts.push(cm);
                            for st in body {
                                self.lower_stmt(st)?;
                            }
                            self.subst_consts.pop();
                            self.subst_stack.pop();
                        }
                        return Ok(());
                    }
                }
                }
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
                let has_continue = body.iter().any(stmt_has_continue);
                let incr_bb = if has_continue { Some(self.new_block()) } else { None };
                self.loops.push((end_bb, incr_bb.unwrap_or(head)));
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.loops.pop();
                if let Some(ib) = incr_bb {
                    self.push(Instr::Jmp(ib));
                    self.cur = ib;
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
            StmtKind::Break => {
                let Some((brk, _)) = self.loops.last().copied() else {
                    return Err(format!("line {}: IR: break outside a loop", s.line));
                };
                self.push(Instr::Jmp(brk));
                self.cur = self.new_block();
            }
            StmtKind::Continue => {
                let Some((_, cont)) = self.loops.last().copied() else {
                    return Err(format!("line {}: IR: continue outside a loop", s.line));
                };
                self.push(Instr::Jmp(cont));
                self.cur = self.new_block();
            }
            other => {
                return Err(format!(
                    "line {}: IR: statement not supported yet ({:?})",
                    s.line, other
                ))
            }
        }
        if let StmtKind::Let { name, init, .. } = &s.kind {
            if self.seen_lets.contains(name) {
                // A shadowing declaration must not change the outer constant.
                self.ast_consts.remove(name);
            } else {
                self.seen_lets.insert(name.clone());
                if let Some(c) = self.const_eval(init) {
                    self.ast_consts.insert(name.clone(), c);
                }
                if let Some(c) = self.const_eval(init) {
                    if let Some(v) = self.vars.get(name).copied() {
                        self.var_init_const.insert(v, c);
                    }
                }
            }
        }
        if matches!(
            s.kind,
            StmtKind::If { .. } | StmtKind::While { .. } | StmtKind::For { .. } | StmtKind::Arena { .. }
        ) {
            // Constants do not survive a merge or a loop.
            self.var_init_const.clear();
        }
        if let StmtKind::Assign { name, value } = &s.kind {
            if let Some(v) = self.vars.get(name).copied() {
                match self.const_eval(value) {
                    Some(c) => {
                        self.var_init_const.insert(v, c);
                    }
                    None => {
                        self.var_init_const.remove(&v);
                    }
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> IrFunc {
        IrFunc {
            name: String::new(),
            vty: self.vty.clone(),
            var_ty: {
                // Unnamed slots hold pointers/lengths (array and vector
                // descriptors); only named scalars carry an explicit type.
                let mut out = vec![Ty::I64; self.nvars];
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
            array_len_args: Vec::new(),
            param_vars: Vec::new(),
            blocks: std::mem::take(&mut self.blocks),
            narrow_index: std::collections::HashSet::new(),
        }
    }
}

pub fn lower(prog: &Program) -> Result<IrProgram, String> {
    let mut sigs: HashMap<String, Ty> = HashMap::new();
    for f in &prog.funcs {
        sigs.insert(f.name.clone(), f.ret);
    }
    fn trivial(e: &Expr, params: &std::collections::HashSet<String>) -> bool {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) => true,
            ExprKind::Var(n) => params.contains(n),
            ExprKind::Bin { l, r, .. } => trivial(l, params) && trivial(r, params),
            ExprKind::Un { e, .. } => trivial(e, params),
            ExprKind::Cast { e, .. } => trivial(e, params),
            _ => false,
        }
    }
    let mut inlinable: HashMap<String, (Vec<String>, Expr)> = HashMap::new();
    for f in &prog.funcs {
        if f.body.len() != 1 {
            continue;
        }
        let StmtKind::Return(Some(e)) = &f.body[0].kind else { continue };
        let params: Vec<String> = f
            .params
            .iter()
            .filter_map(|(n, p)| match p {
                ParamTy::Scalar(Ty::Struct(_)) => None,
                ParamTy::Scalar(_) => Some(n.clone()),
                _ => None,
            })
            .collect();
        if params.len() != f.params.len() {
            continue;
        }
        let set: std::collections::HashSet<String> = params.iter().cloned().collect();
        if trivial(e, &set) {
            inlinable.insert(f.name.clone(), (params, e.clone()));
        }
    }
    let mut funcs = Vec::new();
    let mut extras = Vec::new();
    for f in &prog.funcs {
        let (ir, mut ex) = lower_func(f, &sigs, &prog.structs, &inlinable)?;
        funcs.push(ir);
        extras.append(&mut ex);
    }
    funcs.append(&mut extras);
    Ok(IrProgram { funcs })
}

fn lower_func(
    f: &Func,
    sigs: &HashMap<String, Ty>,
    structs: &[StructDef],
    inlinable: &HashMap<String, (Vec<String>, Expr)>,
) -> Result<(IrFunc, Vec<IrFunc>), String> {
    let mut b = Builder::new(f.ret);
    b.fname = f.name.clone();
    collect_par_array_names(&f.body, &mut b.byte_bool);
    b.sigs = sigs.clone();
    b.struct_defs = structs.to_vec();
    b.inlinable = inlinable.clone();
    let mut nparams = 0usize;
    let mut array_len_args: Vec<(usize, usize)> = Vec::new();
    let mut struct_params: Vec<(String, u16, usize)> = Vec::new();
    let mut vec_params: Vec<(String, Ty, usize, usize, usize, u64)> = Vec::new();
    for (name, pty) in &f.params {
        match pty {
            ParamTy::Scalar(ty) => {
                if let Ty::Struct(id) = ty {
                    let ptr_name = format!("{}#ptr", name);
                    let pv = b.var(&ptr_name);
                    b.var_ty.insert(ptr_name.clone(), Ty::I64);
                    struct_params.push((name.clone(), *id, pv));
                    nparams += 1;
                    continue;
                }
                b.var(name);
                b.var_ty.insert(name.clone(), *ty);
                nparams += 1;
            }
            ParamTy::Array(elem) => {
                let pv = b.var(name);
                let lv = b.nvars;
                b.nvars += 1;
                b.arrays.insert(name.clone(), (*elem, LenRef::Var(lv)));
                array_len_args.push((nparams + 1, lv));
                let _ = pv;
                nparams += 2;
            }
            ParamTy::Vec(elem) => {
                let esz = crate::ty_size(*elem, &b.struct_defs);
                let pn = format!("{}#vp", name);
                let ln = format!("{}#vl", name);
                let cn = format!("{}#vc", name);
                let p = b.var(&pn);
                b.var_ty.insert(pn, Ty::I64);
                let l = b.var(&ln);
                b.var_ty.insert(ln, Ty::I64);
                let c = b.var(&cn);
                b.var_ty.insert(cn, Ty::I64);
                vec_params.push((name.clone(), *elem, p, l, c, esz));
                nparams += 3;
            }
        }
    }
    if let Ty::Struct(_) = f.ret {
        let sv = b.var("#sret");
        b.var_ty.insert("#sret".to_string(), Ty::I64);
        b.sret_var = Some(sv);
        nparams += 1;
    }
    for (name, id, ptr_var) in struct_params {
        let (size, _) = struct_layout(&b.struct_defs, id);
        let slots = ((size + 7) / 8) as usize;
        let var_index = b.nvars;
        b.nvars += slots;
        b.struct_vars.insert(name, (var_index, slots, id));
        b.push(Instr::CopySlots { ptr_var, var_index, slots });
    }
    for (name, elem, p_addr, l_addr, c_addr, esz) in &vec_params {
        let pv = b.nvars;
        b.nvars += 3;
        b.vecs
            .insert(name.clone(), (*elem, pv, pv + 1, pv + 2, *esz));
        b.push(Instr::LoadPtr { dst_var: pv, src_var: *p_addr });
        b.push(Instr::LoadPtr { dst_var: pv + 1, src_var: *l_addr });
        b.push(Instr::LoadPtr { dst_var: pv + 2, src_var: *c_addr });
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
    if !vec_params.is_empty() {
        for bi in 0..b.blocks.len() {
            if matches!(b.blocks[bi].instrs.last(), Some(Instr::Ret(_))) {
                let mut stores = Vec::new();
                for (name, _, p_addr, l_addr, c_addr, _) in &vec_params {
                    if let Some((_, pv, lv, cv, _)) = b.vecs.get(name).copied() {
                        stores.push(Instr::StorePtr { dst_ptr_var: *p_addr, src_var: pv });
                        stores.push(Instr::StorePtr { dst_ptr_var: *l_addr, src_var: lv });
                        stores.push(Instr::StorePtr { dst_ptr_var: *c_addr, src_var: cv });
                    }
                }
                let at = b.blocks[bi].instrs.len() - 1;
                for (k, ins) in stores.into_iter().enumerate() {
                    b.blocks[bi].instrs.insert(at + k, ins);
                }
            }
        }
    }
    let extras = std::mem::take(&mut b.extra);
    let mut out = b.finish();
    out.name = f.name.clone();
    out.nparams = nparams;
    out.array_len_args = array_len_args;
    out.param_vars = (0..nparams).collect();
    out.argc_var = argc_var;
    out.argv_var = argv_var;
    Ok((out, extras))
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

fn affine_has_negative_term(
    v: Value,
    defs: &HashMap<Value, (DefKind, usize)>,
    instrs: &[&Instr],
) -> bool {
    let Some((kind, _)) = defs.get(&v) else { return false };
    match kind {
        DefKind::Const(_) | DefKind::LoadVar(_) => false,
        DefKind::Other => {
            let Some(ins) = instrs.iter().find(|i| def_kind(i).map(|(d, _)| d) == Some(v)) else {
                return false;
            };
            match ins {
                Instr::Trunc { a, .. } => affine_has_negative_term(*a, defs, instrs),
                Instr::Un { neg: true, .. } => true,
                Instr::Bin { op, l, r, .. } => match op {
                    Op::Sub => true,
                    Op::Add | Op::Mul => {
                        affine_has_negative_term(*l, defs, instrs)
                            || affine_has_negative_term(*r, defs, instrs)
                    }
                    _ => false,
                },
                _ => false,
            }
        }
    }
}

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
            if let Instr::Trunc { a, .. } = ins {
                return affine_of(*a, defs, instrs, consts, vars);
            }
            match ins {
                Instr::Bin { op, l, r, .. } => match op {
                    Op::Add | Op::Sub => {
                        let (mut a, ca) = affine_of(*l, defs, instrs, consts, vars)?;
                        let (b, cb) = affine_of(*r, defs, instrs, consts, vars)?;
                        let sign: i64 = if *op == Op::Sub { -1 } else { 1 };
                        for (k, val) in b {
                            let add = sign.checked_mul(val)?;
                            let cur = a.entry(k).or_insert(0);
                            *cur = cur.checked_add(add)?;
                        }
                        let cterm = sign.checked_mul(cb)?;
                        Some((a, ca.checked_add(cterm)?))
                    }
                    Op::Mul => {
                        let (la, lc) = affine_of(*l, defs, instrs, consts, vars)?;
                        let (ra, rc) = affine_of(*r, defs, instrs, consts, vars)?;
                        if la.is_empty() && ra.is_empty() {
                            Some((HashMap::new(), lc.checked_mul(rc)?))
                        } else if la.is_empty() {
                            let k = lc;
                            let m = ra
                                .into_iter()
                                .map(|(v, c)| c.checked_mul(k).map(|x| (v, x)))
                                .collect::<Option<HashMap<_, _>>>()?;
                            Some((m, rc.checked_mul(k)?))
                        } else if ra.is_empty() {
                            let k = rc;
                            let m = la
                                .into_iter()
                                .map(|(v, c)| c.checked_mul(k).map(|x| (v, x)))
                                .collect::<Option<HashMap<_, _>>>()?;
                            Some((m, lc.checked_mul(k)?))
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                Instr::Un { neg: true, a, .. } => {
                    let (m, c) = affine_of(*a, defs, instrs, consts, vars)?;
                    let mm = m
                        .into_iter()
                        .map(|(v, cv)| cv.checked_neg().map(|x| (v, x)))
                        .collect::<Option<HashMap<_, _>>>()?;
                    Some((mm, c.checked_neg()?))
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
        Instr::ArenaBegin { dst } => Some((*dst, DefKind::Other)),
        Instr::FieldLoad { dst, .. } | Instr::IndexFieldLoad { dst, .. } => {
            Some((*dst, DefKind::Other))
        }
        _ => None,
    }
}

/// The loop index must be provably non-negative for an unsigned bounds check
/// to be removable: it must be initialised with a non-negative constant before
/// the header and only ever be incremented by one afterwards.
fn strip_trunc(f: &IrFunc, mut v: Value) -> Value {
    for _ in 0..4 {
        let Some(ins) = f
            .blocks
            .iter()
            .flat_map(|b| b.instrs.iter())
            .find(|i| dst_of(i) == Some(v))
        else {
            return v;
        };
        match ins {
            Instr::Trunc { a, .. } => v = *a,
            _ => return v,
        }
    }
    v
}

fn var_nonneg(
    defs: &HashMap<Value, (DefKind, usize)>,
    f: &IrFunc,
    iv: usize,
    head: usize,
    vals: &HashMap<Value, i64>,
) -> bool {
    let n = f.blocks.len();
    let mut good_init: Vec<usize> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for ins in &b.instrs {
            let Instr::StoreVar { var, src } = ins else { continue };
            if *var != iv {
                continue;
            }
            if let Some(c) = vals.get(src) {
                if *c < 0 {
                    return false;
                }
                if !good_init.contains(&bi) {
                    good_init.push(bi);
                }
                continue;
            }
            let src_inc = strip_trunc(f, *src);
            let mut increment = false;
            for b2 in &f.blocks {
                for i2 in &b2.instrs {
                    if let Instr::Bin { dst, op: Op::Add, l, r } = i2 {
                        if *dst == src_inc {
                            let is_iv = |v: &Value| {
                                matches!(defs.get(v), Some((DefKind::LoadVar(x), _)) if *x == iv)
                            };
                            let step_nonneg = |v: &Value| {
                                match defs.get(v) {
                                    Some((DefKind::Const(c), _)) => *c >= 0,
                                    _ => vals.get(v).map(|c| *c >= 0).unwrap_or(false),
                                }
                            };
                            increment = (is_iv(l) && step_nonneg(r)) || (is_iv(r) && step_nonneg(l));
                        }
                    }
                }
            }
            if !increment {
                return false;
            }
        }
    }
    if good_init.is_empty() {
        return false;
    }
    // Every path from entry to the header must pass through an initialising
    // block; otherwise the loop can start with an unknown (possibly negative)
    // value.
    let mut seen = vec![false; n];
    let mut stack = vec![0usize];
    while let Some(bi) = stack.pop() {
        if bi == head {
            return false;
        }
        if seen[bi] {
            continue;
        }
        seen[bi] = true;
        if good_init.contains(&bi) {
            continue;
        }
        for succ in block_successors(&f.blocks[bi].instrs) {
            if succ < n {
                stack.push(succ);
            }
        }
    }
    true
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
        // Dominator sets: a loop-bound fact is only valid at an access if the
        // header dominates the access block and the access executes before the
        // iteration's update of the induction variable.
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (bi, b) in f.blocks.iter().enumerate() {
            for succ in block_successors(&b.instrs) {
                if succ < n {
                    preds[succ].push(bi);
                }
            }
        }
        let mut dom: Vec<std::collections::HashSet<usize>> =
            vec![Default::default(); n];
        dom[0].insert(0);
        for d in dom.iter_mut().skip(1) {
            for i in 0..n {
                d.insert(i);
            }
        }
        loop {
            let mut changed = false;
            for b in 1..n {
                if preds[b].is_empty() {
                    continue;
                }
                let mut new: std::collections::HashSet<usize> =
                    dom[preds[b][0]].clone();
                for p in &preds[b][1..] {
                    new.retain(|x| dom[*p].contains(x));
                }
                new.insert(b);
                if new != dom[b] {
                    dom[b] = new;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut constraints: Vec<HashMap<usize, (Bound, usize)>> = vec![HashMap::new(); n];
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
            // The "true" edge is the only edge on which the header condition
            // holds. If the false/exit edge can reach a region block without
            // re-entering the header (short-circuit conditions such as
            // `while i < n || spin < m`), the body is reachable with the
            // condition false and the fact is invalid.
            let mut bad_else = false;
            let mut seen_else: std::collections::HashSet<usize> = Default::default();
            let mut stack_else = vec![exit];
            while let Some(bb) = stack_else.pop() {
                if bb == h {
                    continue;
                }
                if region.contains(&bb) {
                    bad_else = true;
                    break;
                }
                if !seen_else.insert(bb) {
                    continue;
                }
                for succ in block_successors(&f.blocks[bb].instrs) {
                    if succ < n {
                        stack_else.push(succ);
                    }
                }
            }
            if bad_else {
                continue;
            }
            // Head layout: LoadVar iv; ...; Bin Lt(l=iv', r=end) -> cond.
            let mut iv: Option<usize> = None;
            let mut bound: Option<Bound> = None;
            let Some(Instr::Br { cond: br_cond, .. }) = f.blocks[h].instrs.last() else {
                continue;
            };
            let Some(Instr::Bin { op: Op::Lt, l, r, .. }) = f
                .blocks[h]
                .instrs
                .iter()
                .find(|ins| dst_of(ins) == Some(*br_cond))
            else {
                continue;
            };
            {
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
            let (Some(iv), Some(bound)) = (iv, bound) else { continue };
            // Keep proofs inside the i32 range: the loop bound must be a
            // positive constant at most i32::MAX, otherwise an increment could
            // wrap before the loop exits and invalidate the "non-negative"
            // reasoning.
            let bound = match bound {
                Bound::Const(b) if b > 0 && b <= i32::MAX as i64 => Bound::Const(b),
                _ => continue,
            };
            if !var_nonneg(&defs, f, iv, h, &vals) {
                if std::env::var("MAGIC_DEBUG_NONNEG").is_ok() {
                    eprintln!("rejected loop h={} iv={} bound={:?}", h, iv, bound);
                }
                continue;
            }
            if region.iter().any(|bb| !dom[*bb].contains(&h)) {
                continue;
            }
            // The index may only be modified by `i = i + 1` inside the region.
            let mut ok = true;
            for bb in &region {
                for ins in &f.blocks[*bb].instrs {
                    if let Instr::StoreVar { var, src } = ins {
                        if *var == iv {
                            let src = &strip_trunc(f, *src);
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
            let mut inc_site: Option<(usize, usize)> = None;
            let mut inc_count = 0usize;
            for bb in &region {
                for (ii, ins) in f.blocks[*bb].instrs.iter().enumerate() {
                    if matches!(ins, Instr::StoreVar { var, .. } if *var == iv) {
                        inc_count += 1;
                        inc_site = Some((*bb, ii));
                    }
                }
            }
            if inc_count != 1 {
                continue;
            }
            let (inc_bi, inc_ii) = inc_site.unwrap();
            // Blocks reachable from the increment without re-entering the
            // header execute after it in the same iteration; the entry fact no
            // longer holds there. Mark them (nested loops can re-enter an
            // earlier-looking access block, so plain dominance is not enough).
            let mut after_inc: std::collections::HashSet<usize> = Default::default();
            let mut stack: Vec<usize> = block_successors(&f.blocks[inc_bi].instrs)
                .into_iter()
                .filter(|s| *s < n && *s != h && region.contains(s))
                .collect();
            while let Some(bb) = stack.pop() {
                if !after_inc.insert(bb) {
                    continue;
                }
                for succ in block_successors(&f.blocks[bb].instrs) {
                    if succ < n && succ != h && region.contains(&succ) {
                        stack.push(succ);
                    }
                }
            }
            for bb in &region {
                if *bb == inc_bi {
                    if !after_inc.contains(&inc_bi) {
                        constraints[*bb].insert(iv, (bound, inc_ii));
                    }
                } else if !after_inc.contains(bb) {
                    constraints[*bb].insert(iv, (bound, usize::MAX));
                }
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
                        let before = constraints[*bb]
                            .get(&iv)
                            .map(|(_, b)| *b)
                            .unwrap_or(0);
                        if idx_is_iv && len_matches && ii < before {
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
                            let before = constraints[*bb]
                                .get(&iv)
                                .map(|(_, b)| *b)
                                .unwrap_or(0);
                            if idx_is_iv && len_matches && ii < before {
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
                if len_const > i32::MAX as i64 {
                    continue;
                }
                if affine_has_negative_term(*idx, &defs, &all_instrs) {
                    continue;
                }
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
                        Some((Bound::Const(b), before)) if ii < *before => {
                            let term = coef.checked_mul(b.saturating_sub(1));
                            match term.and_then(|t| max.checked_add(t)) {
                                Some(m) => max = m,
                                None => {
                                    proved = false;
                                    break;
                                }
                            }
                        }
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
        Instr::ArenaBegin { dst } => defs.push(*dst),
        Instr::ArenaEnd { mark } => uses.push(*mark),
        Instr::Str { dst, .. } => defs.push(*dst),
        Instr::AddrVar { dst, .. } => defs.push(*dst),
        Instr::CopySlots { .. } => {}
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
        Instr::AddrVar { .. }
        | Instr::CopySlots { .. }
        | Instr::StoreVec { .. }
        | Instr::LoadPtr { .. }
        | Instr::StorePtr { .. } => {}
        Instr::CopySlotsTo { src, .. } => uses.push(*src),
        Instr::LoadCtx { .. } => {}
        Instr::FuncAddr { dst, .. } => defs.push(*dst),
        Instr::IndexStoreBoolAtomic { idx, src, .. } => {
            uses.push(*idx);
            uses.push(*src);
        }
        Instr::FastDivPow2 { dst, a, .. } => {
            uses.push(*a);
            defs.push(*dst);
        }
        Instr::Aos2Step { dst, tmp, idx, k, .. } => {
            uses.push(*idx);
            uses.push(*k);
            defs.push(*dst);
            defs.push(*tmp);
        }
        Instr::SaxpyI32 { c_idx, b_idx, count, av, start, limit, .. } => {
            uses.push(*c_idx);
            uses.push(*b_idx);
            uses.push(*count);
            uses.push(*av);
            uses.push(*start);
            uses.push(*limit);
        }
        Instr::Select { dst, cond, a, b, .. } => {
            uses.push(*cond);
            uses.push(*a);
            uses.push(*b);
            defs.push(*dst);
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
                | Instr::AddrVar { .. }
        | Instr::Select { .. }
        | Instr::FuncAddr { .. }
                | Instr::CopySlots { .. }
                | Instr::StoreVec { .. }
                | Instr::LoadPtr { .. }
                | Instr::StorePtr { .. }
                | Instr::CopySlotsTo { .. }
                | Instr::LoadCtx { .. }
                | Instr::FuncAddr { .. }
                | Instr::IndexStoreBoolAtomic { .. }
                | Instr::SaxpyI32 { .. }
        | Instr::AddrVar { .. }
                | Instr::VecNew { .. }
                | Instr::VecReserve { .. }
                | Instr::VecLen { .. }
                | Instr::VecPushScalar { .. }
                | Instr::VecPushStruct { .. }
                | Instr::Argv { .. }
                | Instr::InputRead { .. }
                | Instr::PrintBytes { .. }
                | Instr::Assert { .. }
                | Instr::ArenaBegin { .. }
                | Instr::ArenaEnd { .. }
        )
    })
}

const POOL: [(&str, &str, &str); 11] = [
    ("%rdx", "%edx", "%dl"),
    ("%rsi", "%esi", "%sil"),
    ("%rdi", "%edi", "%dil"),
    ("%r8", "%r8d", "%r8b"),
    ("%r9", "%r9d", "%r9b"),
    ("%r11", "%r11d", "%r11b"),
    ("%rbx", "%ebx", "%bl"),
    ("%r12", "%r12d", "%r12b"),
    ("%r13", "%r13d", "%r13b"),
    ("%r14", "%r14d", "%r14b"),
    ("%r15", "%r15d", "%r15b"),
];

fn load_index_reg(out: &mut String, f: &IrFunc, idx: Value, a: usize) {
    if f.vty.get(idx.0 as usize) == Some(&Ty::I32) {
        out.push_str(&format!("    movslq {}, %rax\n", POOL[a].1));
    } else {
        out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
    }
}

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
    scales: HashMap<(Value, u64), usize>,
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

    fn scaled_index(&mut self, idx: Value, size: u64, idx_reg: usize) -> usize {
        if let Some(&r) = self.scales.get(&(idx, size)) {
            return r;
        }
        if self.scales.len() >= 4 {
            if let Some(key) = self.scales.keys().next().copied() {
                if let Some(r) = self.scales.remove(&key) {
                    self.free.push(r);
                }
            }
        }
        let r = if self.free.is_empty() {
            let spilled = self.spill_excluding(&[idx_reg]);
            self.free.push(spilled);
            self.free.pop().unwrap()
        } else {
            self.free.pop().unwrap()
        };
        self.out.push_str(&format!(
            "    mov {}, {}\n",
            POOL[idx_reg].0,
            POOL[r].0
        ));
        if let Some(rest) = scale_insn(size, POOL[r].0).strip_prefix("    ") {
            self.out.push_str(&format!("    {}", rest));
        }
        self.scales.insert((idx, size), r);
        r
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
        scales: HashMap::new(),
    };
    for (ii, ins) in instrs.iter().enumerate() {
        match ins {
            Instr::AddrVar { .. }
            | Instr::CopySlots { .. }
            | Instr::StoreVec { .. }
            | Instr::LoadPtr { .. }
            | Instr::StorePtr { .. }
            | Instr::CopySlotsTo { .. }
            | Instr::LoadCtx { .. }
            | Instr::FuncAddr { .. }
            | Instr::IndexStoreBoolAtomic { .. } => {}
            Instr::Select { dst, cond, a, b, float } => {
                let c = ra.operand(*cond);
                if *float {
                    let xd = ra.xdst(*dst);
                    let xa = ra.xoperand(*a);
                    let xb = ra.xoperand(*b);
                    ra.out.push_str(&format!("    movapd {}, {}\n", XMM[xa], XMM[xd]));
                    ra.out.push_str(&format!("    movapd {}, %xmm1\n", XMM[xb]));
                    ra.out.push_str(&format!("    test {}, {}\n    jne 1f\n", POOL[c].0, POOL[c].0));
                    ra.out.push_str(&format!("    movapd %xmm1, {}\n1:\n", XMM[xd]));
                    ra.xdef_store(*dst, xd);
                    ra.free_dead(ii);
                    continue;
                }
                let a_reg = ra.operand(*a);
                let b_reg = ra.operand_excluding(*b, &[a_reg]);
                let d = ra.dst_excluding(*dst, &[a_reg, b_reg]);
                ra.out.push_str(&format!("    mov {}, {}\n", POOL[a_reg].0, POOL[d].0));
                ra.out.push_str(&format!("    mov {}, %rax\n", POOL[b_reg].0));
                ra.out.push_str(&format!("    test {}, {}\n", POOL[c].0, POOL[c].0));
                ra.out.push_str(&format!("    cmovne %rax, {}\n", POOL[d].0));
                ra.def_store(*dst, d);
                ra.free_dead(ii);
                continue;
            }
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
                    let i32v = f.var_ty.get(*var).copied() == Some(Ty::I32);
                    if let Some(reg) = var_regs.get(var) {
                        if i32v {
                            ra.out.push_str(&format!(
                                "    movslq {}, {}\n",
                                reg32_name(reg),
                                POOL[d].0
                            ));
                        } else {
                            ra.out.push_str(&format!("    mov {}, {}\n", reg, POOL[d].0));
                        }
                    } else if i32v {
                        ra.out.push_str(&format!(
                            "    movslq -{}(%rbp), {}\n",
                            varoff(*var),
                            POOL[d].0
                        ));
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
                    let i32v = f.var_ty.get(*var).copied() == Some(Ty::I32);
                    if let Some(reg) = var_regs.get(var) {
                        if i32v {
                            ra.out.push_str(&format!(
                                "    movl {}, {}\n",
                                POOL[a].1,
                                reg32_name(reg)
                            ));
                        } else {
                            ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, reg));
                        }
                    } else if i32v {
                        ra.out.push_str(&format!(
                            "    movl {}, -{}(%rbp)\n",
                            POOL[a].1,
                            varoff(*var)
                        ));
                    } else {
                        ra.out.push_str(&format!(
                            "    mov {}, -{}(%rbp)\n",
                            POOL[a].0,
                            varoff(*var)
                        ));
                    }
                }
            }
            Instr::FastDivPow2 { dst, a, k, d, rem } => {
                let av = ra.operand(*a);
                let dr = ra.dst_excluding(*dst, &[av]);
                let m = d - 1;
                let imm_ok = m <= i32::MAX as i64;
                let dv_ok = *d <= i32::MAX as i64;
                ra.out.push_str(&format!("    mov {}, {}\n", POOL[av].0, POOL[dr].0));
                ra.out.push_str(&format!("    test {}, {}\n", POOL[av].0, POOL[av].0));
                ra.out.push_str("    js 2f\n");
                if *rem {
                    if imm_ok {
                        ra.out.push_str(&format!("    and ${}, {}\n", m, POOL[dr].0));
                    } else {
                        ra.out.push_str(&format!("    movabs ${}, %rcx\n    and %rcx, {}\n", m, POOL[dr].0));
                    }
                } else {
                    ra.out.push_str(&format!("    sar ${}, {}\n", k, POOL[dr].0));
                }
                ra.out.push_str("    jmp 3f\n2:\n");
                if *rem {
                    if imm_ok {
                        ra.out.push_str(&format!("    and ${}, {}\n", m, POOL[dr].0));
                    } else {
                        ra.out.push_str(&format!("    movabs ${}, %rcx\n    and %rcx, {}\n", m, POOL[dr].0));
                    }
                    ra.out.push_str(&format!("    test {}, {}\n    jz 3f\n", POOL[dr].0, POOL[dr].0));
                    if dv_ok {
                        ra.out.push_str(&format!("    sub ${}, {}\n", d, POOL[dr].0));
                    } else {
                        ra.out.push_str(&format!("    movabs ${}, %rcx\n    sub %rcx, {}\n", d, POOL[dr].0));
                    }
                } else {
                    if dv_ok {
                        ra.out.push_str(&format!("    add ${}, {}\n", m, POOL[dr].0));
                    } else {
                        ra.out.push_str(&format!("    movabs ${}, %rcx\n    add %rcx, {}\n", m, POOL[dr].0));
                    }
                    ra.out.push_str(&format!("    sar ${}, {}\n", k, POOL[dr].0));
                }
                ra.out.push_str("3:\n");
                ra.def_store(*dst, dr);
                ra.free_dead(ii);
                continue;
            }
            Instr::SaxpyI32 { .. } => {
                unreachable!("SaxpyI32 must be lowered in a slow block");
            }
            Instr::Aos2Step { dst, tmp, ptr_var, idx, size, off, k, len, line } => {
                let i = ra.operand(*idx);
                if *line != 0 {
                    load_index_reg(&mut ra.out, f, *idx, i);
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
                    ra.out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                    ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                    ra.out.push_str("    call .Lir_panic\n1:\n");
                }
                let (base, temp_ptr) = match var_regs.get(ptr_var).copied() {
                    Some(reg) => (reg.to_string(), None),
                    None => {
                        let p = ra.alloc_excluding(&[i]);
                        ra.out.push_str(&format!("    mov -{}(%rbp), {}\n", varoff(*ptr_var), POOL[p].0));
                        (POOL[p].0.to_string(), Some(p))
                    }
                };
                let t = ra.scaled_index(*idx, *size, i);
                let xd = ra.xdst(*dst);
                let xv = ra.xdst_excluding(*tmp, &[xd]);
                ra.out.push_str(&format!("    movupd {}({},{}), {}\n", off, base, POOL[t].0, XMM[xd]));
                ra.out.push_str(&format!("    movupd {}({},{}), {}\n", off + 16, base, POOL[t].0, XMM[xv]));
                ra.out.push_str(&format!("    addpd {}, {}\n", XMM[xv], XMM[xd]));
                let kx = ra.xoperand(*k);
                ra.out.push_str(&format!("    unpcklpd {}, {}\n", XMM[kx], XMM[kx]));
                ra.out.push_str(&format!("    mulpd {}, {}\n", XMM[kx], XMM[xv]));
                ra.out.push_str(&format!("    movupd {}, {}({},{})\n", XMM[xd], off, base, POOL[t].0));
                ra.out.push_str(&format!("    movupd {}, {}({},{})\n", XMM[xv], off + 16, base, POOL[t].0));
                if let Some(p) = temp_ptr {
                    ra.free.push(p);
                }
                ra.xdef_store(*dst, xd);
                ra.xdef_store(*tmp, xv);
                ra.free_dead(ii);
                continue;
            }
            Instr::Bin { dst, op, l, r } => {
                if f.narrow_index.contains(dst)
                    && matches!(op, Op::Add | Op::Sub | Op::Mul | Op::And | Op::Or)
                {
                    let a = ra.operand(*l);
                    let b = ra.operand_excluding(*r, &[a]);
                    let d = match ra.dead_reg(*l, ii) {
                        Some(i) => ra.dst_at(*dst, i),
                        None => ra.dst_excluding(*dst, &[a, b]),
                    };
                    if d != a {
                        ra.out.push_str(&format!("    movl {}, {}\n", POOL[a].1, POOL[d].1));
                    }
                    let (mn, rev) = match op {
                        Op::Add => ("addl", false),
                        Op::Sub => ("subl", false),
                        Op::Mul => ("imull", false),
                        Op::And => ("andl", false),
                        Op::Or => ("orl", false),
                        _ => unreachable!(),
                    };
                    if rev {
                        ra.out.push_str(&format!("    {} {}, {}\n", mn, POOL[d].1, POOL[b].1));
                    } else {
                        ra.out.push_str(&format!("    {} {}, {}\n", mn, POOL[b].1, POOL[d].1));
                    }
                    ra.def_store(*dst, d);
                    ra.free_dead(ii);
                    continue;
                }
                if ra.deferred.contains_key(dst) {
                    // Emitted together with the branch that consumes it.
                    continue;
                }
                let commutative = matches!(op, Op::Add | Op::Mul | Op::And | Op::Or);
                let imm_r = ra.consts.get(r).copied().filter(|c| *c >= i32::MIN as i64 && *c <= i32::MAX as i64);
                let imm_l = ra.consts.get(l).copied().filter(|c| *c >= i32::MIN as i64 && *c <= i32::MAX as i64);
                if !matches!(op, Op::Div | Op::Rem | Op::Shr | Op::Shl) {
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
                        if d == a {
                            // constant - register: negate then add
                            ra.out.push_str(&format!("    neg {}\n", POOL[d].0));
                            ra.out.push_str(&format!("    add ${}, {}\n", imm, POOL[d].0));
                        } else {
                            ra.out.push_str(&format!("    mov ${}, {}\n", imm, POOL[d].0));
                            ra.out.push_str(&format!("    sub {}, {}\n", POOL[a].0, POOL[d].0));
                        }
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
                if matches!(op, Op::Shr | Op::Shl) {
                    if d != a {
                        ra.out.push_str(&format!("    mov {}, {}\n", POOL[a].0, POOL[d].0));
                    }
                    ra.out.push_str(&format!("    mov {}, %rcx\n", POOL[b].0));
                    if matches!(op, Op::Shr) {
                        ra.out.push_str(&format!("    sar %cl, {}\n", POOL[d].0));
                    } else {
                        ra.out.push_str(&format!("    shl %cl, {}\n", POOL[d].0));
                    }
                    ra.def_store(*dst, d);
                    ra.free_dead(ii);
                    continue;
                }
                if matches!(op, Op::Div | Op::Rem) {
                    ra.out.push_str(&format!("    mov {}, %rax\n", POOL[a].0));
                    ra.out.push_str(&format!("    mov {}, %rcx\n", POOL[b].0));
                    ra.out.push_str("    test %rcx, %rcx\n    jne 1f\n");
                    ra.out.push_str("    lea .Ldiv0(%rip), %rdi\n    call .Lir_panic\n1:\n");
                    ra.out.push_str("    cqo\n    idiv %rcx\n");
                    if matches!(op, Op::Rem) {
                        ra.out.push_str("    mov %rdx, %rax\n");
                    }
                    ra.out.push_str(&format!("    mov %rax, {}\n", POOL[d].0));
                    ra.def_store(*dst, d);
                    ra.free_dead(ii);
                    continue;
                }
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
                load_index_reg(&mut ra.out, f, *idx, a);
                match len {
                    LenRef::Const(n) => {
                        ra.out.push_str(&format!("    cmp ${}, %rax\n    jb 1f\n", n));
                    }
                    LenRef::Var(v) => {
                        if let Some(reg) = var_regs.get(v) {
                            ra.out.push_str(&format!("    mov {}, %rcx\n", reg));
                        } else {
                            ra.out.push_str(&format!("    mov -{}(%rbp), %rcx\n", varoff(*v)));
                        }
                        ra.out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                    }
                }
                ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                ra.out.push_str("    call .Lir_panic\n1:\n");
            }
            Instr::IndexLoad { dst, ptr_var, idx, elem } => {
                if ra.is_float(*dst) {
                    let a = ra.operand(*idx);
                    load_index_reg(&mut ra.out, f, *idx, a);
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
                load_index_reg(&mut ra.out, f, *idx, a);
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
                    load_index_reg(&mut ra.out, f, *idx, a);
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
                load_index_reg(&mut ra.out, f, *idx, a);
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
                let i = ra.operand(*idx);
                if *line != 0 {
                    load_index_reg(&mut ra.out, f, *idx, i);
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
                    ra.out.push_str("    cmp %rcx, %rax\n    jb 1f\n");
                    ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                    ra.out.push_str("    call .Lir_panic\n1:\n");
                }
                let (base, temp_ptr) = match var_regs.get(ptr_var).copied() {
                    Some(reg) => (reg.to_string(), None),
                    None => {
                        let p = ra.alloc_excluding(&[i]);
                        ra.out.push_str(&format!("    mov -{}(%rbp), {}\n", varoff(*ptr_var), POOL[p].0));
                        (POOL[p].0.to_string(), Some(p))
                    }
                };
                let t = ra.scaled_index(*idx, *size, i);
                let disp = if *off > 0 {
                    format!("{}({},{})", off, base, POOL[t].0)
                } else {
                    format!("({},{})", base, POOL[t].0)
                };
                if ra.is_float(*dst) {
                    let d = ra.xdst(*dst);
                    ra.out.push_str(&format!("    movsd {}, {}\n", disp, XMM[d]));
                    ra.xdef_store(*dst, d);
                } else {
                    let d = ra.dst(*dst);
                    match ty {
                        Ty::U8 | Ty::Bool => ra.out.push_str(&format!("    movzbq {}, {}\n", disp, POOL[d].0)),
                        Ty::I32 => ra.out.push_str(&format!("    movslq {}, {}\n", disp, POOL[d].0)),
                        _ => ra.out.push_str(&format!("    mov {}, {}\n", disp, POOL[d].0)),
                    }
                    ra.def_store(*dst, d);
                }
                if let Some(p) = temp_ptr {
                    ra.free.push(p);
                }
                ra.free_dead(ii);
                continue;
            }
            Instr::IndexFieldStore { ptr_var, idx, size, off, ty, src, len, line } => {
                let (sreg, i) = if ra.is_float(*src) {
                    let x = ra.xoperand(*src);
                    ra.out.push_str(&format!("    movapd {}, %xmm0\n", XMM[x]));
                    let i = ra.operand(*idx);
                    (None, i)
                } else {
                    let s = ra.operand(*src);
                    ra.out.push_str(&format!("    mov {}, %rcx\n", POOL[s].0));
                    let i = ra.operand_excluding(*idx, &[s]);
                    (Some(s), i)
                };
                let _ = sreg;
                if *line != 0 {
                    load_index_reg(&mut ra.out, f, *idx, i);
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
                    ra.out.push_str("    cmp %r10, %rax\n    jb 1f\n");
                    ra.out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n", line));
                    ra.out.push_str("    call .Lir_panic\n1:\n");
                }
                if let Some(reg) = var_regs.get(ptr_var) {
                    ra.out.push_str(&format!("    mov {}, %r10\n", reg));
                } else {
                    ra.out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                }
                let t = ra.scaled_index(*idx, *size, i);
                let disp = if *off > 0 {
                    format!("{}(%r10,{})", off, POOL[t].0)
                } else {
                    format!("(%r10,{})", POOL[t].0)
                };
                if ra.is_float(*src) {
                    ra.out.push_str(&format!("    movsd %xmm0, {}\n", disp));
                } else {
                    match ty {
                        Ty::U8 | Ty::Bool => ra.out.push_str(&format!("    movb %cl, {}\n", disp)),
                        Ty::I32 => ra.out.push_str(&format!("    movl %ecx, {}\n", disp)),
                        _ => ra.out.push_str(&format!("    movq %rcx, {}\n", disp)),
                    }
                }
                ra.free_dead(ii);
                continue;
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
        | Instr::AddrVar { .. }
            | Instr::VecNew { .. }
            | Instr::VecReserve { .. }
            | Instr::VecLen { .. }
            | Instr::VecPushScalar { .. }
            | Instr::VecPushStruct { .. }
            | Instr::Argv { .. }
            | Instr::InputRead { .. }
            | Instr::PrintBytes { .. }
            | Instr::Assert { .. }
            | Instr::ArenaBegin { .. }
            | Instr::ArenaEnd { .. } => {
                unreachable!("slow block")
            }
        }
        ra.free_dead(ii);
    }
}

fn ty_code(t: Ty) -> i64 {
    match t {
        Ty::I32 => 1,
        Ty::I64 => 2,
        Ty::U8 => 3,
        Ty::F64 => 4,
        Ty::Bool => 5,
        Ty::Str => 6,
        Ty::Void => 7,
        Ty::Struct(i) => 100 + i as i64,
    }
}

fn expr_key(ins: &Instr) -> Option<(u8, Vec<i64>)> {
    let mut k: Vec<i64> = Vec::new();
    match ins {
        Instr::Const { v, .. } => {
            k.push(1);
            k.push(*v);
        }
        Instr::LoadVar { var, .. } => {
            k.push(2);
            k.push(*var as i64);
        }
        Instr::Bin { op, l, r, .. } => {
            k.push(3);
            k.push(*op as i64);
            let (a, b) = if matches!(op, Op::Add | Op::Mul | Op::And | Op::Or) && l.0 > r.0 {
                (*r, *l)
            } else {
                (*l, *r)
            };
            k.push(a.0 as i64);
            k.push(b.0 as i64);
        }
        Instr::Un { neg, a, .. } => {
            k.push(4);
            k.push(*neg as i64);
            k.push(a.0 as i64);
        }
        Instr::Trunc { ty, a, .. } => {
            k.push(5);
            k.push(ty_code(*ty));
            k.push(a.0 as i64);
        }
        Instr::FBin { op, l, r, .. } => {
            k.push(6);
            k.push(*op as i64);
            k.push(l.0 as i64);
            k.push(r.0 as i64);
        }
        Instr::FNeg { a, .. } | Instr::I2F { a, .. } | Instr::F2I { a, .. } | Instr::Sqrt { a, .. } => {
            k.push(7);
            k.push(match ins {
                Instr::FNeg { .. } => 1,
                Instr::I2F { .. } => 2,
                Instr::F2I { .. } => 3,
                _ => 4,
            });
            k.push(a.0 as i64);
        }
        Instr::FCmp { op, l, r, .. } => {
            k.push(8);
            k.push(*op as i64);
            k.push(l.0 as i64);
            k.push(r.0 as i64);
        }
        _ => return None,
    }
    Some((k[0] as u8, k[1..].to_vec()))
}

fn dst_of(ins: &Instr) -> Option<Value> {
    match ins {
        Instr::Const { dst, .. }
        | Instr::LoadVar { dst, .. }
        | Instr::Bin { dst, .. }
        | Instr::Un { dst, .. }
        | Instr::Trunc { dst, .. }
        | Instr::FBin { dst, .. }
        | Instr::FNeg { dst, .. }
        | Instr::I2F { dst, .. }
        | Instr::F2I { dst, .. }
        | Instr::Sqrt { dst, .. }
        | Instr::FCmp { dst, .. } => Some(*dst),
        _ => None,
    }
}

/// Local value numbering inside each block: duplicate constant loads, variable
/// loads and pure arithmetic are replaced by the first occurrence. Only values
/// that are not used outside the block are merged (their slot stays unused).
fn cse_blocks(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let n = f.blocks.len();
        let mut used_outside = vec![false; f.nvalues.max(1)];
        for (bi, b) in f.blocks.iter().enumerate() {
            for ins in &b.instrs {
                let mut u = Vec::new();
                let mut d = Vec::new();
                instr_uses_defs(ins, &mut u, &mut d);
                for v in u {
                    // record whether this value is used in another block
                    let _ = &d;
                }
            }
            let _ = bi;
        }
        // per-block use membership
        let mut uses_in: Vec<std::collections::HashSet<Value>> =
            vec![Default::default(); n];
        for (bi, b) in f.blocks.iter().enumerate() {
            for ins in &b.instrs {
                let mut u = Vec::new();
                let mut d = Vec::new();
                instr_uses_defs(ins, &mut u, &mut d);
                for v in u {
                    uses_in[bi].insert(v);
                }
            }
        }
        // a value is "external" if any *other* block uses it
        let mut external = vec![false; f.nvalues.max(1)];
        for bi in 0..n {
            for v in &uses_in[bi] {
                let outside = (0..n).any(|other| other != bi && uses_in[other].contains(v));
                if outside {
                    external[v.0 as usize] = true;
                }
            }
        }
        for bi in 0..n {
            let mut subst: HashMap<Value, Value> = HashMap::new();
            let resolve = |v: Value, subst: &HashMap<Value, Value>| -> Value {
                let mut cur = v;
                let mut hops = 0;
                while let Some(next) = subst.get(&cur) {
                    cur = *next;
                    hops += 1;
                    if hops > 64 {
                        break;
                    }
                }
                cur
            };
            let mut keys: HashMap<(u8, Vec<i64>), Value> = HashMap::new();
            let mut remove: std::collections::HashSet<usize> = Default::default();
            let mut var_invalid = false;
            for (ii, ins) in f.blocks[bi].instrs.iter().enumerate() {
                if let Instr::StoreVar { var, .. } = ins {
                    keys.retain(|(kind, rest), _| !(*kind == 2 && rest[0] == *var as i64));
                    var_invalid = true;
                    continue;
                }
                let _ = var_invalid;
                if let Some(dst) = dst_of(ins) {
                    if !external[dst.0 as usize] {
                        // rewrite operand keys through substitutions
                        let mut canon = ins.clone();
                        rewrite_operands(&mut canon, &|v| resolve(v, &subst));
                        if let Some(mut k) = expr_key(&canon) {
                            let vt = f
                                .vty
                                .get(dst.0 as usize)
                                .copied()
                                .unwrap_or(Ty::I32);
                            k.1.push(ty_code(vt));
                            if let Some(prev) = keys.get(&k) {
                                subst.insert(dst, *prev);
                                remove.insert(ii);
                                continue;
                            }
                            keys.insert(k, dst);
                        }
                    }
                }
            }
            if remove.is_empty() {
                continue;
            }
            let old = std::mem::take(&mut f.blocks[bi].instrs);
            let mut new_instrs = Vec::new();
            for (ii, mut ins) in old.into_iter().enumerate() {
                if remove.contains(&ii) {
                    continue;
                }
                rewrite_operands(&mut ins, &|v| resolve(v, &subst));
                new_instrs.push(ins);
            }
            f.blocks[bi].instrs = new_instrs;
        }
    }
}

fn rewrite_operands(ins: &mut Instr, f: &dyn Fn(Value) -> Value) {
    let mut rw = |v: &mut Value| *v = f(*v);
    match ins {
        Instr::Bin { l, r, .. } => {
            rw(l);
            rw(r);
        }
        Instr::Un { a, .. }
        | Instr::Trunc { a, .. }
        | Instr::FNeg { a, .. }
        | Instr::I2F { a, .. }
        | Instr::F2I { a, .. }
        | Instr::Sqrt { a, .. } => rw(a),
        Instr::FBin { l, r, .. } | Instr::FCmp { l, r, .. } => {
            rw(l);
            rw(r);
        }
        Instr::StoreVar { src, .. } => rw(src),
        Instr::Print { v, .. } => rw(v),
        Instr::PrintPrec { v, prec } => {
            rw(v);
            rw(prec);
        }
        Instr::Ret(Some(v)) => rw(v),
        Instr::Br { cond, .. } => rw(cond),
        Instr::Assert { cond, .. } => rw(cond),
        Instr::Fill { len, value, .. } => {
            rw(len);
            rw(value);
        }
        Instr::BoundsCheck { idx, .. } => rw(idx),
        Instr::Alloc { len, .. } => rw(len),
        Instr::IndexLoad { idx, .. } => rw(idx),
        Instr::IndexStore { idx, src, .. } => {
            rw(idx);
            rw(src);
        }
        Instr::FieldStore { src, .. } => rw(src),
        Instr::IndexFieldLoad { idx, .. } => rw(idx),
        Instr::IndexFieldStore { idx, src, .. } => {
            rw(idx);
            rw(src);
        }
        Instr::AddrVar { .. }
        | Instr::CopySlots { .. }
        | Instr::StoreVec { .. }
        | Instr::LoadPtr { .. }
        | Instr::StorePtr { .. } => {}
        Instr::CopySlotsTo { src, .. } => rw(src),
        Instr::LoadCtx { .. } | Instr::FuncAddr { .. } => {}
        Instr::IndexStoreBoolAtomic { idx, src, .. } => {
            rw(idx);
            rw(src);
        }
        Instr::FastDivPow2 { dst, a, .. } => {
            rw(dst);
            rw(a);
        }
        Instr::Aos2Step { dst, tmp, idx, k, .. } => {
            rw(dst);
            rw(tmp);
            rw(idx);
            rw(k);
        }
        Instr::SaxpyI32 { c_idx, b_idx, count, av, start, limit, .. } => {
            rw(c_idx);
            rw(b_idx);
            rw(count);
            rw(av);
            rw(start);
            rw(limit);
        }
        Instr::Select { cond, a, b, .. } => {
            rw(cond);
            rw(a);
            rw(b);
        }
        Instr::StructInit { fields, .. } => {
            for (_, _, v) in fields {
                rw(v);
            }
        }
        Instr::VecReserve { need, .. } => rw(need),
        Instr::VecPushScalar { src, .. } => rw(src),
        Instr::Argv { idx, .. } => rw(idx),
        Instr::InputRead { buf, len, .. } => {
            rw(buf);
            rw(len);
        }
        Instr::PrintBytes { buf, len, .. } => {
            rw(buf);
            rw(len);
        }
        Instr::Call { args, .. } => {
            for a in args {
                rw(a);
            }
        }
        _ => {}
    }
}

/// Remove `i32` truncations that are provably redundant: if interval
/// analysis shows the operand already fits in i32, the wrap cannot happen.
/// Loop induction variables with a constant zero start, unit step and a
/// constant upper bound contribute intervals; arithmetic over them is exact.
fn collect_loops(
    f: &IrFunc,
    defs: &HashMap<Value, (DefKind, usize)>,
    var_consts: &HashMap<usize, i64>,
) -> Vec<(usize, std::collections::HashSet<usize>, Option<(usize, i64)>)> {
    let n = f.blocks.len();
    let mut out = Vec::new();
    for h in 0..n {
        let Some(Instr::Br { then_bb, else_bb, .. }) = f.blocks[h].instrs.last() else {
            continue;
        };
        let mut region = std::collections::HashSet::new();
        let mut stack = vec![*then_bb];
        let mut reaches = false;
        while let Some(bb) = stack.pop() {
            if bb == h {
                reaches = true;
                continue;
            }
            if bb == *else_bb || !region.insert(bb) {
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
        let mut iv_c: Option<(usize, i64)> = None;
        for ins in &f.blocks[h].instrs {
            if let Instr::Bin { op, l, r, .. } = ins {
                if !matches!(op, Op::Lt | Op::Le) {
                    continue;
                }
                if let Some((DefKind::LoadVar(v), hb)) = defs.get(l).copied() {
                    if hb != h {
                        continue;
                    }
                    let bound = match defs.get(r).copied() {
                        Some((DefKind::Const(c), rb)) if rb == h => Some(c),
                        Some((DefKind::LoadVar(w), rb)) if rb == h => var_consts.get(&w).copied(),
                        _ => None,
                    };
                    if let Some(c) = bound {
                        // Exclusive upper bound of the induction variable's
                        // value: `i < c` leaves c, `i <= c` leaves c + 1.
                        let upper = if matches!(op, Op::Le) {
                            c.checked_add(1)
                        } else {
                            Some(c)
                        };
                        if let Some(upper) = upper {
                            if upper > 0 {
                                iv_c = Some((v, upper));
                            }
                        }
                    }
                }
            }
        }
        out.push((h, region, iv_c));
    }
    out
}

fn value_ranges(
    f: &IrFunc,
    defs: &HashMap<Value, (DefKind, usize)>,
    var_range: &HashMap<usize, (i64, i64)>,
    array_range: &HashMap<usize, (i64, i64)>,
    with_loads: bool,
) -> HashMap<Value, (i64, i64)> {
    let mut range: HashMap<Value, (i64, i64)> = HashMap::new();
    for _ in 0..4 {
        let mut changed = false;
        for b in &f.blocks {
            for ins in &b.instrs {
                let r = match ins {
                    Instr::Const { v, .. } => Some((*v, *v)),
                    Instr::LoadVar { var, .. } => var_range.get(var).copied(),
                    Instr::Bin { op, l, r, .. } => {
                        match (range.get(l).copied(), range.get(r).copied()) {
                            (Some((a0, a1)), Some((b0, b1))) => match op {
                                Op::Add => a0.checked_add(b0).zip(a1.checked_add(b1)),
                                Op::Sub => a0.checked_sub(b1).zip(a1.checked_sub(b0)),
                                Op::Mul => {
                                    let mut lo = i64::MAX;
                                    let mut hi = i64::MIN;
                                    let mut good = true;
                                    for (x, y) in [(a0, b0), (a0, b1), (a1, b0), (a1, b1)] {
                                        match x.checked_mul(y) {
                                            Some(p) => {
                                                lo = lo.min(p);
                                                hi = hi.max(p);
                                            }
                                            None => good = false,
                                        }
                                    }
                                    if good { Some((lo, hi)) } else { None }
                                }
                                Op::Shr | Op::Shl => None,
                                Op::Div | Op::Rem => {
                                    if b0 == b1 && b0 > 0 {
                                        if *op == Op::Div && a0 >= 0 {
                                            Some((a0 / b0, a1 / b0))
                                        } else if *op == Op::Rem {
                                            if a0 >= 0 {
                                                Some((0, b0 - 1))
                                            } else if a1 <= 0 {
                                                Some((-(b0 - 1), 0))
                                            } else {
                                                Some((-(b0 - 1), b0 - 1))
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            },
                            _ => None,
                        }
                    }
                    Instr::Un { neg: true, a, .. } => range.get(a).and_then(|(lo, hi)| {
                        lo.checked_neg()
                            .zip(hi.checked_neg())
                            .map(|(x, y)| (y.min(x), y.max(x)))
                    }),
                    Instr::Trunc { ty, a, .. } => range.get(a).map(|(lo, hi)| match ty {
                        Ty::I32 => {
                            if *lo >= i32::MIN as i64 && *hi <= i32::MAX as i64 {
                                (*lo, *hi)
                            } else {
                                (i32::MIN as i64, i32::MAX as i64)
                            }
                        }
                        Ty::U8 => (0, 255),
                        Ty::Bool => (0, 1),
                        _ => (*lo, *hi),
                    }),
                    Instr::IndexLoad { ptr_var, elem, .. } if with_loads => {
                        if matches!(elem, Ty::I32 | Ty::U8 | Ty::I64) {
                            array_range.get(ptr_var).copied()
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some((lo, hi)) = r {
                    if let Some(dst) = dst_of(ins) {
                        if range.get(&dst) != Some(&(lo, hi)) {
                            range.insert(dst, (lo, hi));
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    range
}

fn value_uses_var(f: &IrFunc, defs: &HashMap<Value, (DefKind, usize)>, var: usize, v: Value, depth: usize) -> bool {
    if depth > 24 {
        return true;
    }
    let Some((kind, _)) = defs.get(&v).copied() else { return false };
    if let DefKind::LoadVar(x) = kind {
        return x == var;
    }
    let Some(ins) = f.blocks.iter().flat_map(|b| b.instrs.iter()).find(|i| dst_of(i) == Some(v)) else {
        return false;
    };
    let mut uses = Vec::new();
    let mut defs_v = Vec::new();
    instr_uses_defs(ins, &mut uses, &mut defs_v);
    uses.into_iter().any(|u| value_uses_var(f, defs, var, u, depth + 1))
}

fn remove_redundant_truncations(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let mut defs: HashMap<Value, (DefKind, usize)> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for ins in &b.instrs {
                if let Some((v, k)) = def_kind(ins) {
                    defs.insert(v, (k, bi));
                }
            }
        }
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
        let has_vec = f.blocks.iter().any(|b| {
            b.instrs.iter().any(|i| {
                matches!(
                    i,
                    Instr::VecNew { .. }
                        | Instr::VecReserve { .. }
                        | Instr::VecPushScalar { .. }
                        | Instr::VecPushStruct { .. }
                )
            })
        });
        let loops = collect_loops(f, &defs, &var_consts);
        // Base variable ranges: canonical loop induction variables.
        let mut var_range: HashMap<usize, (i64, i64)> = HashMap::new();
        for (_, region, ivc) in &loops {
            let Some((iv, upper)) = *ivc else { continue };
            let mut ok = true;
            let mut saw_zero = false;
            for bb in region {
                for ins in &f.blocks[*bb].instrs {
                    let Instr::StoreVar { var, src } = ins else { continue };
                    if *var != iv {
                        continue;
                    }
                    if let Some((DefKind::Const(0), _)) = defs.get(src).copied() {
                        saw_zero = true;
                        continue;
                    }
                    let mut inc = false;
                    for b2 in &f.blocks {
                        for i2 in &b2.instrs {
                            if let Instr::Bin { dst, op: Op::Add, l, r } = i2 {
                                if dst == src {
                                    let is_iv = |v: &Value| {
                                        matches!(defs.get(v), Some((DefKind::LoadVar(x), _)) if *x == iv)
                                    };
                                    let one = |v: &Value| {
                                        matches!(defs.get(v), Some((DefKind::Const(1), _)))
                                    };
                                    inc = (is_iv(l) && one(r)) || (is_iv(r) && one(l));
                                }
                            }
                        }
                    }
                    if !inc {
                        ok = false;
                    }
                }
            }
            if ok && saw_zero {
                var_range.insert(iv, (0, upper));
            }
        }
        // Arrays: conservative element ranges from store-only loops.
        let mut array_range: HashMap<usize, (i64, i64)> = HashMap::new();
        if !has_vec {
            let mut ptr_vars: std::collections::HashSet<usize> = Default::default();
            let mut passed: std::collections::HashSet<usize> = Default::default();
            for b in &f.blocks {
                for ins in &b.instrs {
                    match ins {
                        Instr::IndexLoad { ptr_var, .. } | Instr::IndexStore { ptr_var, .. } => {
                            ptr_vars.insert(*ptr_var);
                        }
                        Instr::Call { args, .. } => {
                            for a in args {
                                if let Some((DefKind::LoadVar(v), _)) = defs.get(a).copied() {
                                    passed.insert(v);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut mixed: std::collections::HashSet<usize> = Default::default();
            for (_, region, _) in &loops {
                let mut stored: std::collections::HashSet<usize> = Default::default();
                let mut loaded: std::collections::HashSet<usize> = Default::default();
                for bb in region {
                    for ins in &f.blocks[*bb].instrs {
                        match ins {
                            Instr::IndexStore { ptr_var, .. } => {
                                stored.insert(*ptr_var);
                            }
                            Instr::IndexLoad { ptr_var, .. } => {
                                loaded.insert(*ptr_var);
                            }
                            _ => {}
                        }
                    }
                }
                for p in stored.intersection(&loaded) {
                    mixed.insert(*p);
                }
            }
            let base = value_ranges(f, &defs, &var_range, &array_range, false);
            for p in ptr_vars {
                if passed.contains(&p) || mixed.contains(&p) {
                    continue;
                }
                let mut lo = 0i64.min(0);
                let mut hi = 0i64;
                let mut ok = true;
                for b in &f.blocks {
                    for ins in &b.instrs {
                        match ins {
                            Instr::IndexStore { ptr_var, src, elem, .. } if *ptr_var == p => {
                                if matches!(elem, Ty::F64 | Ty::Bool | Ty::Struct(_)) {
                                    ok = false;
                                } else if let Some((a, b2)) = base.get(src).copied() {
                                    lo = lo.min(a);
                                    hi = hi.max(b2);
                                } else {
                                    ok = false;
                                }
                            }
                            Instr::Fill { ptr_var, value, elem, .. } if *ptr_var == p => {
                                if matches!(elem, Ty::F64 | Ty::Bool | Ty::Struct(_)) {
                                    ok = false;
                                } else if let Some((a, b2)) = base.get(value).copied() {
                                    lo = lo.min(a);
                                    hi = hi.max(b2);
                                } else {
                                    ok = false;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if ok {
                    array_range.insert(p, (lo, hi));
                }
            }
        }
        // Accumulator variables: v += delta with a counted loop bound.
        let with_arrays = value_ranges(f, &defs, &var_range, &array_range, true);
        for (h, region, ivc) in &loops {
            let Some((iv, upper)) = *ivc else { continue };
            let nested = region.iter().any(|bb| {
                block_successors(&f.blocks[*bb].instrs)
                    .iter()
                    .any(|s| *s != *h && region.contains(s) && *s <= *bb)
            });
            if nested {
                continue;
            }
            let mut vars: std::collections::HashSet<usize> = Default::default();
            for bb in region {
                for ins in &f.blocks[*bb].instrs {
                    if let Instr::StoreVar { var, .. } = ins {
                        if *var != iv {
                            vars.insert(*var);
                        }
                    }
                }
            }
            for v in vars {
                let mut store_in = None;
                let mut count = 0;
                for bb in region {
                    for ins in &f.blocks[*bb].instrs {
                        if let Instr::StoreVar { var, src } = ins {
                            if *var == v {
                                count += 1;
                                store_in = Some(*src);
                            }
                        }
                    }
                }
                if count != 1 {
                    continue;
                }
                let Some(src) = store_in else { continue };
                let Some(Instr::Bin { op: Op::Add, l, r, .. }) = f
                    .blocks
                    .iter()
                    .flat_map(|b| b.instrs.iter())
                    .find(|i| dst_of(i) == Some(src))
                else {
                    continue;
                };
                let is_v = |x: &Value| matches!(defs.get(x), Some((DefKind::LoadVar(w), _)) if *w == v);
                let delta = if is_v(l) && !is_v(r) {
                    *r
                } else if is_v(r) && !is_v(l) {
                    *l
                } else {
                    continue;
                };
                if value_uses_var(f, &defs, v, delta, 0) {
                    continue;
                }
                let Some((dlo, dhi)) = with_arrays.get(&delta).copied() else { continue };
                let iterations = upper.max(0);
                let lo_inc = iterations.checked_mul(dlo.min(0));
                let hi_inc = iterations.checked_mul(dhi.max(0));
                let (Some(lo_inc), Some(hi_inc)) = (lo_inc, hi_inc) else { continue };
                let mut base_ok = true;
                let mut found_any = false;
                let mut c0 = 0i64;
                for (bi, b) in f.blocks.iter().enumerate() {
                    if region.contains(&bi) {
                        continue;
                    }
                    for ins in &b.instrs {
                        if let Instr::StoreVar { var, src } = ins {
                            if *var == v {
                                match defs.get(src).copied() {
                                    Some((DefKind::Const(c), _)) => {
                                        found_any = true;
                                        c0 = c;
                                    }
                                    _ => base_ok = false,
                                }
                            }
                        }
                    }
                }
                if !base_ok || !found_any {
                    continue;
                }
                let lo = c0.checked_add(lo_inc);
                let hi = c0.checked_add(hi_inc);
                if let (Some(lo), Some(hi)) = (lo, hi) {
                    var_range.insert(v, (lo.min(hi), lo.max(hi)));
                }
            }
        }
        // Final ranges and truncation removal.
        let range = value_ranges(f, &defs, &var_range, &array_range, true);
        let mut subst: HashMap<Value, Value> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::Trunc { dst, ty: Ty::I32, a } = ins {
                    if let Some((lo, hi)) = range.get(a) {
                        if *lo >= i32::MIN as i64 && *hi <= i32::MAX as i64 {
                            subst.insert(*dst, *a);
                        }
                    }
                }
            }
        }
        if subst.is_empty() {
            continue;
        }
        let resolve = |v: Value, subst: &HashMap<Value, Value>| -> Value {
            let mut cur = v;
            for _ in 0..64 {
                match subst.get(&cur) {
                    Some(next) => cur = *next,
                    None => break,
                }
            }
            cur
        };
        for b in f.blocks.iter_mut() {
            let old = std::mem::take(&mut b.instrs);
            let mut new_instrs = Vec::new();
            for mut ins in old {
                if let Instr::Trunc { dst, ty: Ty::I32, .. } = &ins {
                    if subst.contains_key(dst) {
                        continue;
                    }
                }
                rewrite_operands(&mut ins, &|v| resolve(v, &subst));
                new_instrs.push(ins);
            }
            b.instrs = new_instrs;
        }
    }
}

fn reg32_name(reg: &str) -> &'static str {
    match reg {
        "%rbx" => "%ebx",
        "%r12" => "%r12d",
        "%r13" => "%r13d",
        "%r14" => "%r14d",
        "%r15" => "%r15d",
        "%rdx" => "%edx",
        "%rsi" => "%esi",
        "%rdi" => "%edi",
        "%r8" => "%r8d",
        "%r9" => "%r9d",
        "%r11" => "%r11d",
        _ => "%eax",
    }
}

/// Store i32 variables as four bytes and drop the explicit wrap when the
/// truncated value is only consumed by such a store: the low 32 bits of the
/// 64-bit computation are exactly the wrapped i32 value, and every load
/// sign-extends the four-byte slot again.
fn compress_i32_variable_traffic(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let mut uses: HashMap<Value, usize> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                let mut u = Vec::new();
                let mut d = Vec::new();
                instr_uses_defs(ins, &mut u, &mut d);
                for v in u {
                    *uses.entry(v).or_insert(0) += 1;
                }
            }
        }
        let mut subst: HashMap<Value, Value> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::Trunc { dst, ty: Ty::I32, a } = ins {
                    if uses.get(dst).copied().unwrap_or(0) != 1 {
                        continue;
                    }
                    let mut is_i32_store = false;
                    for b2 in &f.blocks {
                        for i2 in &b2.instrs {
                            if let Instr::StoreVar { var, src } = i2 {
                                if src == dst
                                    && f.var_ty.get(*var).copied() == Some(Ty::I32)
                                {
                                    is_i32_store = true;
                                }
                            }
                        }
                    }
                    if is_i32_store {
                        subst.insert(*dst, *a);
                    }
                }
            }
        }
        if subst.is_empty() {
            continue;
        }
        let resolve = |v: Value, subst: &HashMap<Value, Value>| -> Value {
            let mut cur = v;
            loop {
                match subst.get(&cur) {
                    Some(n) => cur = *n,
                    None => break,
                }
            }
            cur
        };
        for b in f.blocks.iter_mut() {
            let old = std::mem::take(&mut b.instrs);
            let mut out = Vec::new();
            for mut ins in old {
                if let Instr::Trunc { dst, ty: Ty::I32, .. } = &ins {
                    if subst.contains_key(dst) {
                        continue;
                    }
                }
                rewrite_operands(&mut ins, &|v| resolve(v, &subst));
                out.push(ins);
            }
            b.instrs = out;
        }
    }
}

/// Replace signed `/` and `%` by a positive power-of-two constant with the
/// classic sign-correction + shift sequence, avoiding a hardware division
/// (hash and tree probes use `x % (1 << k)` inside their hottest loops).
/// Replace a small array whose every access has a constant index with
/// individual scalar slots. Combined with full unrolling this turns the
/// n-body four-body arrays into straight-line scalar code with no pointer,
/// bounds or scaling work.
fn scalarize_const_arrays(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let mut vals: HashMap<Value, i64> = HashMap::new();
        let mut vc: HashMap<usize, i64> = HashMap::new();
        for _ in 0..6 {
            vals = compute_value_consts(f, &vc);
            let next = compute_var_consts(f, &vals);
            if next == vc {
                break;
            }
            vc = next;
        }
        let mut candidates: Vec<(usize, Ty, i64)> = Vec::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::Alloc { ptr_var, elem, len, arena, .. } = ins {
                    if *arena {
                        continue;
                    }
                    if let Some(n) = vals.get(len).copied() {
                        if n > 0 && n <= 16 && !matches!(elem, Ty::Struct(_)) {
                            candidates.push((*ptr_var, *elem, n));
                        }
                    }
                }
            }
        }
        for (p, elem, n) in candidates {
            let mut ok = true;
            let mut fill: Option<Value> = None;
            for b in &f.blocks {
                for ins in &b.instrs {
                    match ins {
                        Instr::Alloc { ptr_var, .. } if *ptr_var == p => {}
                        Instr::Fill { ptr_var, value, .. } if *ptr_var == p => fill = Some(*value),
                        Instr::IndexLoad { ptr_var, idx, .. } if *ptr_var == p => {
                            match vals.get(idx).copied() {
                                Some(k) if k >= 0 && k < n => {}
                                _ => ok = false,
                            }
                        }
                        Instr::IndexStore { ptr_var, idx, .. } if *ptr_var == p => {
                            match vals.get(idx).copied() {
                                Some(k) if k >= 0 && k < n => {}
                                _ => ok = false,
                            }
                        }
                        Instr::BoundsCheck { .. } => {}
                        other => {
                            if instr_mentions_var(other, p) {
                                ok = false;
                            }
                        }
                    }
                }
            }
            if !ok {
                continue;
            }
            let base = f.nvars;
            f.nvars += n as usize;
            for _ in 0..n {
                f.var_ty.push(elem);
            }
            let mut zero: Option<Value> = None;
            for b in f.blocks.iter_mut() {
                let old = std::mem::take(&mut b.instrs);
                let mut out = Vec::with_capacity(old.len() + n as usize);
                for ins in old {
                    match ins {
                        Instr::Alloc { ptr_var, .. } if ptr_var == p => {
                            if fill.is_none() {
                                if zero.is_none() {
                                    let z = Value(f.nvalues as u32);
                                    f.nvalues += 1;
                                    f.vty.push(elem);
                                    out.push(Instr::Const { dst: z, v: 0 });
                                    zero = Some(z);
                                }
                                let src = zero.unwrap();
                                for k in 0..n {
                                    out.push(Instr::StoreVar { var: base + k as usize, src });
                                }
                            }
                        }
                        Instr::Fill { ptr_var, value, .. } if ptr_var == p => {
                            for k in 0..n {
                                out.push(Instr::StoreVar { var: base + k as usize, src: value });
                            }
                        }
                        Instr::IndexLoad { dst, ptr_var, idx, .. } if ptr_var == p => {
                            let k = vals[&idx] as usize;
                            out.push(Instr::LoadVar { dst, var: base + k });
                        }
                        Instr::IndexStore { ptr_var, idx, src, .. } if ptr_var == p => {
                            let k = vals[&idx] as usize;
                            out.push(Instr::StoreVar { var: base + k, src });
                        }
                        other => out.push(other),
                    }
                }
                b.instrs = out;
            }
        }
    }
}

fn instr_mentions_var(ins: &Instr, p: usize) -> bool {
    match ins {
        Instr::LoadVar { var, .. } | Instr::StoreVar { var, .. } => *var == p,
        Instr::Alloc { ptr_var, .. }
        | Instr::Fill { ptr_var, .. }
        | Instr::IndexLoad { ptr_var, .. }
        | Instr::IndexStore { ptr_var, .. }
        | Instr::IndexFieldLoad { ptr_var, .. }
        | Instr::IndexFieldStore { ptr_var, .. }
        | Instr::VecNew { ptr_var, .. }
        | Instr::VecReserve { ptr_var, .. }
        | Instr::VecPushScalar { ptr_var, .. }
        | Instr::VecPushStruct { ptr_var, .. }
        | Instr::Aos2Step { ptr_var, .. }
        | Instr::CopySlots { ptr_var, .. } => *ptr_var == p,
        Instr::AddrVar { var_index, .. }
        | Instr::StructInit { var_index, .. }
        | Instr::FieldLoad { var_index, .. }
        | Instr::FieldStore { var_index, .. } => *var_index == p,
        Instr::CopySlotsTo { dst_ptr_var, .. } | Instr::StorePtr { dst_ptr_var, .. } => {
            *dst_ptr_var == p
        }
        Instr::StoreVec { src_ptr_var, ptr_var, len_var, cap_var } => {
            *src_ptr_var == p || *ptr_var == p || *len_var == p || *cap_var == p
        }
        Instr::VecPushStruct { tmp_var, .. } => *tmp_var == p,
        _ => false,
    }
}

fn strength_reduce_pow2(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
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
        let mut defs: HashMap<Value, DefKind> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Some((v, k)) = def_kind(ins) {
                    defs.insert(v, k);
                }
            }
        }
        for b in f.blocks.iter_mut() {
            for ins in b.instrs.iter_mut() {
                let Instr::Bin { dst, op, l, r } = ins else { continue };
                if !matches!(op, Op::Div | Op::Rem)
                    || matches!(f.vty.get(dst.0 as usize), Some(Ty::F64))
                    || matches!(f.vty.get(l.0 as usize), Some(Ty::F64))
                {
                    continue;
                }
                let mut div = None;
                if let Some(c) = vals.get(r) {
                    div = Some(*c);
                } else if let Some(DefKind::LoadVar(var)) = defs.get(r) {
                    if let Some(c) = var_consts.get(var) {
                        div = Some(*c);
                    }
                }
                let Some(d) = div else { continue };
                if d <= 0 || d.count_ones() != 1 || d > (1i64 << 62) {
                    continue;
                }
                let rem = matches!(op, Op::Rem);
                *ins = Instr::FastDivPow2 {
                    dst: *dst,
                    a: *l,
                    k: d.trailing_zeros(),
                    d,
                    rem,
                };
            }
        }
    }
}

/// Fuse the `x += vx; y += vy; vx *= k; vy *= k` shape on an array of
/// structs into two SSE2 pair operations. Only accesses whose bounds check was
/// already proven (`line == 0`) are vectorised, and every intermediate value
/// must have exactly one use, so the transformation is a pure replacement.
fn vectorize_aos_pairs(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let mut uses: HashMap<Value, usize> = HashMap::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                let mut u = Vec::new();
                let mut d = Vec::new();
                instr_uses_defs(ins, &mut u, &mut d);
                for v in u {
                    *uses.entry(v).or_insert(0) += 1;
                }
            }
        }
        for b in f.blocks.iter_mut() {
            let old = std::mem::take(&mut b.instrs);
            let mut out = Vec::with_capacity(old.len());
            let mut i = 0usize;
            while i < old.len() {
                let trial = try_aos_window(&old, i, &uses);
                if let Some((len, fused)) = trial {
                    // The window includes the scalar constant that the pair
                    // multiply broadcasts; keep its definition.
                    if let Some(c @ Instr::Const { .. }) = old.get(i + 9) {
                        out.push(c.clone());
                    }
                    out.push(fused);
                    i += len;
                } else {
                    out.push(old[i].clone());
                    i += 1;
                }
            }
            b.instrs = out;
        }
    }
}

fn try_aos_window(
    ins: &[Instr],
    at: usize,
    uses: &HashMap<Value, usize>,
) -> Option<(usize, Instr)> {
    if at + 15 > ins.len() {
        return None;
    }
    let load = |x: &Instr| -> Option<(Value, usize, Value, u64, u64, LenRef, usize)> {
        match x {
            Instr::IndexFieldLoad { dst, ptr_var, idx, size, off, ty: Ty::F64, len, line } => {
                Some((*dst, *ptr_var, *idx, *size, *off, *len, *line))
            }
            _ => None,
        }
    };
    let store = |x: &Instr| -> Option<(usize, Value, u64, LenRef, usize)> {
        match x {
            Instr::IndexFieldStore { ptr_var, off, ty: Ty::F64, src, len, line, .. } => {
                Some((*ptr_var, *src, *off, *len, *line))
            }
            _ => None,
        }
    };
    let (a0, p0, idx0, size0, off0, len0, line0) = load(&ins[at])?;
    let (a1, p1, idx1, size1, off1, len1, line1) = load(&ins[at + 1])?;
    if p0 != p1 || idx0 != idx1 || size0 != size1 || off1 != off0 + 16 || len0 != len1 {
        return None;
    }
    let (r0, l0, r0b) = match &ins[at + 2] {
        Instr::FBin { dst, op: Op::Add, l, r } => (*dst, *l, *r),
        _ => return None,
    };
    if l0 != a0 || r0b != a1 {
        return None;
    }
    let (p2, s0, so0, stlen0, sl0) = store(&ins[at + 3])?;
    if p2 != p0 || s0 != r0 || so0 != off0 || stlen0 != len0 {
        return None;
    }
    let (a2, p3, idx2, size2, off2, len2, line2) = load(&ins[at + 4])?;
    let (a3, p4, idx3, size3, off3, len3, line3) = load(&ins[at + 5])?;
    if p3 != p0 || p4 != p0 || idx2 != idx0 || idx3 != idx0 || size2 != size0 || size3 != size0 || len2 != len0 || len3 != len0 {
        return None;
    }
    if off2 != off0 + 8 || off3 != off0 + 24 {
        return None;
    }
    let (r1, l1, r1b) = match &ins[at + 6] {
        Instr::FBin { dst, op: Op::Add, l, r } => (*dst, *l, *r),
        _ => return None,
    };
    if l1 != a2 || r1b != a3 {
        return None;
    }
    let (p5, s1, so1, stlen1, sl1) = store(&ins[at + 7])?;
    if p5 != p0 || s1 != r1 || so1 != off0 + 8 || stlen1 != len0 {
        return None;
    }
    let (a4, p6, idx4, size4, off4, len4, line4) = load(&ins[at + 8])?;
    if p6 != p0 || idx4 != idx0 || size4 != size0 || off4 != off0 + 16 || len4 != len0 {
        return None;
    }
    let k = match &ins[at + 9] {
        Instr::Const { v, .. } => *v,
        _ => return None,
    };
    let (r2, fa, fk) = match &ins[at + 10] {
        Instr::FBin { dst, op: Op::Mul, l, r } => (*dst, *l, *r),
        _ => return None,
    };
    if fa != a4 {
        return None;
    }
    let (p7, s2, so2, stlen2, sl2) = store(&ins[at + 11])?;
    if p7 != p0 || s2 != r2 || so2 != off0 + 16 || stlen2 != len0 {
        return None;
    }
    let (a5, p8, idx5, size5, off5, len5, line5) = load(&ins[at + 12])?;
    if p8 != p0 || idx5 != idx0 || size5 != size0 || off5 != off0 + 24 || len5 != len0 {
        return None;
    }
    let (r3, fb, fk2) = match &ins[at + 13] {
        Instr::FBin { dst, op: Op::Mul, l, r } => (*dst, *l, *r),
        _ => return None,
    };
    if fb != a5 || fk2 != fk {
        return None;
    }
    let (p9, s3, so3, stlen3, sl3) = store(&ins[at + 14])?;
    if p9 != p0 || s3 != r3 || so3 != off0 + 24 || stlen3 != len0 {
        return None;
    }
    for v in [a0, a1, r0, a2, a3, r1, a4, r2, a5, r3] {
        if uses.get(&v).copied().unwrap_or(0) != 1 {
            return None;
        }
    }
    let fused = Instr::Aos2Step {
        dst: r0,
        tmp: r1,
        ptr_var: p0,
        idx: idx0,
        size: size0,
        off: off0,
        k: fk,
        len: len0,
        line: [line0, line1, line2, line3, line4, line5]
            .into_iter()
            .find(|l| *l != 0)
            .unwrap_or(0),
    };
    let _ = (r2, r3);
    Some((15, fused))

}

/// When every call site passes the same constant length for an array
/// parameter (recursive calls that pass the parameter through are allowed),
/// replace loads of that length parameter with the constant. This lets the
/// bounds prover see the real length even though the ABI still passes it.
fn propagate_const_array_lens(prog: &mut IrProgram) {
    let mut consts: HashMap<(usize, usize), i64> = HashMap::new();
    for _ in 0..6 {
        let mut maps: Vec<HashMap<Value, i64>> = Vec::new();
        for f in &prog.funcs {
            let mut vc: HashMap<usize, i64> = HashMap::new();
            let mut vals: HashMap<Value, i64> = HashMap::new();
            for _ in 0..6 {
                vals = compute_value_consts(f, &vc);
                let next = compute_var_consts(f, &vals);
                if next == vc {
                    break;
                }
                vc = next;
            }
            maps.push(vals);
        }
        let mut changed = false;
        for (fi, f) in prog.funcs.iter().enumerate() {
            for (arg_pos, var) in &f.array_len_args {
                if consts.contains_key(&(fi, *var)) {
                    continue;
                }
                let mut found: Option<i64> = None;
                let mut ok = true;
                let mut calls = 0usize;
                for (ci, caller) in prog.funcs.iter().enumerate() {
                    let caller_vals = &maps[ci];
                    for b in &caller.blocks {
                        for ins in &b.instrs {
                            let Instr::Call { name, args, .. } = ins else { continue };
                            if name != &f.name {
                                continue;
                            }
                            let Some(a) = args.get(*arg_pos).copied() else {
                                ok = false;
                                continue;
                            };
                            calls += 1;
                            if let Some(c) = caller_vals.get(&a) {
                                match found {
                                    None => found = Some(*c),
                                    Some(prev) if prev == *c => {}
                                    _ => ok = false,
                                }
                            } else if ci == fi {
                                // recursive pass-through of one of our own
                                // length parameters
                                let is_own_len = matches!(
                                    caller.blocks.iter().flat_map(|b| b.instrs.iter()).find(|i| dst_of(i) == Some(a)),
                                    Some(Instr::LoadVar { var: lv, .. }) if lv == var
                                );
                                if !is_own_len {
                                    ok = false;
                                }
                            } else {
                                ok = false;
                            }
                        }
                    }
                }
                if ok && calls > 0 {
                    if let Some(c) = found {
                        consts.insert((fi, *var), c);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for (&(fi, var), &c) in consts.iter() {
        let f = &mut prog.funcs[fi];
        if f.blocks.iter().any(|b| b.instrs.iter().any(|i| matches!(i, Instr::StoreVar { var: v, .. } if *v == var))) {
            continue;
        }
        for b in f.blocks.iter_mut() {
            for ins in b.instrs.iter_mut() {
                if let Instr::LoadVar { dst, var: v } = ins {
                    if *v == var {
                        *ins = Instr::Const { dst: *dst, v: c };
                    }
                }
            }
        }
    }
    // Drop the now-unused length arguments from signatures and call sites.
    for fi in 0..prog.funcs.len() {
        let name = prog.funcs[fi].name.clone();
        let mut removals: Vec<(usize, usize, i64)> = Vec::new();
        for (pos, var) in prog.funcs[fi].array_len_args.clone() {
            let Some(c) = consts.get(&(fi, var)).copied() else { continue };
            if prog.funcs[fi].blocks.iter().any(|b| {
                b.instrs.iter().any(|i| match i {
                    Instr::LoadVar { var: v, .. } | Instr::StoreVar { var: v, .. } => *v == var,
                    _ => false,
                })
            }) {
                continue;
            }
            removals.push((pos, var, c));
        }
        removals.sort_by(|a, b| b.0.cmp(&a.0));
        for (pos, var, c) in removals {
            replace_len_refs(&mut prog.funcs[fi], var, c);
            for caller in prog.funcs.iter_mut() {
                for b in caller.blocks.iter_mut() {
                    for ins in b.instrs.iter_mut() {
                        if let Instr::Call { name: cn, args, .. } = ins {
                            if cn == &name && pos < args.len() {
                                args.remove(pos);
                            }
                        }
                    }
                }
            }
            let callee = &mut prog.funcs[fi];
            if pos < callee.param_vars.len() {
                callee.param_vars.remove(pos);
            }
            callee.nparams = callee.nparams.saturating_sub(1);
            let mut newlists = Vec::new();
            for (p, v) in callee.array_len_args.drain(..) {
                if v == var {
                    continue;
                }
                newlists.push(if p > pos { (p - 1, v) } else { (p, v) });
            }
            callee.array_len_args = newlists;
        }
    }
}

fn replace_len_refs(f: &mut IrFunc, var: usize, c: i64) {
    for b in f.blocks.iter_mut() {
        for ins in b.instrs.iter_mut() {
            let len = match ins {
                Instr::BoundsCheck { len, .. }
                | Instr::IndexFieldLoad { len, .. }
                | Instr::IndexFieldStore { len, .. }
                | Instr::SaxpyI32 {
                    c_len: len,
                    b_len: _,
                    ..
                } => len,
                Instr::SaxpyI32 { b_len: len, .. } => len,
                _ => continue,
            };
            if matches!(*len, LenRef::Var(v) if v == var) {
                *len = LenRef::Const(c);
            }
        }
    }
}

fn resolve_constant_lengths(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
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
        if var_consts.is_empty() {
            continue;
        }
        for b in f.blocks.iter_mut() {
            for ins in b.instrs.iter_mut() {
                let len = match ins {
                    Instr::BoundsCheck { len, .. }
                    | Instr::IndexFieldLoad { len, .. }
                    | Instr::IndexFieldStore { len, .. } => len,
                    _ => continue,
                };
                if let LenRef::Var(v) = len {
                    if let Some(c) = var_consts.get(v) {
                        *len = LenRef::Const(*c);
                    }
                }
            }
        }
    }
}

/// Mark i32 arithmetic that feeds only array indices and emit it with 32-bit
/// operations, removing the intermediate sign-extension instructions.
fn narrow_index_chains(prog: &mut IrProgram) {
    for f in &mut prog.funcs {
        let mut defs: HashMap<Value, usize> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for ins in &b.instrs {
                if let Some(d) = dst_of(ins) {
                    defs.insert(d, bi);
                }
            }
        }
        // value -> uses: (block, consumer dst, is_fast_index)
        let mut uses: HashMap<Value, Vec<(usize, Option<Value>, bool)>> = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            let fast = !slow_block(&b.instrs);
            for ins in &b.instrs {
                let mut u = Vec::new();
                let mut dv = Vec::new();
                instr_uses_defs(ins, &mut u, &mut dv);
                let idx_slot = match ins {
                    Instr::BoundsCheck { idx, .. }
                    | Instr::IndexLoad { idx, .. }
                    | Instr::IndexStore { idx, .. }
                    | Instr::IndexFieldLoad { idx, .. }
                    | Instr::IndexFieldStore { idx, .. } => Some(*idx),
                    _ => None,
                };
                let consumer = dst_of(ins);
                for v in u {
                    let is_idx = Some(v) == idx_slot && fast;
                    uses.entry(v).or_default().push((bi, consumer, is_idx));
                }
            }
        }
        let mut marked: std::collections::HashSet<Value> = Default::default();
        let mut subst: HashMap<Value, Value> = HashMap::new();
        let mut stack: Vec<Value> = Vec::new();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::BoundsCheck { idx, .. }
                | Instr::IndexLoad { idx, .. }
                | Instr::IndexStore { idx, .. }
                | Instr::IndexFieldLoad { idx, .. }
                | Instr::IndexFieldStore { idx, .. } = ins
                {
                    if f.vty.get(idx.0 as usize) == Some(&Ty::I32) {
                        stack.push(*idx);
                    }
                }
            }
        }
        while let Some(v) = stack.pop() {
            if marked.contains(&v) {
                continue;
            }
            let Some(&bi) = defs.get(&v) else { continue };
            if slow_block(&f.blocks[bi].instrs) {
                continue;
            }
            let ok = uses.get(&v).map_or(false, |list| {
                list.iter().all(|(ub, cons, is_idx)| {
                    if *is_idx {
                        return true;
                    }
                    match cons {
                        Some(c) => marked.contains(c),
                        None => false,
                    }
                })
            });
            if !ok {
                continue;
            }
            let def = f.blocks[bi]
                .instrs
                .iter()
                .find(|i| dst_of(i) == Some(v))
                .cloned();
            match def {
                Some(Instr::Trunc { ty: Ty::I32, a, .. }) => {
                    if f.vty.get(a.0 as usize) != Some(&Ty::I32) {
                        continue;
                    }
                    marked.insert(v);
                    subst.insert(v, a);
                    stack.push(a);
                }
                Some(Instr::Bin { op, l, r, .. })
                    if matches!(op, Op::Add | Op::Sub | Op::Mul | Op::And | Op::Or)
                        && f.vty.get(v.0 as usize) == Some(&Ty::I32) =>
                {
                    marked.insert(v);
                    stack.push(l);
                    stack.push(r);
                }
                Some(Instr::Un { neg: true, a, .. })
                    if f.vty.get(v.0 as usize) == Some(&Ty::I32) =>
                {
                    marked.insert(v);
                    stack.push(a);
                }
                _ => {}
            }
        }
        if marked.is_empty() {
            continue;
        }
        f.narrow_index = marked.iter().copied().collect();
        if subst.is_empty() {
            continue;
        }
        // The substitution graph is acyclic: a Trunc destination is allocated
        // after its operand, so following the chain strictly decreases the id.
        let resolve = |v: Value, subst: &HashMap<Value, Value>| -> Value {
            let mut cur = v;
            loop {
                match subst.get(&cur) {
                    Some(n) => cur = *n,
                    None => break,
                }
            }
            cur
        };
        for b in f.blocks.iter_mut() {
            let old = std::mem::take(&mut b.instrs);
            let mut out = Vec::new();
            for mut ins in old {
                if let Instr::Trunc { dst, ty: Ty::I32, .. } = &ins {
                    if subst.contains_key(dst) {
                        continue;
                    }
                }
                rewrite_operands(&mut ins, &|v| resolve(v, &subst));
                out.push(ins);
            }
            b.instrs = out;
        }
    }
}

pub fn optimize(prog: &mut IrProgram) {
    propagate_const_array_lens(prog);
    resolve_constant_lengths(prog);
    remove_redundant_bounds(prog);
    // scalarize_const_arrays is disabled: an audit showed it could remove
    // arrays still referenced by SaxpyI32. The pass stays for a later fix.
    strength_reduce_pow2(prog);
    remove_redundant_truncations(prog);
    narrow_index_chains(prog);
    cse_blocks(prog);
    compress_i32_variable_traffic(prog);
    vectorize_aos_pairs(prog);
}

pub fn emit(prog: &IrProgram) -> String {
    let mut out = String::new();
    out.push_str(".text\n");
    out.push_str(".section .bss\n.align 8\n.Lbt_buf: .zero 512\n.text\n");
    out.push_str(".Lfmt_ll: .asciz \"%lld\\n\"\n");
    out.push_str(".Lfmt_s: .asciz \"%s\\n\"\n");
    out.push_str(".Ltrue: .asciz \"true\"\n");
    out.push_str(".Lfalse: .asciz \"false\"\n");
    out.push_str(".Ldiv0: .asciz \"line 0: division by zero\"\n");
    out.push_str(".Lfmt_panic: .asciz \"%s\\n\"\n");
    out.push_str(".Lom: .asciz \"out of memory\"\n");
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
                    Instr::Aos2Step { line, .. } => Some(*line),
                    Instr::SaxpyI32 { line, .. } => Some(*line),
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
    out.push_str(".cfi_startproc\n.Lir_panic:\n");
    out.push_str("    push %rbp\n");
    out.push_str("    .cfi_def_cfa_offset 16\n    .cfi_offset %rbp, -16\n");
    out.push_str("    mov %rsp, %rbp\n    .cfi_def_cfa_register %rbp\n");
    out.push_str("    mov %rdi, %rdx\n");
    out.push_str("    mov stderr@GOTPCREL(%rip), %rax\n    mov (%rax), %rdi\n");
    out.push_str("    lea .Lfmt_panic(%rip), %rsi\n    xor %eax, %eax\n");
    out.push_str("    call fprintf@PLT\n");
    out.push_str("    lea .Lbt_buf(%rip), %rdi\n    mov $64, %esi\n    call backtrace@PLT\n");
    out.push_str("    lea .Lbt_buf(%rip), %rdi\n    mov %eax, %esi\n    mov $2, %edx\n");
    out.push_str("    call backtrace_symbols_fd@PLT\n");
    out.push_str("    mov $1, %edi\n    call exit@PLT\n.cfi_endproc\n");
    for f in &prog.funcs {
        // Variables used most often get a callee-saved register for the whole
        // function; the emitter never touches these registers elsewhere.
        let mut var_counts: HashMap<usize, u32> = HashMap::new();
        // Approximate loop depth per block from backward edges, so variables
        // used in inner loops outrank equally-used outer ones.
        let mut depth = vec![0u32; f.blocks.len()];
        for (bi, b) in f.blocks.iter().enumerate() {
            for succ in block_successors(&b.instrs) {
                if succ <= bi && succ < f.blocks.len() {
                    for d in depth.iter_mut().take(bi + 1).skip(succ) {
                        *d += 1;
                    }
                }
            }
        }
        for (bi, b) in f.blocks.iter().enumerate() {
            let w = 1 + depth[bi] * depth[bi];
            for ins in &b.instrs {
                match ins {
                    Instr::LoadVar { var, .. } => {
                        *var_counts.entry(*var).or_insert(0) += w;
                    }
                    Instr::StoreVar { var, .. } => {
                        *var_counts.entry(*var).or_insert(0) += w;
                    }
                    Instr::Alloc { ptr_var, .. }
                    | Instr::Fill { ptr_var, .. }
                    | Instr::IndexLoad { ptr_var, .. }
                    | Instr::IndexStore { ptr_var, .. }
                    | Instr::IndexFieldLoad { ptr_var, .. }
                    | Instr::IndexFieldStore { ptr_var, .. } => {
                        *var_counts.entry(*ptr_var).or_insert(0) += 3 * w;
                    }
                    _ => {}
                }
            }
        }
        let mut addr_taken: std::collections::HashSet<usize> = Default::default();
        for b in &f.blocks {
            for ins in &b.instrs {
                if let Instr::AddrVar { var_index, slots, .. } = ins {
                    for k in 0..*slots {
                        addr_taken.insert(*var_index + k);
                    }
                }
            }
        }
        let mut ranked: Vec<(usize, u32)> = var_counts.into_iter().collect();
        ranked.retain(|(v, _)| !addr_taken.contains(v));
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let chosen: Vec<(usize, &'static str)> = ranked
            .into_iter()
            .take(5)
            .enumerate()
            .map(|(i, (var, _))| (var, ["%rbx", "%r12", "%r13", "%r14", "%r15"][i]))
            .collect();
        let var_regs: HashMap<usize, &'static str> = chosen.iter().copied().collect();
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

        out.push_str(".p2align 4\n");
        out.push_str(".cfi_startproc\n");
        out.push_str(&format!(".globl {}\n.type {}, @function\n{}:\n", f.name, f.name, f.name));
        out.push_str("    push %rbp\n");
        out.push_str("    .cfi_def_cfa_offset 16\n    .cfi_offset %rbp, -16\n");
        out.push_str("    mov %rsp, %rbp\n");
        out.push_str("    .cfi_def_cfa_register %rbp\n");
        let frame = 8 * (f.nvars + f.nvalues);
        let frame = (frame + 15) & !15;
        // Push all five callee-saved registers: the first `chosen.len()` hold
        // promoted variables and the rest become extra temporaries.
        let pad = 8usize;
        if frame + pad > 0 {
            out.push_str(&format!("    sub ${}, %rsp\n", frame + pad));
        }
        for i in 0..5 {
            out.push_str(&format!("    push {}\n", ["%rbx", "%r12", "%r13", "%r14", "%r15"][i]));
        }
        let regs = ["%rdi", "%rsi", "%rdx", "%rcx", "%r8", "%r9"];
        if let (Some(ac), Some(av)) = (f.argc_var, f.argv_var) {
            out.push_str("    movslq %edi, %rax\n");
            out.push_str(&format!("    mov %rax, -{}(%rbp)\n", varoff(ac)));
            out.push_str(&format!("    mov %rsi, -{}(%rbp)\n", varoff(av)));
        }
        for i in 0..f.nparams {
            let var = f.param_vars.get(i).copied().unwrap_or(i);
            let src = if i < 6 {
                regs[i].to_string()
            } else {
                out.push_str(&format!(
                    "    mov {}(%rbp), %rax\n",
                    16 + 8 * (i - 6)
                ));
                "%rax".to_string()
            };
            if let Some(reg) = var_regs.get(&var) {
                out.push_str(&format!("    mov {}, {}\n", src, reg));
            } else {
                out.push_str(&format!("    mov {}, -{}(%rbp)\n", src, varoff(var)));
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
        let mut loop_heads = vec![false; f.blocks.len()];
        for (bi, b) in f.blocks.iter().enumerate() {
            for succ in block_successors(&b.instrs) {
                if succ <= bi && succ < f.blocks.len() {
                    loop_heads[succ] = true;
                }
            }
        }
        for (bb, b) in f.blocks.iter().enumerate() {
            if bb == 0 || loop_heads[bb] {
                out.push_str(".p2align 6\n");
            } else {
                out.push_str(".p2align 4\n");
            }
            out.push_str(&format!("{}:\n", lblock(bb)));
            if !slow_block(&b.instrs) {
                let block_bool = b.instrs.iter().any(|i| {
                    matches!(
                        i,
                        Instr::IndexLoad { elem: Ty::Bool, .. }
                            | Instr::IndexStore { elem: Ty::Bool, .. }
                    )
                });
                let block_div = b
                    .instrs
                    .iter()
                    .any(|i| matches!(i, Instr::Bin { op: Op::Div | Op::Rem, .. }));
                let mut block_pool: Vec<usize> = Vec::new();
                for i in 0..6 {
                    if block_bool && (i == 1 || i == 2) {
                        continue;
                    }
                    if block_div && i == 0 {
                        continue;
                    }
                    block_pool.push(i);
                }
                for i in 6 + chosen.len()..11 {
                    block_pool.push(i);
                }
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
                    &block_pool,
                    &last_use,
                    &fn_vals,
                    &deferred,
                );
                continue;
            }
            for ins in &b.instrs {
                let ld = |v: Value| format!("    mov -{}(%rbp), %rax\n", voff(f, v));
                match ins {
                    Instr::FastDivPow2 { dst, a, k, d, rem } => {
                        out.push_str(&ld(*a));
                        out.push_str("    mov %rax, %rcx\n");
                        out.push_str("    test %rax, %rax\n    js 2f\n");
                        if *rem {
                            out.push_str(&format!("    movabs ${}, %rdx\n    and %rdx, %rcx\n", d - 1));
                            out.push_str("    jmp 3f\n2:\n");
                            out.push_str(&format!("    movabs ${}, %rdx\n    and %rdx, %rcx\n", d - 1));
                            out.push_str("    test %rcx, %rcx\n    jz 3f\n");
                            out.push_str(&format!("    movabs ${}, %rdx\n    sub %rdx, %rcx\n", d));
                        } else {
                            out.push_str(&format!("    sar ${}, %rcx\n", k));
                            out.push_str("    jmp 3f\n2:\n");
                            out.push_str(&format!("    movabs ${}, %rdx\n    add %rdx, %rcx\n", d - 1));
                            out.push_str(&format!("    sar ${}, %rcx\n", k));
                        }
                        out.push_str("3:\n");
                        out.push_str(&format!("    mov %rcx, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::IndexStoreBoolAtomic { ptr_var, idx, src } => {
                        out.push_str(&format!("    mov -{}(%rbp), %rax\n", voff(f, *idx)));
                        out.push_str(&format!("    mov -{}(%rbp), %rdx\n", voff(f, *src)));
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str("    mov %rax, %r11\n    shr $6, %r11\n");
                        out.push_str("    and $63, %rax\n    mov %rax, %rcx\n");
                        out.push_str("    mov $1, %rsi\n    shl %cl, %rsi\n");
                        out.push_str("    test %rdx, %rdx\n    je 2f\n");
                        out.push_str("    lock or %rsi, (%r10,%r11,8)\n    jmp 3f\n");
                        out.push_str("2:\n    not %rsi\n    lock and %rsi, (%r10,%r11,8)\n3:\n");
                    }
                    Instr::SaxpyI32 { c_ptr, c_len, b_ptr, b_len, c_idx, b_idx, count, av, start, limit, line } => {
                        if f.vty.get(c_idx.0 as usize) == Some(&Ty::I32) {
                            out.push_str(&format!("    movslq -{}(%rbp), %r10\n", voff(f, *c_idx)));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", voff(f, *c_idx)));
                        }
                        if f.vty.get(b_idx.0 as usize) == Some(&Ty::I32) {
                            out.push_str(&format!("    movslq -{}(%rbp), %r11\n", voff(f, *b_idx)));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r11\n", voff(f, *b_idx)));
                        }
                        out.push_str(&format!("    movslq -{}(%rbp), %rcx\n", voff(f, *count)));
                        out.push_str(&format!("    movslq -{}(%rbp), %rdi\n", voff(f, *start)));
                        out.push_str(&format!("    movslq -{}(%rbp), %rsi\n", voff(f, *limit)));
                        out.push_str("    cmp %rsi, %rdi\n    jge 7f\n");
                        out.push_str("    test %rcx, %rcx\n    jz 9f\n7:\n");
                        out.push_str("    test %r10, %r10\n    js 9f\n");
                        out.push_str("    test %r11, %r11\n    js 9f\n");
                        out.push_str("    test %rcx, %rcx\n    js 9f\n");
                        for (len, idx) in [(c_len, "%r10"), (b_len, "%r11")] {
                            match len {
                                LenRef::Const(n) => {
                                    if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                                        out.push_str(&format!("    mov ${}, %rax\n", n));
                                    } else {
                                        out.push_str(&format!("    movabs ${}, %rax\n", n));
                                    }
                                }
                                LenRef::Var(v) => {
                                    if let Some(reg) = var_regs.get(v) {
                                        out.push_str(&format!("    mov {}, %rax\n", reg));
                                    } else {
                                        out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*v)));
                                    }
                                }
                            }
                            out.push_str(&format!("    lea ({},%rcx), %rdx\n    cmp %rax, %rdx\n    ja 9f\n", idx));
                        }
                        if let Some(reg) = var_regs.get(c_ptr) {
                            out.push_str(&format!("    mov {}, %r8\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r8\n", varoff(*c_ptr)));
                        }
                        if let Some(reg) = var_regs.get(b_ptr) {
                            out.push_str(&format!("    mov {}, %r9\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r9\n", varoff(*b_ptr)));
                        }
                        out.push_str("    lea (%r8,%r10,4), %r8\n    lea (%r9,%r11,4), %r9\n");
                        out.push_str(&format!("    movslq -{}(%rbp), %rax\n", voff(f, *av)));
                        out.push_str("    movd %eax, %xmm0\n    pshufd $0, %xmm0, %xmm0\n");
                        out.push_str("    xor %edx, %edx\n");
                        out.push_str("1:\n    lea 4(%rdx), %rax\n    cmp %rcx, %rax\n    ja 2f\n");
                        out.push_str("    movdqu (%r8,%rdx,4), %xmm1\n    movdqu (%r9,%rdx,4), %xmm2\n");
                        out.push_str("    movdqa %xmm2, %xmm3\n    psrlq $32, %xmm3\n");
                        out.push_str("    pmuludq %xmm0, %xmm2\n    pmuludq %xmm0, %xmm3\n");
                        out.push_str("    pshufd $8, %xmm2, %xmm2\n    pshufd $8, %xmm3, %xmm3\n");
                        out.push_str("    punpckldq %xmm3, %xmm2\n    paddd %xmm2, %xmm1\n");
                        out.push_str("    movdqu %xmm1, (%r8,%rdx,4)\n    add $4, %rdx\n    jmp 1b\n");
                        out.push_str("2:\n    movd %xmm0, %esi\n    cmp %rcx, %rdx\n    jae 8f\n");
                        out.push_str("3:\n    movslq (%r9,%rdx,4), %rax\n    imul %esi, %eax\n");
                        out.push_str("    add %eax, (%r8,%rdx,4)\n    inc %rdx\n    cmp %rcx, %rdx\n    jb 3b\n");
                        out.push_str("    jmp 8f\n9:\n");
                        out.push_str(&format!("    lea .Lmsg{}(%rip), %rdi\n    call .Lir_panic\n8:\n", line));
                    }
                    Instr::Aos2Step { ptr_var, idx, size, off, k, len, line, .. } => {
                        out.push_str(&ld(*idx));
                        out.push_str("    mov %rax, %r11\n");
                        if *line != 0 {
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
                        }
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str(&scale_insn(*size, "%r11"));
                        out.push_str(&format!("    movupd {}(%r10,%r11), %xmm0\n", off));
                        out.push_str(&format!("    movupd {}(%r10,%r11), %xmm1\n", off + 16));
                        out.push_str("    addpd %xmm1, %xmm0\n");
                        out.push_str(&format!("    movupd %xmm0, {}(%r10,%r11)\n", off));
                        out.push_str(&format!("    movsd -{}(%rbp), %xmm2\n", voff(f, *k)));
                        out.push_str("    unpcklpd %xmm2, %xmm2\n");
                        out.push_str("    mulpd %xmm2, %xmm1\n");
                        out.push_str(&format!("    movupd %xmm1, {}(%r10,%r11)\n", off + 16));
                    }
                    Instr::Select { dst, cond, a, b, float } => {
                        if *float {
                            out.push_str(&format!("    movsd -{}(%rbp), %xmm0\n", voff(f, *a)));
                            out.push_str(&format!("    movsd -{}(%rbp), %xmm1\n", voff(f, *b)));
                            out.push_str(&format!("    mov -{}(%rbp), %rcx\n", voff(f, *cond)));
                            out.push_str("    cmp $0, %rcx\n");
                            out.push_str("    jne 1f\n    movapd %xmm1, %xmm0\n1:\n");
                            out.push_str(&format!("    movsd %xmm0, -{}(%rbp)\n", voff(f, *dst)));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", voff(f, *a)));
                            out.push_str(&format!("    mov -{}(%rbp), %rdx\n", voff(f, *b)));
                            out.push_str(&format!("    mov -{}(%rbp), %rcx\n", voff(f, *cond)));
                            out.push_str("    cmp $0, %rcx\n");
                            out.push_str("    cmovne %rax, %rdx\n");
                            out.push_str(&format!("    mov %rdx, -{}(%rbp)\n", voff(f, *dst)));
                        }
                    }
                    Instr::LoadCtx { dst_var, ctx_var, off } => {
                        if let Some(reg) = var_regs.get(ctx_var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*ctx_var)));
                        }
                        out.push_str(&format!("    mov {}(%rax), %rdx\n", off));
                        if let Some(reg) = var_regs.get(dst_var) {
                            out.push_str(&format!("    mov %rdx, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rdx, -{}(%rbp)\n", varoff(*dst_var)));
                        }
                    }
                    Instr::FuncAddr { dst, name } => {
                        out.push_str(&format!("    lea {}(%rip), %rax\n", name));
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::CopySlotsTo { dst_ptr_var, src, slots } => {
                        if let Some(reg) = var_regs.get(dst_ptr_var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*dst_ptr_var)));
                        }
                        out.push_str(&format!("    mov -{}(%rbp), %rdx\n", voff(f, *src)));
                        for i in 0..*slots {
                            out.push_str(&format!("    mov -{}(%rdx), %rcx\n", i * 8));
                            out.push_str(&format!("    mov %rcx, -{}(%rax)\n", i * 8));
                        }
                    }
                    Instr::Const { dst, v } => {
                        if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                            out.push_str(&format!("    mov ${}, %rax\n", v));
                        } else {
                            out.push_str(&format!("    movabs ${}, %rax\n", v));
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::LoadVar { dst, var } => {
                        let i32v = f.var_ty.get(*var).copied() == Some(Ty::I32);
                        if let Some(reg) = var_regs.get(var) {
                            if i32v {
                                out.push_str(&format!(
                                    "    movslq {}, %rax\n",
                                    reg32_name(reg)
                                ));
                            } else {
                                out.push_str(&format!("    mov {}, %rax\n", reg));
                            }
                        } else if i32v {
                            out.push_str(&format!(
                                "    movslq -{}(%rbp), %rax\n",
                                varoff(*var)
                            ));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*var)));
                        }
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::StoreVar { var, src } => {
                        out.push_str(&ld(*src));
                        let i32v = f.var_ty.get(*var).copied() == Some(Ty::I32);
                        if let Some(reg) = var_regs.get(var) {
                            if i32v {
                                out.push_str(&format!(
                                    "    movl %eax, {}\n",
                                    reg32_name(reg)
                                ));
                            } else {
                                out.push_str(&format!("    mov %rax, {}\n", reg));
                            }
                        } else if i32v {
                            out.push_str(&format!("    movl %eax, -{}(%rbp)\n", varoff(*var)));
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
                            Op::Shr | Op::Shl => {
                                if matches!(op, Op::Shr) {
                                    out.push_str("    sar %cl, %rax\n");
                                } else {
                                    out.push_str("    shl %cl, %rax\n");
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
                        out.push_str(&format!("    lea .Lstr_{}_{}(%rip), %rax\n", f.name, id));
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::VecNew { ptr_var, len_var, cap_var, esz } => {
                        out.push_str(&format!("    mov ${}, %rdi\n", esz * 8));
                        out.push_str("    call malloc@PLT\n");
                        out.push_str("    test %rax, %rax\n    jne 8f\n");
                        out.push_str("    lea .Lom(%rip), %rdi\n    call .Lir_panic\n8:\n");
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
                        out.push_str(&format!(
                            "    mov ${}, %rax\n    cmp %rax, %r11\n    ja 9f\n",
                            i64::MAX / (*esz as i64).max(1)
                        ));
                        out.push_str("    mov %r11, %rsi\n");
                        out.push_str(&scale_insn(*esz, "%rsi"));
                        out.push_str("    push %r11\n    sub $8, %rsp\n");
                        out.push_str("    call realloc@PLT\n");
                        out.push_str("    add $8, %rsp\n    pop %r11\n");
                        out.push_str("    test %rax, %rax\n    jne 8f\n9:\n");
                        out.push_str("    lea .Lom(%rip), %rdi\n    call .Lir_panic\n8:\n");
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
                        out.push_str(&format!(
                            "    mov ${}, %rax\n    cmp %rax, %rcx\n    ja 9f\n",
                            i64::MAX / (2 * (*esz as i64).max(1))
                        ));
                        out.push_str("    lea (%rcx,%rcx), %rsi\n");
                        out.push_str(&scale_insn(*esz, "%rsi"));
                        out.push_str("    push %r8\n    push %r11\n");
                        out.push_str("    call realloc@PLT\n");
                        out.push_str("    pop %r11\n    pop %r8\n");
                        out.push_str("    test %rax, %rax\n    jne 8f\n9:\n");
                        out.push_str("    lea .Lom(%rip), %rdi\n    call .Lir_panic\n8:\n");
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
                        out.push_str(&scale_insn(*esz, "%r11"));
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
                        out.push_str(&format!(
                            "    mov ${}, %rax\n    cmp %rax, %rcx\n    ja 9f\n",
                            i64::MAX / (2 * (*esz as i64).max(1))
                        ));
                        out.push_str("    lea (%rcx,%rcx), %rsi\n");
                        out.push_str(&scale_insn(*esz, "%rsi"));
                        out.push_str("    push %r11\n    sub $8, %rsp\n");
                        out.push_str("    call realloc@PLT\n");
                        out.push_str("    add $8, %rsp\n    pop %r11\n");
                        out.push_str("    test %rax, %rax\n    jne 8f\n9:\n");
                        out.push_str("    lea .Lom(%rip), %rdi\n    call .Lir_panic\n8:\n");
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
                        out.push_str(&scale_insn(*esz, "%r11"));
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
                    Instr::ArenaBegin { dst } => {
                        out.push_str("    sub $8, %rsp\n    call .Lmf_arena_begin\n    add $8, %rsp\n");
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::ArenaEnd { mark } => {
                        out.push_str(&ld(*mark));
                        out.push_str("    mov %rax, %rdi\n    call .Lmf_arena_end\n");
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
                    Instr::AddrVar { dst, var_index, slots } => {
                        let _ = slots;
                        out.push_str(&format!("    lea -{}(%rbp), %rax\n", varoff(*var_index)));
                        out.push_str(&format!("    mov %rax, -{}(%rbp)\n", voff(f, *dst)));
                    }
                    Instr::CopySlots { ptr_var, var_index, slots } => {
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*ptr_var)));
                        }
                        for i in 0..*slots {
                            out.push_str(&format!("    mov -{}(%rax), %rdx\n", i * 8));
                            out.push_str(&format!(
                                "    mov %rdx, -{}(%rbp)\n",
                                varoff(var_index + i)
                            ));
                        }
                    }
                    Instr::LoadPtr { dst_var, src_var } => {
                        if let Some(reg) = var_regs.get(src_var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*src_var)));
                        }
                        out.push_str("    mov (%rax), %rdx\n");
                        if let Some(reg) = var_regs.get(dst_var) {
                            out.push_str(&format!("    mov %rdx, {}\n", reg));
                        } else {
                            out.push_str(&format!("    mov %rdx, -{}(%rbp)\n", varoff(*dst_var)));
                        }
                    }
                    Instr::StorePtr { dst_ptr_var, src_var } => {
                        if let Some(reg) = var_regs.get(dst_ptr_var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*dst_ptr_var)));
                        }
                        if let Some(reg) = var_regs.get(src_var) {
                            out.push_str(&format!("    mov {}, %rdx\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rdx\n", varoff(*src_var)));
                        }
                        out.push_str("    mov %rdx, (%rax)\n");
                    }
                    Instr::StoreVec { src_ptr_var, ptr_var, len_var, cap_var } => {
                        if let Some(reg) = var_regs.get(src_ptr_var) {
                            out.push_str(&format!("    mov {}, %rax\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %rax\n", varoff(*src_ptr_var)));
                        }
                        for (i, v) in [ptr_var, len_var, cap_var].iter().enumerate() {
                            if let Some(reg) = var_regs.get(v) {
                                out.push_str(&format!("    mov {}, %rdx\n", reg));
                            } else {
                                out.push_str(&format!("    mov -{}(%rbp), %rdx\n", varoff(**v)));
                            }
                            out.push_str(&format!("    mov %rdx, -{}(%rax)\n", i * 8));
                        }
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
                        if *line != 0 {
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
                        }
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str(&scale_insn(*size, "%r11"));
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
                        if *line != 0 {
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
                        }
                        if let Some(reg) = var_regs.get(ptr_var) {
                            out.push_str(&format!("    mov {}, %r10\n", reg));
                        } else {
                            out.push_str(&format!("    mov -{}(%rbp), %r10\n", varoff(*ptr_var)));
                        }
                        out.push_str(&scale_insn(*size, "%r11"));
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
                    Instr::Alloc { ptr_var, elem, esz, len, arena } => {
                        out.push_str(&ld(*len));
                        if *elem == Ty::Bool {
                            out.push_str("    add $63, %rax\n    shr $6, %rax\n");
                        }
                        let esz = if *elem == Ty::Bool { 8 } else { *esz };
                        out.push_str(&format!("    mov ${}, %rcx\n    cmp %rcx, %rax\n    ja 8f\n", i64::MAX / (esz as i64).max(1)));
                        out.push_str(&format!("    imul ${}, %rax\n", esz));
                        if *arena {
                            out.push_str("    mov %rax, %rdi\n    call .Lmf_arena_alloc\n");
                        } else {
                            out.push_str("    mov %rax, %rdi\n    mov $1, %esi\n");
                            out.push_str("    call calloc@PLT\n");
                            out.push_str("    test %rax, %rax\n    jne 7f\n8:\n    lea .Lom(%rip), %rdi\n    call .Lir_panic\n7:\n");
                        }
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
                            out.push_str(&format!("    lea (%r10,%r11,{}), %r9\n", esz));
                            if esz == 4 {
                                out.push_str("    movd %r8d, %xmm0\n    pshufd $0, %xmm0, %xmm0\n");
                                out.push_str("1:\n    lea 16(%r10), %rcx\n");
                                out.push_str("    cmp %r9, %rcx\n    ja 3f\n");
                                out.push_str("    movups %xmm0, (%r10)\n    mov %rcx, %r10\n    jmp 1b\n");
                            } else if esz == 8 {
                                out.push_str("    movq %r8, %xmm0\n    punpcklqdq %xmm0, %xmm0\n");
                                out.push_str("1:\n    lea 32(%r10), %rcx\n");
                                out.push_str("    cmp %r9, %rcx\n    ja 3f\n");
                                out.push_str("    movups %xmm0, (%r10)\n    movups %xmm0, 16(%r10)\n");
                                out.push_str("    mov %rcx, %r10\n    jmp 1b\n");
                            } else {
                                out.push_str("1:\n");
                                out.push_str(&format!("    lea {}(%r10), %rcx\n", esz * 4));
                                out.push_str("    cmp %r9, %rcx\n    ja 3f\n");
                                for k in 0..4 {
                                    out.push_str(&format!("    mov %r8b, {}(%r10)\n", k));
                                }
                                out.push_str("    mov %rcx, %r10\n    jmp 1b\n");
                            }
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
        out.push_str("    .cfi_def_cfa_register %rbp\n");
        for i in (0..5).rev() {
            out.push_str(&format!("    pop {}\n", ["%rbx", "%r12", "%r13", "%r14", "%r15"][i]));
        }
        out.push_str("    leave\n");
        out.push_str("    .cfi_def_cfa %rsp, 8\n");
        out.push_str("    ret\n");
        out.push_str(".cfi_endproc\n");
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
    // Parallel-for runtime: fixed fan-out of four pthreads (main thread takes
    // the first chunk). Each worker calls the generated body function with
    // (ctx, lo, hi).
    out.push_str(".p2align 4\n.Lpar_worker:\n");
    out.push_str("    mov %rdi, %rax\n    mov 0(%rax), %r11\n    mov 8(%rax), %rdi\n");
    out.push_str("    mov 16(%rax), %rsi\n    mov 24(%rax), %rdx\n");
    out.push_str("    sub $8, %rsp\n    call *%r11\n    add $8, %rsp\n    ret\n");
    out.push_str(".p2align 4\n.Lpar_for:\n");
    out.push_str("    push %rbp\n    mov %rsp, %rbp\n    push %rbx\n    push %r12\n");
    out.push_str("    push %r13\n    push %r14\n    push %r15\n    sub $664, %rsp\n");
    out.push_str("    mov %rdi, %r12\n    mov %rsi, %r13\n    mov %rdx, %r14\n    mov %rcx, %r15\n");
    out.push_str("    sub %rdx, %rcx\n    jle .Lpar_done\n    mov %rcx, 640(%rsp)\n");
    out.push_str("    mov $84, %edi\n    call sysconf@PLT\n    cmp $1, %rax\n    jge 0f\n    mov $4, %rax\n0:\n");
    out.push_str("    cmp $8, %rax\n    jle 0f\n    mov $8, %rax\n0:\n");
    out.push_str("    mov 640(%rsp), %rdx\n    cmp %rdx, %rax\n    jle 1f\n    mov %rdx, %rax\n1:\n");
    out.push_str("    mov %rax, %rdx\n");
    out.push_str("1:\n    mov %rdx, 648(%rsp)\n    lea -1(%rdx), %rax\n");
    out.push_str("    add 640(%rsp), %rax\n    xor %edx, %edx\n    divq 648(%rsp)\n");
    out.push_str("    mov %rax, 656(%rsp)\n    xor %ebx, %ebx\n");
    out.push_str(".Lpar_loop:\n    cmp 648(%rsp), %rbx\n    jge .Lpar_join_setup\n");
    out.push_str("    mov %rbx, %rax\n    imulq 656(%rsp), %rax\n    add %r14, %rax\n");
    out.push_str("    mov 656(%rsp), %rdx\n    add %rax, %rdx\n    cmp %r15, %rdx\n");
    out.push_str("    jbe 2f\n    mov %r15, %rdx\n2:\n    cmp %rdx, %rax\n    jge .Lpar_next\n");
    out.push_str("    mov %rbx, %r10\n    shl $5, %r10\n    lea (%rsp,%r10), %r11\n");
    out.push_str("    mov %r12, 0(%r11)\n    mov %r13, 8(%r11)\n    mov %rax, 16(%r11)\n");
    out.push_str("    mov %rdx, 24(%r11)\n    movq $0, 512(%rsp,%rbx,8)\n");
    out.push_str("    lea 512(%rsp,%rbx,8), %rdi\n    xor %esi, %esi\n");
    out.push_str("    lea .Lpar_worker(%rip), %rdx\n    mov %r11, %rcx\n    call pthread_create@PLT\n");
    out.push_str("    test %eax, %eax\n    je .Lpar_next\n");
    out.push_str("    mov 0(%r11), %r10\n    mov 8(%r11), %rdi\n    mov 16(%r11), %rsi\n");
    out.push_str("    mov 24(%r11), %rdx\n    push %r11\n    call *%r10\n    pop %r11\n");
    out.push_str(".Lpar_next:\n    inc %rbx\n    jmp .Lpar_loop\n");
    out.push_str(".Lpar_join_setup:\n    xor %ebx, %ebx\n.Lpar_join:\n");
    out.push_str("    cmp 648(%rsp), %rbx\n    jge .Lpar_done\n");
    out.push_str("    mov 512(%rsp,%rbx,8), %rdi\n    test %rdi, %rdi\n    je 4f\n    xor %esi, %esi\n    call pthread_join@PLT\n4:\n");
    out.push_str("    inc %rbx\n    jmp .Lpar_join\n.Lpar_done:\n");
    out.push_str("    add $664, %rsp\n    pop %r15\n    pop %r14\n    pop %r13\n    pop %r12\n");
    out.push_str("    pop %rbx\n    leave\n    ret\n");
    // Network helpers mirroring the native backend's inline sequences.
    out.push_str(".p2align 4\n.Lmf_flush:\n    xor %edi, %edi\n    jmp fflush@PLT\n");
    out.push_str(".p2align 4\n.Lmf_env:\n    jmp getenv@PLT\n");
    out.push_str(".p2align 4\n.Lnet_read:\n    jmp read@PLT\n");
    out.push_str(".p2align 4\n.Lnet_read_at:\n    add %rdx, %rsi\n    mov %rcx, %rdx\n    jmp read@PLT\n");
    out.push_str(".p2align 4\n.Lnet_write_at:\n    add %rdx, %rsi\n    mov %rcx, %rdx\n    jmp write@PLT\n");
    out.push_str(".p2align 4\n.Lnet_write:\n    jmp write@PLT\n");
    out.push_str(".p2align 4\n.Lnet_close:\n    jmp close@PLT\n");
    out.push_str(".p2align 4\n.Lnet_accept:\n    xor %esi, %esi\n    xor %edx, %edx\n    jmp accept@PLT\n");
    out.push_str(".p2align 4\n.Lnet_port:\n    push %rbx\n    sub $32, %rsp\n");
    out.push_str("    mov %edi, %ebx\n    movl $16, 24(%rsp)\n");
    out.push_str("    mov %ebx, %edi\n    lea (%rsp), %rsi\n    lea 24(%rsp), %rdx\n    call getsockname@PLT\n");
    out.push_str("    movzwl 2(%rsp), %eax\n    rol $8, %ax\n    movzwl %ax, %eax\n");
    out.push_str("    add $32, %rsp\n    pop %rbx\n    ret\n");
    out.push_str(".p2align 4\n.Lnet_listen:\n    push %rbx\n    push %r12\n    sub $8, %rsp\n");
    out.push_str("    mov %edi, %r12d\n    mov $2, %edi\n    mov $1, %esi\n    xor %edx, %edx\n    call socket@PLT\n");
    out.push_str("    test %eax, %eax\n    js 2f\n    mov %eax, %ebx\n    sub $16, %rsp\n");
    out.push_str("    movw $2, (%rsp)\n    mov %r12d, %eax\n    rol $8, %ax\n    movw %ax, 2(%rsp)\n");
    out.push_str("    movl $0, 4(%rsp)\n    movq $0, 8(%rsp)\n");
    out.push_str("    mov %ebx, %edi\n    mov %rsp, %rsi\n    mov $16, %edx\n    call bind@PLT\n");
    out.push_str("    test %eax, %eax\n    js 3f\n    mov %ebx, %edi\n    mov $16, %esi\n    call listen@PLT\n");
    out.push_str("    test %eax, %eax\n    js 3f\n    mov %ebx, %eax\n    add $16, %rsp\n");
    out.push_str("1:\n    add $8, %rsp\n    pop %r12\n    pop %rbx\n    ret\n");
    out.push_str("3:\n    mov %ebx, %edi\n    call close@PLT\n    mov $-1, %eax\n    add $16, %rsp\n    jmp 1b\n");
    out.push_str("2:\n    mov $-1, %eax\n    jmp 1b\n");
    out.push_str(".p2align 4\n.Lnet_connect:\n    push %rbx\n    push %r12\n    push %r13\n    sub $16, %rsp\n");
    out.push_str("    mov %esi, %r12d\n    call inet_addr@PLT\n    mov %eax, 4(%rsp)\n");
    out.push_str("    movw $2, (%rsp)\n    mov %r12d, %eax\n    rol $8, %ax\n    movw %ax, 2(%rsp)\n");
    out.push_str("    movq $0, 8(%rsp)\n    mov $2, %edi\n    mov $1, %esi\n    xor %edx, %edx\n    call socket@PLT\n");
    out.push_str("    test %eax, %eax\n    js 2f\n    mov %eax, %ebx\n    mov %ebx, %edi\n    mov %rsp, %rsi\n");
    out.push_str("    mov $16, %edx\n    call connect@PLT\n    test %eax, %eax\n    js 3f\n");
    out.push_str("    mov %ebx, %eax\n1:\n    add $16, %rsp\n    pop %r13\n    pop %r12\n    pop %rbx\n    ret\n");
    out.push_str("3:\n    mov %ebx, %edi\n    call close@PLT\n2:\n    mov $-1, %eax\n    jmp 1b\n");
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
    // Arena blocks: allocations are anonymous mmap regions (lazily zeroed
    // pages) that are munmap'd when the block ends, so sequential regions
    // never accumulate and untouched pages stay unreachable.
    out.push_str(".section .bss\n.p2align 3\n.Larena_ptrs: .zero 65536\n.Larena_count: .zero 8\n.text\n");
    out.push_str(".p2align 4\n.Lmf_arena_begin:\n    mov .Larena_count(%rip), %rax\n    ret\n");
    out.push_str(".p2align 4\n.Lmf_arena_alloc:\n    push %rbx\n    push %r12\n");
    out.push_str("    mov %rdi, %rbx\n    xor %edi, %edi\n    mov %rbx, %rsi\n");
    out.push_str("    mov $3, %edx\n    mov $0x22, %ecx\n    mov $-1, %r8\n    xor %r9d, %r9d\n");
    out.push_str("    call mmap@PLT\n    cmp $-1, %rax\n    je 9f\n");
    out.push_str("    mov .Larena_count(%rip), %r12\n    cmp $4096, %r12\n    jae 8f\n");
    out.push_str("    lea (%r12,%r12), %rcx\n    mov %rax, .Larena_ptrs(,%rcx,8)\n");
    out.push_str("    mov %rbx, .Larena_ptrs+8(,%rcx,8)\n    inc %r12\n    mov %r12, .Larena_count(%rip)\n");
    out.push_str("    pop %r12\n    pop %rbx\n    ret\n8:\n    mov %rax, %rdi\n    mov %rbx, %rsi\n    call munmap@PLT\n");
    out.push_str("9:\n    lea .Lom(%rip), %rdi\n    call .Lir_panic\n");
    out.push_str(".p2align 4\n.Lmf_arena_end:\n    push %rbx\n    push %r12\n    sub $8, %rsp\n    mov %rdi, %r12\n");
    out.push_str("1:\n    mov .Larena_count(%rip), %rbx\n    cmp %r12, %rbx\n    jbe 2f\n");
    out.push_str("    dec %rbx\n    mov %rbx, .Larena_count(%rip)\n    lea (%rbx,%rbx), %rcx\n");
    out.push_str("    mov .Larena_ptrs(,%rcx,8), %rdi\n    mov .Larena_ptrs+8(,%rcx,8), %rsi\n");
    out.push_str("    call munmap@PLT\n    jmp 1b\n");
    out.push_str("2:\n    add $8, %rsp\n    pop %r12\n    pop %rbx\n    ret\n");
    // String literals.
    let mut strs: std::collections::BTreeSet<(String, usize, String)> = Default::default();
    for f in &prog.funcs {
        for b in &f.blocks {
            for i in &b.instrs {
                if let Instr::Str { id, s, .. } = i {
                    strs.insert((f.name.clone(), *id, s.clone()));
                }
            }
        }
    }
    if !strs.is_empty() {
        out.push_str(".section .rodata\n");
        for (fname, id, text) in &strs {
            out.push_str(&format!(".Lstr_{}_{}: .asciz \"{}\"\n", fname, id, asm_escape(text)));
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

fn scale_insn(size: u64, reg: &str) -> String {
    if size > 1 && (size & (size - 1)) == 0 {
        format!("    shl ${}, {}\n", size.trailing_zeros(), reg)
    } else {
        format!("    imul ${}, {}\n", size, reg)
    }
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
