//! Integration tests: source -> C, through the public API.

#[test]
fn hello_pipeline() {
    let c = magic::compile_to_c("fn main() { print(\"Hello, MAGIC!\"); }").unwrap();
    assert!(c.contains("#include <stdint.h>"));
    assert!(c.contains("int main(void)"));
    assert!(c.contains("Hello, MAGIC!"));
}

#[test]
fn loops_and_mutation() {
    let src = "fn main() {\n\
               var i: i64 = 0;\n\
               var total: i64 = 0;\n\
               while i < 10 { total = total + i; i = i + 1; }\n\
               print(total);\n\
               }";
    let c = magic::compile_to_c(src).unwrap();
    assert!(c.contains("while ((i < 10))"));
    assert!(c.contains("int64_t total = 0;"));
}

#[test]
fn type_errors_are_rejected() {
    // bool variable, integer initializer
    assert!(magic::compile_to_c("fn main() { let x: bool = 1; }").is_err());
    // wrong argument type
    assert!(magic::compile_to_c(
        "fn f(a: i32) -> i32 { return a; } fn main() { f(true); }"
    )
    .is_err());
    // missing main
    assert!(magic::compile_to_c("fn f() {}").is_err());
}

#[test]
fn memory_report_sums_constant_arrays() {
    let rows =
        magic::memory_report("fn main() { let n = 10; var a: [i32] = [0; n]; }").unwrap();
    assert_eq!(rows[0], ("main".to_string(), 40));
}

#[test]
fn memory_report_ignores_dynamic_sizes() {
    let rows = magic::memory_report(
        "fn f(n: i32) { var a: [i32] = [0; n]; } fn main() { f(3); }",
    )
    .unwrap();
    assert_eq!(rows[0], ("f".to_string(), 0));
}
#[test]
fn all_examples_compile() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("examples directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("magic") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read example");
        magic::compile_to_c(&src)
            .unwrap_or_else(|e| panic!("{:?}: {e}", path.file_name().unwrap()));
        checked += 1;
    }
    assert!(checked >= 10, "expected at least 10 examples, found {checked}");
}

#[test]
fn reports_one_row_per_arena() {
    let src = "fn main() {\n\
               arena {\n\
               var a: [i32] = [0; 2];\n\
               }\n\
               arena {\n\
               var b: [i32] = [0; 3];\n\
               }\n\
               }";
    let rows = magic::memory_report(src).unwrap();
    assert_eq!(rows.len(), 3, "rows: {rows:?}");
    assert_eq!(rows[0], ("main".to_string(), 20));
    assert_eq!(rows[1].1, 8);
    assert_eq!(rows[2].1, 12);
}

#[test]
fn reports_struct_array_sizes() {
    let rows = magic::memory_report(
        "struct P { x: i32, y: i32 }\nfn main() { var a: [P] = [P { x: 0, y: 0 }; 10]; }",
    )
    .unwrap();
    assert_eq!(rows[0], ("main".to_string(), 80));
}

#[test]
fn reports_nested_struct_sizes() {
    let src = "struct Inner { a: i32, b: i32 }\n\
               struct Outer { i: Inner, c: i64 }\n\
               fn main() { var a: [Outer] = [Outer { i: Inner { a: 0, b: 0 }, c: 0 }; 10]; }";
    let rows = magic::memory_report(src).unwrap();
    assert_eq!(rows[0], ("main".to_string(), 160));
}

#[test]
fn reports_struct_padding() {
    let src = "struct S { a: u8, b: i64 }\n\
               fn main() { var a: [S] = [S { a: 0, b: 0 }; 10]; }";
    let rows = magic::memory_report(src).unwrap();
    assert_eq!(rows[0], ("main".to_string(), 160));
}

#[test]
fn reports_bounds_for_expression_sizes() {
    let rows = magic::memory_report("fn main() { let n = 4 * 4; var a: [i32] = [0; n]; }").unwrap();
    assert_eq!(rows[0], ("main".to_string(), 64));
}

#[test]
fn reports_arena_bound_separately() {
    let rows = magic::memory_report("fn main() { arena { var a: [i32] = [0; 4]; } }").unwrap();
    assert_eq!(rows[0], ("main".to_string(), 16));
    assert_eq!(rows[1], ("main arena at line 1".to_string(), 16));
}

#[test]
fn small_programs_stay_single_file() {
    let parts = magic::compile_parts("fn main() { print(1); }", 8).unwrap();
    assert_eq!(parts.len(), 1);
}

#[test]
fn large_programs_split_into_chunks() {
    let mut src = String::new();
    for i in 0..40 {
        src.push_str(&format!("fn f{i}(x: i32) -> i32 {{ return x + {i}; }}\n"));
    }
    src.push_str("fn main() { print(f0(1)); }\n");
    let parts = magic::compile_parts(&src, 4).unwrap();
    assert_eq!(parts.len(), 4);
    // main is defined exactly once
    assert_eq!(
        parts.iter().filter(|p| p.contains("int main(void) {")).count(),
        1
    );
    // the chunk holding main declares the function it calls
    let main_chunk = parts
        .iter()
        .find(|p| p.contains("int main(void) {"))
        .unwrap();
    assert!(main_chunk.contains("int32_t f0(int32_t x);"));
}

#[test]
fn string_escapes_survive_codegen() {
    let c = magic::compile_to_c("fn main() { print(\"a\\nb\\\"c\"); }").unwrap();
    assert!(c.contains("a\\nb\\\"c"));
}
