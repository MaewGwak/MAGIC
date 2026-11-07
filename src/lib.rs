//! MAGIC v0 pipeline: source -> tokens -> AST -> type check -> C.

pub mod ast;
pub mod codegen;
pub mod ir;
pub mod lexer;
pub mod native;
pub mod parser;
pub mod types;

/// Paths named by `import "..."` statements, in source order.
pub fn imports_of(src: &str) -> Result<Vec<String>, String> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    let program = parser::Parser::new(tokens).parse_program()?;
    Ok(program.imports)
}

/// Compile MAGIC source to a single C11 translation unit.
pub fn compile_to_c(src: &str) -> Result<String, String> {
    compile_to_c_opts(src, false)
}

/// Like [`compile_to_c`], optionally unrolling small constant `for` loops.
pub fn compile_to_c_opts(src: &str, unroll: bool) -> Result<String, String> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    let mut program = parser::Parser::new(tokens).parse_program()?;
    types::Checker::check(&mut program)?;
    types::lower_calls(&mut program);
    codegen::retain_reachable(&mut program);
    Ok(codegen::emit_opts(&program, unroll))
}

/// Compile into translation units. Small programs stay a single file.
pub fn compile_parts(src: &str, parts: usize) -> Result<Vec<String>, String> {
    compile_parts_opts(src, parts, false)
}

pub fn compile_parts_opts(src: &str, parts: usize, unroll: bool) -> Result<Vec<String>, String> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    let mut program = parser::Parser::new(tokens).parse_program()?;
    types::Checker::check(&mut program)?;
    types::lower_calls(&mut program);
    codegen::retain_reachable(&mut program);
    let n = if program.funcs.len() < 32 {
        1
    } else {
        parts.max(1)
    };
    Ok(codegen::emit_parts_opts(&program, n, unroll))
}

/// Compile MAGIC source to x86-64 assembly (Linux, System V ABI).
pub fn compile_to_asm(src: &str) -> Result<String, String> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    let mut program = parser::Parser::new(tokens).parse_program()?;
    types::Checker::check(&mut program)?;
    types::lower_calls(&mut program);
    codegen::retain_reachable(&mut program);
    native::emit(&program)
}

/// Compile to x86-64 assembly through the experimental typed IR.
pub fn compile_to_asm_ir(src: &str) -> Result<String, String> {
    compile_to_asm_ir_with_sources(src, &[])
}

/// Compile to IR assembly with `(concatenated_start_line, display_path)` segments
/// so runtime panics can report `file:line` instead of a loader offset.
pub fn compile_to_asm_ir_with_sources(src: &str, sources: &[(usize, String)]) -> Result<String, String> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    let mut program = parser::Parser::new(tokens).parse_program()?;
    types::Checker::check(&mut program)?;
    types::lower_calls(&mut program);
    codegen::retain_reachable(&mut program);
    let mut ir = ir::lower(&program)?;
    ir.source_files = sources.to_vec();
    ir::optimize(&mut ir);
    Ok(ir::emit(&ir))
}

/// Syntactic sum of the bytes each function's fixed arrays can occupy.
/// Only constant sizes (`[T] = [v; 10]` or `[v; n]` with `let n = 10`) are counted.
/// This is NOT a peak-RSS bound: it ignores recursion depth, growable-array growth,
/// stack frames, allocator metadata and fragmentation. Use it as a lowering hint,
/// never as a proof.
pub fn memory_report(src: &str) -> Result<Vec<(String, u64)>, String> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    let mut program = parser::Parser::new(tokens).parse_program()?;
    types::Checker::check(&mut program)?;
    Ok(report(&program))
}

fn align_up(n: u64, a: u64) -> u64 {
    if a <= 1 {
        n
    } else {
        (n + a - 1) / a * a
    }
}

pub fn ty_align(t: ast::Ty, structs: &[ast::StructDef]) -> u64 {
    use ast::Ty::*;
    match t {
        Bool | U8 => 1,
        I32 => 4,
        I64 | F64 => 8,
        Struct(i) => structs
            .get(i as usize)
            .map(|s| s.fields.iter().map(|(_, ft)| ty_align(*ft, structs)).max().unwrap_or(1))
            .unwrap_or(1),
        _ => 1,
    }
}

pub fn ty_size(t: ast::Ty, structs: &[ast::StructDef]) -> u64 {
    use ast::Ty::*;
    match t {
        Struct(i) => {
            let Some(s) = structs.get(i as usize) else {
                return 0;
            };
            let mut off = 0u64;
            let mut max_align = 1u64;
            for (_, ft) in &s.fields {
                let a = ty_align(*ft, structs);
                max_align = max_align.max(a);
                off = align_up(off, a) + ty_size(*ft, structs);
            }
            align_up(off, max_align)
        }
        Bool | U8 => 1,
        I32 => 4,
        I64 | F64 | Str => 8,
        _ => 0,
    }
}

fn elem_size(t: ast::Ty, structs: &[ast::StructDef]) -> u64 {
    ty_size(t, structs)
}

fn const_i64(
    e: &ast::Expr,
    consts: &std::collections::HashMap<String, i64>,
) -> Option<i64> {
    use ast::{BinOp, ExprKind};
    match &e.kind {
        ExprKind::Int(v) => Some(*v),
        ExprKind::Var(n) => consts.get(n).copied(),
        ExprKind::Bin { op, l, r } => {
            let a = const_i64(l, consts)?;
            let b = const_i64(r, consts)?;
            match op {
                BinOp::Add => Some(a + b),
                BinOp::Sub => Some(a - b),
                BinOp::Mul => Some(a * b),
                BinOp::Div => {
                    if b != 0 {
                        Some(a / b)
                    } else {
                        None
                    }
                }
                BinOp::Rem => {
                    if b != 0 {
                        Some(a % b)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn sum_stmts(
    stmts: &[ast::Stmt],
    consts: &mut std::collections::HashMap<String, i64>,
    total: &mut u64,
    structs: &[ast::StructDef],
) {
    use ast::StmtKind;
    for s in stmts {
        match &s.kind {
            StmtKind::Let { name, init, .. } => {
                if let Some(v) = const_i64(init, consts) {
                    consts.insert(name.clone(), v);
                }
            }
            StmtKind::ArrayLet { elem, count, .. } => {
                if let Some(n) = const_i64(count, consts) {
                    if n > 0 {
                        // `[bool]` is bit-packed: eight elements per byte.
                        let bytes = if *elem == ast::Ty::Bool {
                            ((n as u64) + 7) / 8
                        } else {
                            (n as u64) * elem_size(*elem, structs)
                        };
                        *total += bytes;
                    }
                }
            }
            StmtKind::Arena { body } => sum_stmts(body, consts, total, structs),
            _ => {}
        }
    }
}

fn report(program: &ast::Program) -> Vec<(String, u64)> {
    let mut rows = Vec::new();
    for f in &program.funcs {
        let mut consts = std::collections::HashMap::new();
        let mut total: u64 = 0;
        sum_stmts(&f.body, &mut consts, &mut total, &program.structs);
        rows.push((f.name.clone(), total));
        collect_arenas(&f.body, &consts, &f.name, &mut rows, &program.structs);
    }
    rows
}

fn collect_arenas(
    stmts: &[ast::Stmt],
    consts: &std::collections::HashMap<String, i64>,
    fname: &str,
    rows: &mut Vec<(String, u64)>,
    structs: &[ast::StructDef],
) {
    for s in stmts {
        if let ast::StmtKind::Arena { body } = &s.kind {
            let mut sub_consts = consts.clone();
            let mut total: u64 = 0;
            sum_stmts(body, &mut sub_consts, &mut total, structs);
            rows.push((format!("{} arena at line {}", fname, s.line), total));
            collect_arenas(body, consts, fname, rows, structs);
        }
    }
}
