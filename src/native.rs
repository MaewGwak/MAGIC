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

use crate::ast::{BinOp, Expr, ExprKind, Func, ParamTy, Program, Stmt, StmtKind, UnOp};
use std::collections::HashMap;

struct Emitter {
    out: String,
    rodata: Vec<(String, String)>,
    label: usize,
    /// How many 8-byte temporaries are pushed. Used for 16-byte call alignment.
    depth: usize,
    locals: HashMap<String, i64>,
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

    fn assign_frame(&mut self, f: &Func) -> Result<(), String> {
        self.locals.clear();
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
        self.frame = align16(8 * slot);
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
