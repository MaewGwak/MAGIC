//! Experimental native x86-64 backend (Linux, System V ABI).
//!
//! Emits GNU assembler text for an integer subset: functions with up to six
//! integer parameters, locals, arithmetic, comparisons, `if`/`while`/`for`,
//! calls and `print` of integers and string literals. Arrays, structs, floats,
//! vectors and sockets stay on the C backend until later milestones.
//!
//! The subset is deliberately small: correctness first, speed later. Every
//! expression is evaluated into `%rax`; sub-expressions spill through `%rcx`
//! and the stack.

use crate::ast::{BinOp, Expr, ExprKind, Func, ParamTy, Program, Stmt, StmtKind, Ty, UnOp};
use std::collections::HashMap;

struct Emitter {
    out: String,
    rodata: Vec<(String, String)>,
    label: usize,
    /// How many 8-byte temporaries are pushed. Used for 16-byte call alignment.
    depth: usize,
    locals: HashMap<String, i64>,
    arrays: HashMap<String, (i64, Ty, i64)>,
    frame: i64,
    ret_label: String,
    loops: Vec<(String, String)>,
}

fn align16(n: i64) -> i64 {
    (n + 15) & !15
}

impl Emitter {
    fn new() -> Self {
        Emitter {
            out: String::new(),
            rodata: Vec::new(),
            label: 0,
            depth: 0,
            locals: HashMap::new(),
            arrays: HashMap::new(),
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

    /// Collect every name that needs a stack slot: parameters, `var`/`let`
    /// declarations and `for` counters.
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
        let mut names: Vec<String> = f.params.iter().map(|(n, _)| n.clone()).collect();
        Self::collect_locals(&f.body, &mut names);
        let mut seen = std::collections::HashSet::new();
        let mut slot = 0;
        for n in names {
            if seen.insert(n.clone()) {
                slot += 1;
                self.locals.insert(n, 8 * slot);
            }
        }
        let mut consts = HashMap::new();
        let mut arrays = Vec::new();
        Self::collect_arrays(&f.body, &mut consts, &mut arrays)?;
        let mut total = 8 * slot;
        for (name, elem, len) in arrays {
            slot += 1;
            total += 8;
            self.arrays.insert(name, (8 * slot, elem, len));
        }
        self.frame = align16(total);
        Ok(())
    }

    fn bounds_check(&mut self, idx: &str, name: &str, line: usize) -> Result<(), String> {
        let (_, _, len) = *self
            .arrays
            .get(name)
            .ok_or_else(|| format!("native backend line {}: unknown array `{}`", line, name))?;
        let msg_label = format!(".Lmsg{}", self.rodata.len());
        self.rodata.push((
            msg_label.clone(),
            format!("line {}: index out of bounds", line),
        ));
        let nonneg = self.fresh("idxnn");
        let ok = self.fresh("idxok");
        self.line(&format!("    cmp $0, {}", idx));
        self.line(&format!("    jge {}", nonneg));
        self.line(&format!("    lea {}(%rip), %rdi", msg_label));
        self.call(".Lmagic_panic");
        self.line(&format!("{}:", nonneg));
        self.line(&format!("    cmp ${}, {}", len, idx));
        self.line(&format!("    jl {}", ok));
        self.line(&format!("    lea {}(%rip), %rdi", msg_label));
        self.call(".Lmagic_panic");
        self.line(&format!("{}:", ok));
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> Result<(), String> {
        match &e.kind {
            ExprKind::Int(v) => self.line(&format!("    mov ${}, %rax", v)),
            ExprKind::Bool(b) => self.line(&format!("    mov ${}, %rax", if *b { 1 } else { 0 })),
            ExprKind::Str(s) => {
                let label = format!(".Lstr{}", self.rodata.len());
                self.rodata.push((label.clone(), s.clone()));
                self.line(&format!("    lea {}(%rip), %rax", label));
            }
            ExprKind::Var(n) => {
                let off = *self
                    .locals
                    .get(n)
                    .ok_or_else(|| format!("native backend line {}: unknown variable `{}`", e.line, n))?;
                self.line(&format!("    mov -{}(%rbp), %rax", off));
            }
            ExprKind::Index { name, index } => {
                let (off, elem, _len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown array `{}`", e.line, name)
                })?;
                self.expr(index)?;
                self.bounds_check("%rax", name, e.line)?;
                self.line(&format!("    mov -{}(%rbp), %r10", off));
                match elem {
                    Ty::U8 | Ty::Bool => self.line("    movzbq (%r10,%rax,1), %rax"),
                    Ty::I32 => self.line("    movslq (%r10,%rax,4), %rax"),
                    Ty::I64 => self.line("    movq (%r10,%rax,8), %rax"),
                    other => {
                        return Err(format!(
                            "native backend line {}: array element type {:?} is not supported yet",
                            e.line, other
                        ))
                    }
                }
            }
            ExprKind::ArrayLen(name) => {
                let (_, _, len) = *self.arrays.get(name).ok_or_else(|| {
                    format!("native backend line {}: unknown array `{}`", e.line, name)
                })?;
                self.line(&format!("    mov ${}, %rax", len));
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
                    BinOp::Eq => {
                        self.line("    cmp %rcx, %rax");
                        self.line("    sete %al");
                        self.line("    movzbq %al, %rax");
                    }
                    BinOp::Ne => {
                        self.line("    cmp %rcx, %rax");
                        self.line("    setne %al");
                        self.line("    movzbq %al, %rax");
                    }
                    BinOp::Lt => {
                        self.line("    cmp %rcx, %rax");
                        self.line("    setl %al");
                        self.line("    movzbq %al, %rax");
                    }
                    BinOp::Le => {
                        self.line("    cmp %rcx, %rax");
                        self.line("    setle %al");
                        self.line("    movzbq %al, %rax");
                    }
                    BinOp::Gt => {
                        self.line("    cmp %rcx, %rax");
                        self.line("    setg %al");
                        self.line("    movzbq %al, %rax");
                    }
                    BinOp::Ge => {
                        self.line("    cmp %rcx, %rax");
                        self.line("    setge %al");
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
            ExprKind::Call { name, args } => {
                if name == "flush" && args.is_empty() {
                    self.call("fflush@PLT");
                    return Ok(());
                }
                if args.len() > 6 {
                    return Err(format!(
                        "native backend line {}: calls with more than 6 arguments are not supported yet",
                        e.line
                    ));
                }
                for a in args {
                    self.expr(a)?;
                    self.push("%rax");
                }
                let regs = ["%rdi", "%rsi", "%rdx", "%rcx", "%r8", "%r9"];
                for i in (0..args.len()).rev() {
                    self.pop(regs[i]);
                }
                self.call(name);
            }
            other => {
                return Err(format!(
                    "native backend line {}: expression not supported yet ({:?})",
                    e.line, other
                ))
            }
        }
        Ok(())
    }

    fn cond_jump(&mut self, cond: &Expr, target: &str, jump_if_true: bool) -> Result<(), String> {
        self.expr(cond)?;
        self.line("    cmp $0, %rax");
        if jump_if_true {
            self.line(&format!("    jne {}", target));
        } else {
            self.line(&format!("    je {}", target));
        }
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
                let msg_label = format!(".Lmsg{}", self.rodata.len());
                self.rodata
                    .push((msg_label.clone(), format!("line {}: out of memory", s.line)));
                self.line(&format!("    lea {}(%rip), %rdi", msg_label));
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
                let (off, elem, _len) = *self.arrays.get(name).ok_or_else(|| {
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
                let msg_label = format!(".Lmsg{}", self.rodata.len());
                self.rodata
                    .push((msg_label.clone(), "assertion failed".to_string()));
                self.line(&format!("    lea {}(%rip), %rdi", msg_label));
                self.call(".Lmagic_panic");
                self.line(&format!("{}:", ok));
            }
            StmtKind::Print { value, .. } => {
                if let ExprKind::Str(_) = value.kind {
                    self.expr(value)?;
                    self.line("    mov %rax, %rsi");
                    self.line("    lea .Lfmt_s(%rip), %rdi");
                    self.line("    xor %eax, %eax");
                    self.call("printf@PLT");
                } else {
                    self.expr(value)?;
                    self.line("    mov %rax, %rsi");
                    self.line("    lea .Lfmt_i(%rip), %rdi");
                    self.line("    xor %eax, %eax");
                    self.call("printf@PLT");
                }
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
                self.cond_jump(cond, &else_label, false)?;
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
                self.cond_jump(cond, &end, false)?;
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
                // var = start
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
                // compare var < end
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
        let regs = ["%rdi", "%rsi", "%rdx", "%rcx", "%r8", "%r9"];
        if f.params.len() > 6 {
            return Err(format!(
                "native backend: `{}` has more than 6 parameters, which the native backend does not support yet",
                f.name
            ));
        }
        for (i, (name, pty)) in f.params.iter().enumerate() {
            match pty {
                ParamTy::Scalar(_) => {}
                _ => {
                    return Err(format!(
                        "native backend: `{}` takes `{}` by array/vec, which the native backend does not support yet",
                        f.name, name
                    ))
                }
            }
            let off = self.locals[name];
            self.line(&format!("    mov {}, -{}(%rbp)", regs[i], off));
        }
        for s in &f.body {
            self.stmt(s)?;
        }
        if f.name == "main" {
            self.line("    xor %eax, %eax");
        }
        self.line(&format!("{}:", self.ret_label));
        let frees: Vec<i64> = self.arrays.values().map(|(off, _, _)| *off).collect();
        if !frees.is_empty() {
            self.push("%rax");
            for off in frees {
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
    e.line(".section .rodata");
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
    e.line(".section .note.GNU-stack,\"\",@progbits");
    Ok(e.out)
}
