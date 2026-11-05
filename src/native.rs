//! Experimental native x86-64 backend (Linux, System V ABI).
//!
//! Emits GNU assembler text. Subset: integer functions with up to six register
//! arguments (more through the stack), locals, arithmetic, comparisons,
//! `if`/`while`/`for`, calls, `print`, casts between integer types, and heap
//! arrays (including array parameters, which the checker lowers to a pointer
//! plus a length argument) with bounds checks. Structs, floats, vectors and the
//! I/O builtins stay on the C backend until later milestones.
//!
//! Calling convention for array parameters: the checker rewrites a call
//! `f(a, x)` into `f(a, a.len, x)`, so the native prologue reads the pointer
//! and the length like any other two integer arguments.

use crate::ast::{BinOp, Expr, ExprKind, Func, ParamTy, Program, Stmt, StmtKind, Ty, UnOp};
use std::collections::HashMap;

struct Emitter {
    out: String,
    rodata: Vec<(String, String)>,
    label: usize,
    /// 8-byte temporaries currently pushed; used for 16-byte call alignment.
    depth: usize,
    locals: HashMap<String, i64>,
    /// name -> (slot holding the data pointer, element type, constant length or -1)
    arrays: HashMap<String, (i64, Ty, i64)>,
    /// Array parameters whose length is only known at runtime.
    array_len_slot: HashMap<String, i64>,
    /// Slots of arrays this function allocated and must free on return.
    owned: Vec<i64>,
    frame: i64,
    ret_label: String,
    loops: Vec<(String, String)>,
}

fn align16(n: i64) -> i64 {
    (n + 15) & !15
}

const ARG_REGS: [&str; 6] = ["%rdi", "%rsi", "%rdx", "%rcx", "%r8", "%r9"];

impl Emitter {
    fn new() -> Self {
        Emitter {
            out: String::new(),
            rodata: Vec::new(),
            label: 0,
            depth: 0,
            locals: HashMap::new(),
            arrays: HashMap::new(),
            array_len_slot: HashMap::new(),
            owned: Vec::new(),
            frame: 0,
            ret_label: String::new(),
            loops: Vec::new(),
        }
    }

    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn fresh(&mut self, base: &str) -> String {
        self.label += 1;
        format!(".L{}{}", base, self.label)
    }

    fn push(&mut self, reg: &str) {
        self.line(&format!("    push {}", reg));
        self.depth += 1;
    }

    fn pop(&mut self, reg: &str) {
        self.line(&format!("    pop {}", reg));
        self.depth -= 1;
    }

    /// Call with automatic 16-byte stack alignment.
    fn call(&mut self, target: &str) {
        let pad = self.depth % 2 == 1;
        if pad {
            self.line("    sub $8, %rsp");
            self.depth += 1;
        }
        self.line(&format!("    call {}", target));
        if pad {
            self.line("    add $8, %rsp");
            self.depth -= 1;
        }
    }

    fn panic_msg(&mut self, line: usize, text: &str) -> String {
        let label = format!(".Lmsg{}", self.rodata.len());
        self.rodata.push((
            label.clone(),
            format!("line {}: {}", line, text),
        ));
        label
    }

    fn elem_size(ty: Ty) -> i64 {
        match ty {
            Ty::U8 | Ty::Bool => 1,
            Ty::I32 => 4,
            Ty::I64 | Ty::F64 => 8,
            _ => 8,
        }
    }

    fn const_int(e: &Expr, consts: &HashMap<String, i64>) -> Option<i64> {
        match &e.kind {
            ExprKind::Int(v) => Some(*v),
            ExprKind::Var(n) => consts.get(n).copied(),
            ExprKind::Un { op: UnOp::Neg, e } => Self::const_int(e, consts).map(|v| -v),
            _ => None,
        }
    }

    fn collect_locals(stmts: &[Stmt], names: &mut Vec<String>) {
        for s in stmts {
            match &s.kind {
                StmtKind::Let { name, .. } => names.push(name.clone()),
                StmtKind::For { var, body, .. } => {
                    names.push(var.clone());
                    Self::collect_locals(body, names);
                }
                StmtKind::If { then, els, .. } => {
                    Self::collect_locals(then, names);
                    Self::collect_locals(els, names);
                }
                StmtKind::While { body, .. } => Self::collect_locals(body, names),
                StmtKind::Arena { body } => Self::collect_locals(body, names),
                _ => {}
            }
        }
    }

    fn collect_arrays(
        stmts: &[Stmt],
        consts: &mut HashMap<String, i64>,
        out: &mut Vec<(String, Ty, i64)>,
    ) -> Result<(), String> {
        for s in stmts {
            match &s.kind {
                StmtKind::Let { name, init, .. } => {
                    if let Some(v) = Self::const_int(init, consts) {
                        consts.insert(name.clone(), v);
                    }
                }
                StmtKind::ArrayLet { name, elem, count, .. } => {
                    let len = Self::const_int(count, consts).ok_or_else(|| {
                        format!(
                            "native backend line {}: array `{}` needs a constant length",
                            s.line, name
                        )
                    })?;
                    out.push((name.clone(), *elem, len));
                }
                StmtKind::If { then, els, .. } => {
                    Self::collect_arrays(then, consts, out)?;
                    Self::collect_arrays(els, consts, out)?;
                }
                StmtKind::While { body, .. } => Self::collect_arrays(body, consts, out)?,
                StmtKind::For { var, body, .. } => {
                    consts.remove(var);
                    Self::collect_arrays(body, consts, out)?;
                }
                StmtKind::Arena { body } => Self::collect_arrays(body, consts, out)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn assign_frame(&mut self, f: &Func) -> Result<(), String> {
        self.locals.clear();
        self.arrays.clear();
        self.array_len_slot.clear();
        self.owned.clear();
        let mut slot = 0i64;
        for (name, pty) in &f.params {
            match pty {
                ParamTy::Scalar(_) => {
                    slot += 1;
                    self.locals.insert(name.clone(), 8 * slot);
                }
                ParamTy::Array(elem) => {
                    slot += 1;
                    self.arrays.insert(name.clone(), (8 * slot, *elem, -1));
                    slot += 1;
                    self.array_len_slot.insert(name.clone(), 8 * slot);
                }
                ParamTy::Vec(_) => {
                    return Err(format!(
                        "native backend: `{}` takes `{}` by vec, which the native backend does not support yet",
                        f.name, name
                    ))
                }
            }
        }
        let mut names = Vec::new();
        Self::collect_locals(&f.body, &mut names);
        let mut seen = std::collections::HashSet::new();
        for n in names {
            if seen.insert(n.clone()) {
                slot += 1;
                self.locals.insert(n, 8 * slot);
            }
        }
        let mut consts = HashMap::new();
        let mut arrays = Vec::new();
        Self::collect_arrays(&f.body, &mut consts, &mut arrays)?;
        for (name, elem, len) in arrays {
            slot += 1;
            self.arrays.insert(name, (8 * slot, elem, len));
            self.owned.push(8 * slot);
        }
        self.frame = align16(8 * slot);
        Ok(())
    }

    fn bounds_check(&mut self, idx: &str, name: &str, line: usize) -> Result<(), String> {
        let (_, _, len) = *self
            .arrays
            .get(name)
            .ok_or_else(|| format!("native backend line {}: unknown array `{}`", line, name))?;
        let len_slot = self.array_len_slot.get(name).copied();
        let msg = self.panic_msg(line, "index out of bounds");
        let nonneg = self.fresh("idxnn");
        let ok = self.fresh("idxok");
        self.line(&format!("    cmp $0, {}", idx));
        self.line(&format!("    jge {}", nonneg));
        self.line(&format!("    lea {}(%rip), %rdi", msg));
        self.call(".Lmagic_panic");
        self.line(&format!("{}:", nonneg));
        if let Some(len_off) = len_slot {
            self.line(&format!("    mov -{}(%rbp), %r11", len_off));
            self.line(&format!("    cmp %r11, {}", idx));
        } else {
            self.line(&format!("    cmp ${}, {}", len, idx));
        }
        self.line(&format!("    jl {}", ok));
        self.line(&format!("    lea {}(%rip), %rdi", msg));
        self.call(".Lmagic_panic");
        self.line(&format!("{}:", ok));
        Ok(())
    }

    fn array_load(&mut self, name: &str, idx_reg: &str, elem: Ty, line: usize) -> Result<(), String> {
        let (off, _, _) = *self
            .arrays
            .get(name)
            .ok_or_else(|| format!("native backend line {}: unknown array `{}`", line, name))?;
        self.line(&format!("    mov -{}(%rbp), %r10", off));
        match elem {
            Ty::U8 | Ty::Bool => self.line(&format!("    movzbq (%r10,{},1), %rax", idx_reg)),
            Ty::I32 => self.line(&format!("    movslq (%r10,{},4), %rax", idx_reg)),
            Ty::I64 => self.line(&format!("    movq (%r10,{},8), %rax", idx_reg)),
            other => {
                return Err(format!(
                    "native backend line {}: array element type {:?} is not supported yet",
                    line, other
                ))
            }
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> Result<(), String> {
        match &e.kind {
            ExprKind::Int(v) => {
                if *v >= i32::MIN as i64 && *v <= i32::MAX as i64 {
                    self.line(&format!("    mov ${}, %rax", v));
                } else {
                    self.line(&format!("    movabs ${}, %rax", v));
                }
            }
            ExprKind::Bool(b) => self.line(&format!("    mov ${}, %rax", if *b { 1 } else { 0 })),
            ExprKind::Str(s) => {
                let label = format!(".Lstr{}", self.rodata.len());
                self.rodata.push((label.clone(), s.clone()));
                self.line(&format!("    lea {}(%rip), %rax", label));
            }
            ExprKind::Var(n) => {
                if let Some(off) = self.locals.get(n) {
                    self.line(&format!("    mov -{}(%rbp), %rax", *off));
                } else if let Some((off, _, _)) = self.arrays.get(n) {
                    self.line(&format!("    mov -{}(%rbp), %rax", *off));
                } else {
                    return Err(format!(
                        "native backend line {}: unknown variable `{}`",
                        e.line, n
                    ));
                }
            }
            ExprKind::Bin { op, l, r } => {
                self.expr(l)?;
                self.push("%rax");
                self.expr(r)?;
                self.line("    mov %rax, %rcx");
                self.pop("%rax");
                match op {
                    BinOp::Add => self.line("    add %rcx, %rax"),
                    BinOp::Sub => self.line("    sub %rcx, %rax"),
                    BinOp::Mul => self.line("    imul %rcx, %rax"),
                    BinOp::Div | BinOp::Rem => {
                        self.line("    cqo");
                        self.line("    idiv %rcx");
                        if *op == BinOp::Rem {
                            self.line("    mov %rdx, %rax");
                        }
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        self.line("    cmp %rcx, %rax");
                        let set = match op {
                            BinOp::Eq => "sete",
                            BinOp::Ne => "setne",
                            BinOp::Lt => "setl",
                            BinOp::Le => "setle",
                            BinOp::Gt => "setg",
                            BinOp::Ge => "setge",
                            _ => unreachable!(),
                        };
                        self.line(&format!("    {} %al", set));
                        self.line("    movzbq %al, %rax");
                    }
                    BinOp::And => self.line("    and %rcx, %rax"),
                    BinOp::Or => self.line("    or %rcx, %rax"),
                }
            }
            ExprKind::Un { op, e } => {
                self.expr(e)?;
                match op {
                    UnOp::Neg => self.line("    neg %rax"),
                    UnOp::Not => self.line("    xor $1, %rax"),
                }
            }
            ExprKind::Cast { e, ty } => {
                self.expr(e)?;
                match ty {
                    Ty::I64 | Ty::Str => {}
                    Ty::I32 => self.line("    movslq %eax, %rax"),
                    Ty::U8 => self.line("    movzbq %al, %rax"),
                    Ty::Bool => {
                        self.line("    test %rax, %rax");
                        self.line("    setne %al");
                        self.line("    movzbq %al, %rax");
                    }
                    other => {
                        return Err(format!(
                            "native backend line {}: cast to {:?} is not supported yet",
                            e.line, other
                        ))
                    }
                }
            }
            ExprKind::Index { name, index } => {
                let (_, elem, _) = *self
                    .arrays
                    .get(name)
                    .ok_or_else(|| format!("native backend line {}: unknown array `{}`", e.line, name))?;
                self.expr(index)?;
                self.bounds_check("%rax", name, e.line)?;
                self.array_load(name, "%rax", elem, e.line)?;
            }
            ExprKind::ArrayLen(name) => {
                let (_, _, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown array `{}`", e.line, name)
                })?;
                if let Some(len_off) = self.array_len_slot.get(name) {
                    self.line(&format!("    mov -{}(%rbp), %rax", *len_off));
                } else {
                    self.line(&format!("    mov ${}, %rax", len));
                }
            }
            ExprKind::Call { name, args } => self.call_expr(name, args, e.line)?,
            other => {
                return Err(format!(
                    "native backend line {}: expression not supported yet ({:?})",
                    e.line, other
                ))
            }
        }
        Ok(())
    }

    fn call_expr(&mut self, name: &str, args: &[Expr], line: usize) -> Result<(), String> {
        if name == "flush" && args.is_empty() {
            self.call("fflush@PLT");
            return Ok(());
        }
        if name == "str_len" && args.len() == 1 {
            self.expr(&args[0])?;
            self.line("    mov %rax, %rdi");
            self.call("strlen@PLT");
            return Ok(());
        }
        if name == "str_byte" && args.len() == 2 {
            self.expr(&args[0])?;
            self.push("%rax");
            self.line("    mov %rax, %rdi");
            self.call("strlen@PLT");
            self.line("    mov %rax, %r11");
            self.pop("%r10");
            self.expr(&args[1])?;
            self.line("    mov %rax, %rcx");
            let zero = self.fresh("strz");
            let done = self.fresh("strd");
            self.line("    cmp $0, %rcx");
            self.line(&format!("    jl {}", zero));
            self.line("    cmp %r11, %rcx");
            self.line(&format!("    jge {}", zero));
            self.line("    movzbq (%r10,%rcx,1), %rax");
            self.line(&format!("    jmp {}", done));
            self.line(&format!("{}:", zero));
            self.line("    mov $0, %eax");
            self.line(&format!("{}:", done));
            return Ok(());
        }
        if name == "str_byte_n" && args.len() == 3 {
            self.expr(&args[0])?;
            self.push("%rax");
            self.expr(&args[1])?;
            self.push("%rax");
            self.expr(&args[2])?;
            self.line("    mov %rax, %r11");
            self.pop("%rcx");
            self.pop("%r10");
            let zero = self.fresh("strz");
            let done = self.fresh("strd");
            self.line("    cmp $0, %rcx");
            self.line(&format!("    jl {}", zero));
            self.line("    cmp %r11, %rcx");
            self.line(&format!("    jge {}", zero));
            self.line("    movzbq (%r10,%rcx,1), %rax");
            self.line(&format!("    jmp {}", done));
            self.line(&format!("{}:", zero));
            self.line("    mov $0, %eax");
            self.line(&format!("{}:", done));
            return Ok(());
        }
        let n = args.len();
        if n > 6 {
            let stack_k = n - 6;
            let pad = (self.depth + stack_k) % 2 == 1;
            if pad {
                self.line("    sub $8, %rsp");
                self.depth += 1;
            }
            for i in (6..n).rev() {
                self.expr(&args[i])?;
                self.push("%rax");
            }
            for i in (0..6).rev() {
                self.expr(&args[i])?;
                self.push("%rax");
            }
            for i in 0..6 {
                let r = ARG_REGS[i].to_string();
                self.pop(&r);
            }
            self.line(&format!("    call {}", name));
            self.line(&format!("    add ${}, %rsp", 8 * stack_k));
            self.depth -= stack_k;
            if pad {
                self.line("    add $8, %rsp");
                self.depth -= 1;
            }
        } else {
            for a in args {
                self.expr(a)?;
                self.push("%rax");
            }
            for i in (0..n).rev() {
                let r = ARG_REGS[i].to_string();
                self.pop(&r);
            }
            if args.is_empty() {
                self.call(name);
            } else {
                self.call(name);
            }
        }
        let _ = line;
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                self.expr(init)?;
                let off = *self.locals.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown variable `{}`", s.line, name)
                })?;
                self.line(&format!("    mov %rax, -{}(%rbp)", off));
            }
            StmtKind::Assign { name, value } => {
                self.expr(value)?;
                let off = *self.locals.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown variable `{}`", s.line, name)
                })?;
                self.line(&format!("    mov %rax, -{}(%rbp)", off));
            }
            StmtKind::Print { value, ty } => {
                let as_str = matches!(value.kind, ExprKind::Str(_)) || matches!(ty, Some(Ty::Str));
                self.expr(value)?;
                if as_str {
                    self.line("    mov %rax, %rsi");
                    self.line("    lea .Lfmt_s(%rip), %rdi");
                } else {
                    self.line("    mov %rax, %rsi");
                    self.line("    lea .Lfmt_i(%rip), %rdi");
                }
                self.line("    xor %eax, %eax");
                self.call("printf@PLT");
            }
            StmtKind::Expr(e) => {
                self.expr(e)?;
            }
            StmtKind::Return(e) => {
                match e {
                    Some(e) => self.expr(e)?,
                    None => self.line("    xor %eax, %eax"),
                }
                let label = self.ret_label.clone();
                self.line(&format!("    jmp {}", label));
            }
            StmtKind::If { cond, then, els } => {
                let else_label = self.fresh("else");
                let end_label = self.fresh("endif");
                self.expr(cond)?;
                self.line("    cmp $0, %rax");
                self.line(&format!("    je {}", else_label));
                for st in then {
                    self.stmt(st)?;
                }
                if !els.is_empty() {
                    self.line(&format!("    jmp {}", end_label));
                }
                self.line(&format!("{}:", else_label));
                for st in els {
                    self.stmt(st)?;
                }
                self.line(&format!("{}:", end_label));
            }
            StmtKind::While { cond, body } => {
                let start = self.fresh("while");
                let end = self.fresh("wend");
                self.loops.push((end.clone(), start.clone()));
                self.line(&format!("{}:", start));
                self.expr(cond)?;
                self.line("    cmp $0, %rax");
                self.line(&format!("    je {}", end));
                for st in body {
                    self.stmt(st)?;
                }
                self.line(&format!("    jmp {}", start));
                self.line(&format!("{}:", end));
                self.loops.pop();
            }
            StmtKind::For { var, start, end, body, par } => {
                if *par {
                    return Err(format!(
                        "native backend line {}: `par for` needs the C backend for now",
                        s.line
                    ));
                }
                self.expr(start)?;
                let voff = *self.locals.get(var).ok_or_else(|| {
                    format!("native backend line {}: unknown loop variable `{}`", s.line, var)
                })?;
                self.line(&format!("    mov %rax, -{}(%rbp)", voff));
                let head = self.fresh("for");
                let cont = self.fresh("forc");
                let done = self.fresh("forend");
                self.loops.push((done.clone(), cont.clone()));
                self.line(&format!("{}:", head));
                self.expr(end)?;
                self.line("    mov %rax, %rcx");
                self.line(&format!("    mov -{}(%rbp), %rax", voff));
                self.line("    cmp %rcx, %rax");
                self.line(&format!("    jge {}", done));
                for st in body {
                    self.stmt(st)?;
                }
                self.line(&format!("{}:", cont));
                self.line(&format!("    mov -{}(%rbp), %rax", voff));
                self.line("    add $1, %rax");
                self.line(&format!("    mov %rax, -{}(%rbp)", voff));
                self.line(&format!("    jmp {}", head));
                self.line(&format!("{}:", done));
                self.loops.pop();
            }
            StmtKind::Break => {
                let (b, _) = self
                    .loops
                    .last()
                    .ok_or_else(|| format!("native backend line {}: break outside a loop", s.line))?;
                let b = b.clone();
                self.line(&format!("    jmp {}", b));
            }
            StmtKind::Continue => {
                let (_, c) = self
                    .loops
                    .last()
                    .ok_or_else(|| format!("native backend line {}: continue outside a loop", s.line))?;
                let c = c.clone();
                self.line(&format!("    jmp {}", c));
            }
            StmtKind::ArrayLet { name, value, .. } => {
                let (off, elem, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown array `{}`", s.line, name)
                })?;
                self.expr(value)?;
                self.push("%rax");
                let size = Self::elem_size(elem) * len;
                self.line(&format!("    mov ${}, %rdi", size));
                self.call("malloc@PLT");
                let ok = self.fresh("malloc");
                self.line("    test %rax, %rax");
                self.line(&format!("    jne {}", ok));
                let msg = self.panic_msg(s.line, "out of memory");
                self.line(&format!("    lea {}(%rip), %rdi", msg));
                self.call(".Lmagic_panic");
                self.line(&format!("{}:", ok));
                self.line(&format!("    mov %rax, -{}(%rbp)", off));
                self.line("    mov %rax, %r10");
                self.pop("%rax");
                self.line(&format!("    mov ${}, %rcx", len));
                self.line("    mov $0, %r8");
                let fill = self.fresh("fill");
                let filled = self.fresh("filled");
                self.line(&format!("{}:", fill));
                self.line("    cmp %rcx, %r8");
                self.line(&format!("    jge {}", filled));
                match elem {
                    Ty::U8 | Ty::Bool => self.line("    movb %al, (%r10,%r8,1)"),
                    Ty::I32 => self.line("    movl %eax, (%r10,%r8,4)"),
                    Ty::I64 => self.line("    movq %rax, (%r10,%r8,8)"),
                    other => {
                        return Err(format!(
                            "native backend line {}: array element type {:?} is not supported yet",
                            s.line, other
                        ))
                    }
                }
                self.line("    add $1, %r8");
                self.line(&format!("    jmp {}", fill));
                self.line(&format!("{}:", filled));
            }
            StmtKind::IndexAssign { name, index, value } => {
                let (off, elem, _) = *self.arrays.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown array `{}`", s.line, name)
                })?;
                self.expr(value)?;
                self.push("%rax");
                self.expr(index)?;
                self.bounds_check("%rax", name, s.line)?;
                self.pop("%rcx");
                self.line(&format!("    mov -{}(%rbp), %r10", off));
                match elem {
                    Ty::U8 | Ty::Bool => self.line("    movb %cl, (%r10,%rax,1)"),
                    Ty::I32 => self.line("    movl %ecx, (%r10,%rax,4)"),
                    Ty::I64 => self.line("    movq %rcx, (%r10,%rax,8)"),
                    other => {
                        return Err(format!(
                            "native backend line {}: array element type {:?} is not supported yet",
                            s.line, other
                        ))
                    }
                }
            }
            StmtKind::Arena { body } => {
                for st in body {
                    self.stmt(st)?;
                }
            }
            StmtKind::Assert { cond } => {
                let ok = self.fresh("assert");
                self.expr(cond)?;
                self.line("    cmp $0, %rax");
                self.line(&format!("    jne {}", ok));
                let msg = self.panic_msg(s.line, "assertion failed");
                self.line(&format!("    lea {}(%rip), %rdi", msg));
                self.call(".Lmagic_panic");
                self.line(&format!("{}:", ok));
            }
            other => {
                return Err(format!(
                    "native backend line {}: statement not supported yet ({:?})",
                    s.line, other
                ))
            }
        }
        Ok(())
    }

    fn func(&mut self, f: &Func) -> Result<(), String> {
        self.assign_frame(f)?;
        self.ret_label = self.fresh("ret");
        self.line(&format!(".globl {}", f.name));
        self.line(&format!(".type {}, @function", f.name));
        self.line(&format!("{}:", f.name));
        self.line("    push %rbp");
        self.line("    mov %rsp, %rbp");
        if self.frame > 0 {
            self.line(&format!("    sub ${}, %rsp", self.frame));
        }

        // Expanded parameters: an array parameter arrives as (pointer, length).
        let mut expanded: Vec<(String, bool)> = Vec::new();
        for (name, pty) in &f.params {
            match pty {
                ParamTy::Scalar(_) => expanded.push((name.clone(), false)),
                ParamTy::Array(_) => {
                    expanded.push((name.clone(), true));
                    expanded.push((format!("{}#len", name), false));
                }
                ParamTy::Vec(_) => {
                    return Err(format!(
                        "native backend: `{}` takes `{}` by vec, which the native backend does not support yet",
                        f.name, name
                    ))
                }
            }
        }
        if expanded.len() > 20 {
            return Err(format!(
                "native backend: `{}` has too many parameters for the native backend",
                f.name
            ));
        }
        for (idx, (name, is_array)) in expanded.iter().enumerate() {
            let src = if idx < 6 {
                ARG_REGS[idx].to_string()
            } else {
                format!("{}(%rbp)", 16 + 8 * (idx - 6))
            };
            if *is_array {
                let (ptr_off, _, _) = self.arrays[name];
                if idx < 6 {
                    self.line(&format!("    mov {}, -{}(%rbp)", src, ptr_off));
                } else {
                    self.line(&format!("    mov {}, %rax", src));
                    self.line(&format!("    mov %rax, -{}(%rbp)", ptr_off));
                }
                let (len_src, len_in_reg) = if idx + 1 < 6 {
                    (ARG_REGS[idx + 1].to_string(), true)
                } else {
                    (format!("{}(%rbp)", 16 + 8 * (idx + 1 - 6)), false)
                };
                let len_off = self.array_len_slot[name];
                if len_in_reg {
                    self.line(&format!("    mov {}, -{}(%rbp)", len_src, len_off));
                } else {
                    self.line(&format!("    mov {}, %rax", len_src));
                    self.line(&format!("    mov %rax, -{}(%rbp)", len_off));
                }
            } else if !name.ends_with("#len") {
                let off = self.locals[name];
                if idx < 6 {
                    self.line(&format!("    mov {}, -{}(%rbp)", src, off));
                } else {
                    self.line(&format!("    mov {}, %rax", src));
                    self.line(&format!("    mov %rax, -{}(%rbp)", off));
                }
            }
        }

        for s in &f.body {
            self.stmt(s)?;
        }
        if f.name == "main" {
            self.line("    xor %eax, %eax");
        }
        self.line(&format!("{}:", self.ret_label));
        let owned = self.owned.clone();
        if !owned.is_empty() {
            self.push("%rax");
            for off in owned {
                self.line(&format!("    mov -{}(%rbp), %rdi", off));
                self.call("free@PLT");
            }
            self.pop("%rax");
        }
        self.line("    leave");
        self.line("    ret");
        self.line(&format!(".size {}, .-{}", f.name, f.name));
        Ok(())
    }
}

pub fn emit(prog: &Program) -> Result<String, String> {
    let mut e = Emitter::new();
    e.line(".text");
    for f in &prog.funcs {
        e.func(f)?;
    }
    e.line(".section .rodata");
    e.line(".Lfmt_i: .asciz \"%ld\\n\"");
    e.line(".Lfmt_s: .asciz \"%s\\n\"");
    e.line(".Lfmt_panic: .asciz \"%s\\n\"");
    let rodata = std::mem::take(&mut e.rodata);
    for (label, text) in &rodata {
        let mut escaped = String::new();
        for ch in text.chars() {
            match ch {
                '"' => escaped.push_str("\\\""),
                '\\' => escaped.push_str("\\\\"),
                '\n' => escaped.push_str("\\n"),
                '\t' => escaped.push_str("\\t"),
                c => escaped.push(c),
            }
        }
        e.line(&format!("{}: .asciz \"{}\"", label, escaped));
    }
    e.line(".text");
    e.line(".Lmagic_panic:");
    e.line("    push %rbp");
    e.line("    mov %rsp, %rbp");
    e.line("    mov %rdi, %rdx");
    e.line("    mov stderr@GOTPCREL(%rip), %rax");
    e.line("    mov (%rax), %rdi");
    e.line("    lea .Lfmt_panic(%rip), %rsi");
    e.line("    xor %eax, %eax");
    e.line("    call fprintf@PLT");
    e.line("    mov $1, %edi");
    e.line("    call exit@PLT");
    e.line(".section .note.GNU-stack,\"\",@progbits");
    Ok(e.out)
}
