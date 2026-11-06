//! Deterministic fuzz-lite: random token soup must produce diagnostics, never
//! a panic, from any compiler stage (C backend, AST-native and IR-native).
use std::panic::{catch_unwind, AssertUnwindSafe};

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

const TOKENS: &[&str] = &[
    "fn", "main", "var", "let", "if", "else", "while", "for", "in", "return", "print", "struct",
    "match", "break", "continue", "par", "arena", "assert", "import", "x", "y", "xs", "i", "n",
    "0", "1", "42", "3.5", "true", "false", "\"s\"", "{", "}", "(", ")", "[", "]", ";", ":", ",",
    ".", "+", "-", "*", "/", "%", "=", "==", "!=", "<", "<=", ">", ">=", "&&", "||", "!", "..",
    "as", "->", "&",
];

fn random_source(seed: u64) -> String {
    let mut state = seed;
    let len = (lcg(&mut state) % 80) as usize + 1;
    let mut out = String::new();
    for _ in 0..len {
        out.push_str(TOKENS[(lcg(&mut state) as usize) % TOKENS.len()]);
        out.push(' ');
        if lcg(&mut state) % 7 == 0 {
            out.push('\n');
        }
    }
    out
}

#[test]
fn random_sources_never_panic() {
    let mut failures = Vec::new();
    for seed in 0..2000u64 {
        let src = random_source(seed.wrapping_mul(0x9e3779b97f4a7c15));
        let c = catch_unwind(AssertUnwindSafe(|| {
            let _ = magic::compile_to_c(&src);
        }));
        let n = catch_unwind(AssertUnwindSafe(|| {
            let _ = magic::compile_to_asm(&src);
        }));
        let i = catch_unwind(AssertUnwindSafe(|| {
            let _ = magic::compile_to_asm_ir(&src);
        }));
        if c.is_err() || n.is_err() || i.is_err() {
            failures.push((seed, src.clone()));
        }
    }
    assert!(failures.is_empty(), "panics on random input: {:?}", failures.first());
}

#[test]
fn deep_nesting_is_rejected_not_crashed() {
    let mut src = String::from("fn main() { ");
    for _ in 0..4000 {
        src.push('{');
    }
    for _ in 0..4000 {
        src.push('}');
    }
    src.push_str(" }");
    let r = catch_unwind(AssertUnwindSafe(|| {
        let _ = magic::compile_to_c(&src);
    }));
    assert!(r.is_ok(), "parser must reject deep nesting without panicking");
    let huge = format!("fn main() {{ var x = {}; }}", "1 + ".repeat(20000) + "1");
    let r = catch_unwind(AssertUnwindSafe(|| {
        let _ = magic::compile_to_c(&huge);
    }));
    assert!(r.is_ok(), "long expression chains must not panic");
}
