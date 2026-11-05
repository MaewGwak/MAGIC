//! End-to-end: MAGIC source -> C -> gcc -> executable -> stdout.
//! Skipped when no C compiler is on PATH.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};

fn cc() -> Option<String> {
    for name in ["gcc", "clang", "cc"] {
        let ok = Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(name.to_string());
        }
    }
    None
}

fn build_and_run_raw(name: &str, src: &str) -> Option<(i32, String, String)> {
    let cc = cc()?;
    let dir = std::env::temp_dir().join(format!("magic_e2e_{}_{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).ok()?;
    let c = match magic::compile_to_c(src) {
        Ok(c) => c,
        Err(e) => panic!("MAGIC compilation failed for {name}: {e}"),
    };
    let c_path = dir.join("t.c");
    let exe_path = dir.join(if cfg!(windows) { "t.exe" } else { "t" });
    std::fs::write(&c_path, c).ok()?;
    let status = Command::new(&cc)
        .arg("-O1")
        .arg("-o")
        .arg(&exe_path)
        .arg(&c_path)
        .status()
        .ok()?;
    if !status.success() {
        panic!("C compilation failed for {name}");
    }
    let out = Command::new(&exe_path).output().ok()?;
    let _ = std::fs::remove_dir_all(&dir);
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n").trim().to_string();
    Some((
        out.status.code().unwrap_or(-1),
        norm(&out.stdout),
        norm(&out.stderr),
    ))
}

fn build_and_run(name: &str, src: &str) -> Option<String> {
    build_and_run_raw(name, src).map(|(_, stdout, _)| stdout)
}

#[test]
fn hello_prints() {
    let out = build_and_run("hello", "fn main() { print(\"e2e\"); }");
    if let Some(out) = out {
        assert_eq!(out, "e2e");
    }
}

#[test]
fn array_parameter_sums() {
    let src = "fn sum(xs: [i32], n: i32) -> i32 {\n\
               var i = 0;\n\
               var total = 0;\n\
               while i < n {\n\
               total = total + xs[i];\n\
               i = i + 1;\n\
               }\n\
               return total;\n\
               }\n\
               fn main() {\n\
               var a: [i32] = [1; 4];\n\
               a[0] = 10;\n\
               a[1] = 20;\n\
               a[2] = 30;\n\
               a[3] = 40;\n\
               print(sum(a, 4));\n\
               }";
    let out = build_and_run("sum", src);
    if let Some(out) = out {
        assert_eq!(out, "100");
    }
}

#[test]
fn arena_prints_inside_and_after() {
    let src = "fn main() {\n\
               let n = 4;\n\
               arena {\n\
               var a: [i32] = [1; n];\n\
               a[0] = 5;\n\
               print(a[0]);\n\
               }\n\
               print(\"after\");\n\
               }";
    let out = build_and_run("arena", src);
    if let Some(out) = out {
        assert_eq!(out, "5\nafter");
    }
}

#[test]
fn index_field_path_roundtrip() {
    let src = "struct Inner { a: i32, b: i32 }\n\
               struct Outer { i: Inner, c: i64 }\n\
               fn main() {\n\
               var os: [Outer] = [Outer { i: Inner { a: 1, b: 2 }, c: 3 }; 3];\n\
               os[1].i.a = 42;\n\
               print(os[1].i.a);\n\
               print(os[0].i.a);\n\
               print(os[1].c);\n\
               }";
    let out = build_and_run("indexpath", src);
    if let Some(out) = out {
        assert_eq!(out, "42\n1\n3");
    }
}

#[test]
fn chained_field_access_works() {
    let src = "struct Inner { a: i32, b: i32 }\n\
               struct Outer { i: Inner, c: i64 }\n\
               fn main() {\n\
               var o: Outer = Outer { i: Inner { a: 3, b: 4 }, c: 9 };\n\
               print(o.i.a);\n\
               o.i.a = 7;\n\
               print(o.i.a);\n\
               print(o.c);\n\
               }";
    let out = build_and_run("chainfield", src);
    if let Some(out) = out {
        assert_eq!(out, "3\n7\n9");
    }
}

#[test]
fn nested_struct_field_roundtrip() {
    let src = "struct Inner { a: i32, b: i32 }\n\
               struct Outer { i: Inner, c: i64 }\n\
               fn main() {\n\
               var o: Outer = Outer { i: Inner { a: 3, b: 4 }, c: 9 };\n\
               var inner: Inner = o.i;\n\
               print(inner.a);\n\
               print(inner.b);\n\
               print(o.c);\n\
               }";
    let out = build_and_run("nestedstruct", src);
    if let Some(out) = out {
        assert_eq!(out, "3\n4\n9");
    }
}

#[test]
fn struct_vec_pop_returns_struct() {
    let src = "struct P { x: i32, y: i32 }\n\
               fn main() {\n\
               var v: [P+] = [];\n\
               v.push(P { x: 1, y: 2 });\n\
               v.push(P { x: 3, y: 4 });\n\
               var last: P = pop(v);\n\
               print(last.x);\n\
               print(last.y);\n\
               print(len(v));\n\
               }";
    let out = build_and_run("structvecpop", src);
    if let Some(out) = out {
        assert_eq!(out, "3\n4\n1");
    }
}

#[test]
fn struct_vec_roundtrip() {
    let src = "struct P { x: i32, y: i32 }\n\
               fn add(v: [P+]) { v.push(P { x: 4, y: 5 }); }\n\
               fn main() {\n\
               var v: [P+] = [];\n\
               add(v);\n\
               v.push(P { x: 7, y: 8 });\n\
               print(len(v));\n\
               print(v[0].x);\n\
               print(v[1].y);\n\
               }";
    let out = build_and_run("structvec", src);
    if let Some(out) = out {
        assert_eq!(out, "2\n4\n8");
    }
}

#[test]
fn clear_resets_length() {
    let src = "fn main() {\n\
               var v: [i32+] = [];\n\
               v.push(7);\n\
               v.push(8);\n\
               print(len(v));\n\
               clear(v);\n\
               print(len(v));\n\
               v.push(9);\n\
               print(v[0]);\n\
               }";
    let out = build_and_run("clear", src);
    if let Some(out) = out {
        assert_eq!(out, "2\n0\n9");
    }
}

#[test]
fn reserve_avoids_reallocation() {
    let src = "fn main() {\n\
               var v: [i32+] = [];\n\
               reserve(v, 1000);\n\
               for i in 0..100 { v.push(i); }\n\
               print(len(v));\n\
               print(v[99]);\n\
               }";
    let out = build_and_run("reserve", src);
    if let Some(out) = out {
        assert_eq!(out, "100\n99");
    }
}

#[test]
fn pop_removes_last_element() {
    let src = "fn main() {\n\
               var v: [i32+] = [];\n\
               v.push(1);\n\
               v.push(2);\n\
               v.push(3);\n\
               print(pop(v));\n\
               print(len(v));\n\
               print(pop(v));\n\
               print(len(v));\n\
               }";
    let out = build_and_run("pop", src);
    if let Some(out) = out {
        assert_eq!(out, "3\n2\n2\n1");
    }
}

#[test]
fn pop_on_vec_parameter_updates_caller() {
    let src = "fn take(v: [i32+]) -> i32 { return pop(v); }\n\
               fn main() {\n\
               var v: [i32+] = [];\n\
               v.push(9);\n\
               print(take(v));\n\
               print(len(v));\n\
               }";
    let out = build_and_run("popparam", src);
    if let Some(out) = out {
        assert_eq!(out, "9\n0");
    }
}

#[test]
fn struct_array_reads_and_writes_fields() {
    let src = "struct Point { x: i32, y: i32 }\n\
               fn main() {\n\
               var a: [Point] = [Point { x: 0, y: 0 }; 4];\n\
               a[0].x = 7;\n\
               a[1].y = 3;\n\
               print(a[0].x);\n\
               print(a[1].y);\n\
               print(a[2].x);\n\
               }";
    let out = build_and_run("structarr", src);
    if let Some(out) = out {
        assert_eq!(out, "7\n3\n0");
    }
}

#[test]
fn vec_parameter_can_push() {
    let src = "fn fill(v: [i32+]) {\n\
               v.push(5);\n\
               v.push(6);\n\
               }\n\
               fn main() {\n\
               var v: [i32+] = [];\n\
               fill(v);\n\
               print(len(v));\n\
               print(v[0] + v[1]);\n\
               }";
    let out = build_and_run("vecpushparam", src);
    if let Some(out) = out {
        assert_eq!(out, "2\n11");
    }
}

#[test]
fn vec_parameter_sums() {
    let src = "fn total(v: [i32+]) -> i32 {\n\
               var t = 0;\n\
               for i in 0..(len(v) as i32) { t = t + v[i]; }\n\
               return t;\n\
               }\n\
               fn main() {\n\
               var v: [i32+] = [];\n\
               v.push(3);\n\
               v.push(4);\n\
               print(total(v));\n\
               }";
    let out = build_and_run("vecparam", src);
    if let Some(out) = out {
        assert_eq!(out, "7");
    }
}

#[test]
fn abs_works_for_narrow_and_wide_types() {
    let src = "fn main() {\n\
               var a: u8 = 5;\n\
               print(abs(a));\n\
               var b: i64 = 0 - 5000000000;\n\
               print(abs(b));\n\
               }";
    let out = build_and_run("abswidths", src);
    if let Some(out) = out {
        assert_eq!(out, "5\n5000000000");
    }
}

#[test]
fn min_max_work_for_u8_and_i64() {
    let src = "fn main() {\n\
               var a: u8 = 5;\n\
               var b: u8 = 9;\n\
               print(min(a, b));\n\
               print(max(a, b));\n\
               var c: i64 = 5000000000;\n\
               var d: i64 = 3000000000;\n\
               print(min(c, d));\n\
               print(max(c, d));\n\
               }";
    let out = build_and_run("minmaxwidths", src);
    if let Some(out) = out {
        assert_eq!(out, "5\n9\n3000000000\n5000000000");
    }
}

#[test]
fn min_and_max_work() {
    let src = "fn main() {\n\
               print(min(3, 7));\n\
               print(max(3, 7));\n\
               print(min(2.5, 1.5));\n\
               print(max(2.5, 1.5));\n\
               }";
    let out = build_and_run("minmax", src);
    if let Some(out) = out {
        assert_eq!(out, "3\n7\n1.5\n2.5");
    }
}

#[test]
fn abs_works_for_int_and_float() {
    let src = "fn main() {\n\
               print(abs(0 - 5));\n\
               print(abs(7));\n\
               print(abs(0.0 - 2.5));\n\
               }";
    let out = build_and_run("abs", src);
    if let Some(out) = out {
        assert_eq!(out, "5\n7\n2.5");
    }
}

#[test]
fn str_byte_n_uses_the_given_length() {
    let src = "fn main() {\n\
               let s = \"hello\";\n\
               let n = str_len(s);\n\
               print(str_byte_n(s, 1, n));\n\
               print(str_byte_n(s, 4, n));\n\
               }";
    let out = build_and_run("strbyten", src);
    if let Some(out) = out {
        assert_eq!(out, "101\n111");
    }
    let oob = "fn main() { print(str_byte_n(\"abc\", 3, 3)); }";
    if let Some((code, _, err)) = build_and_run_raw("strbytenoob", oob) {
        assert_eq!(code, 1, "stderr: {err}");
    }
}

#[test]
fn str_byte_reads_and_bounds_checks() {
    let ok = build_and_run("strbyte", "fn main() { print(str_byte(\"ABC\", 1)); }");
    if let Some(out) = ok {
        assert_eq!(out, "66"); // 'B'
    }
    let src = "fn main() { print(str_byte(\"ABC\", 5)); }";
    if let Some((code, _, err)) = build_and_run_raw("strbyteoob", src) {
        assert_eq!(code, 1, "stderr: {err}");
        assert!(err.contains("index out of bounds"), "stderr was: {err}");
    }
}

#[test]
fn str_len_counts_bytes() {
    let out = build_and_run("strlen", "fn main() { print(str_len(\"hello\")); }");
    if let Some(out) = out {
        assert_eq!(out, "5");
    }
}

#[test]
fn str_eq_compares_contents() {
    let src = "fn main() { print(str_eq(\"abc\", \"abc\")); print(str_eq(\"a\", \"b\")); }";
    let out = build_and_run("streq", src);
    if let Some(out) = out {
        assert_eq!(out, "true\nfalse");
    }
}

#[test]
fn examples_produce_expected_output() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let expected: &[(&str, &str)] = &[
        ("arena", "5\nafter"),
        ("arena_bump", "7\n2"),
        ("array_param", "100"),
        ("cast", "3\n7"),
        ("fib", "832040"),
        ("for", "45"),
        ("hashmap", "2500\n1000000\n-1"),
        ("hello", "Hello, MAGIC!"),
        ("tokenize", "43\n8\n35"),
        ("inventory", "103\n4\n60"),
        ("points", "0\n11\n20"),
        ("sieve", "664579"),
        ("struct", "11\n2"),
        ("sum", "499999500000"),
        ("vec", "10\n285"),
    ];
    for (name, want) in expected {
        let src = std::fs::read_to_string(dir.join(format!("{name}.magic")))
            .unwrap_or_else(|e| panic!("{name}.magic: {e}"));
        if let Some(out) = build_and_run(&format!("ex_{name}"), &src) {
            assert_eq!(&out, want, "example {name}");
        }
    }
}

#[test]
fn struct_return_roundtrip() {
    let src = "struct Point { x: i32, y: i32 }\n\
               fn make() -> Point { return Point { x: 11, y: 22 }; }\n\
               fn main() { var p: Point = make(); print(p.x); print(p.y); }";
    let out = build_and_run("structret", src);
    if let Some(out) = out {
        assert_eq!(out, "11\n22");
    }
}

#[test]
fn struct_parameter_reads_field() {
    let src = "struct Point { x: i32, y: i32 }\n\
               fn px(p: Point) -> i32 { return p.x; }\n\
               fn main() { var p: Point = Point { x: 7, y: 2 }; print(px(p)); }";
    let out = build_and_run("structparam", src);
    if let Some(out) = out {
        assert_eq!(out, "7");
    }
}

#[test]
fn vec_push_and_index() {
    let src = "fn main() {\n\
               var v: [i32+] = [];\n\
               v.push(1);\n\
               v.push(2);\n\
               v.push(3);\n\
               print(len(v));\n\
               print(v[0] + v[1] + v[2]);\n\
               }";
    let out = build_and_run("vec", src);
    if let Some(out) = out {
        assert_eq!(out, "3\n6");
    }
}

#[test]
fn vec_grows_past_initial_capacity() {
    let src = "fn main() {\n\
               var v: [i32+] = [];\n\
               for i in 0..100 { v.push(i); }\n\
               print(len(v));\n\
               print(v[99]);\n\
               }";
    let out = build_and_run("vecgrow", src);
    if let Some(out) = out {
        assert_eq!(out, "100\n99");
    }
}

#[test]
fn len_returns_array_length() {
    let src = "fn main() { var a: [i32] = [7; 5]; print(len(a)); }";
    let out = build_and_run("len", src);
    if let Some(out) = out {
        assert_eq!(out, "5");
    }
}

#[test]
fn break_and_continue_work() {
    let src = "fn main() {\n\
               var t = 0;\n\
               for i in 0..10 {\n\
               if i == 2 { continue; }\n\
               if i == 6 { break; }\n\
               t = t + i;\n\
               }\n\
               print(t);\n\
               }";
    let out = build_and_run("brk", src);
    if let Some(out) = out {
        assert_eq!(out, "13"); // 0+1+3+4+5
    }
}

#[test]
fn break_outside_loop_is_rejected() {
    assert!(magic::compile_to_c("fn main() { break; }").is_err());
}

#[test]
fn break_inside_arena_is_rejected() {
    assert!(magic::compile_to_c(
        "fn main() { while true { arena { var a: [i32] = [0; 2]; break; } } }"
    )
    .is_err());
}

#[test]
fn for_loop_sums() {
    let src = "fn main() { var t = 0; for i in 0..5 { t = t + i; } print(t); }";
    let out = build_and_run("forloop", src);
    if let Some(out) = out {
        assert_eq!(out, "10");
    }
}

#[test]
fn assert_passes() {
    let out = build_and_run("assertok", "fn main() { assert 1 == 1; print(\"ok\"); }");
    if let Some(out) = out {
        assert_eq!(out, "ok");
    }
}

#[test]
fn assert_fails_with_exit_one() {
    let src = "fn main() { assert 1 == 2; }";
    if let Some((code, _, err)) = build_and_run_raw("assertfail", src) {
        assert_eq!(code, 1, "stderr: {err}");
        assert!(err.contains("assertion failed"), "stderr was: {err}");
        assert!(err.contains("line"), "panic should name the line: {err}");
    }
}

#[test]
fn cast_truncates_float_to_int() {
    let out = build_and_run("cast", "fn main() { print(3.9 as i32); }");
    if let Some(out) = out {
        assert_eq!(out, "3");
    }
}

#[test]
fn print_prec_prints_three_decimals() {
    let out = build_and_run("prec", "fn main() { print_prec(1.5, 3); }");
    if let Some(out) = out {
        assert_eq!(out, "1.500");
    }
}

#[test]
fn sqrt_prints_three() {
    let out = build_and_run("sqrt", "fn main() { print(sqrt(9.0)); }");
    if let Some(out) = out {
        assert_eq!(out, "3");
    }
}

#[test]
fn out_of_bounds_exits_one() {
    let src = "fn main() { var a: [i32] = [1; 4]; print(a[10]); }";
    if let Some((code, _, err)) = build_and_run_raw("oob", src) {
        assert_eq!(code, 1, "stderr: {err}");
        assert!(err.contains("index out of bounds"), "stderr was: {err}");
        assert!(err.contains("line"), "panic should name the source line: {err}");
    }
}

#[test]
fn negative_index_exits_one() {
    let src = "fn main() { var a: [i32] = [1; 4]; var i = 0 - 1; print(a[i]); }";
    if let Some((code, _, err)) = build_and_run_raw("negoob", src) {
        assert_eq!(code, 1, "stderr: {err}");
        assert!(err.contains("index out of bounds"), "stderr was: {err}");
    }
}

#[test]
fn struct_field_roundtrip() {
    let src = "struct P { x: i32, y: i32 }\n\
               fn main() {\n\
               var p: P = P { x: 7, y: 35 };\n\
               p.x = p.x + 1;\n\
               print(p.x);\n\
               print(p.y);\n\
               }";
    let out = build_and_run("struct", src);
    if let Some(out) = out {
        assert_eq!(out, "8\n35");
    }
}

#[test]
fn else_if_chain_picks_the_matching_branch() {
    let src = "fn main() {\n\
               let x = 2;\n\
               if x == 1 {\n\
               print(1);\n\
               } else if x == 2 {\n\
               print(2);\n\
               } else {\n\
               print(3);\n\
               }\n\
               }";
    let out = build_and_run("elseif", src);
    if let Some(out) = out {
        assert_eq!(out, "2");
    }
}

#[test]
fn par_for_runs() {
    let src = "fn main() {\n\
               var a: [i32] = [0; 8];\n\
               par for i in 0..8 { a[i] = i * i; }\n\
               var s = 0;\n\
               for i in 0..8 { s = s + a[i]; }\n\
               print(s);\n\
               }";
    let out = build_and_run("parfor", src);
    if let Some(out) = out {
        assert_eq!(out, "140");
    }
}

#[test]
fn file_io_roundtrip() {
    let path = std::env::temp_dir().join(format!("magic_io_{}.bin", std::process::id()));
    let p = path.to_string_lossy().replace('\\', "/");
    let src = format!(
        r#"
fn main() {{
    var buf: [u8] = [0; 8];
    buf[0] = 65; buf[1] = 66; buf[2] = 67;
    var fd = file_open("{p}", "wb");
    print(file_write(fd, buf, 3));
    print(file_close(fd));
    var rd = file_open("{p}", "rb");
    print(file_size(rd));
    var got: [u8] = [0; 8];
    print(file_read(rd, got, 3));
    print(got[0]); print(got[1]); print(got[2]);
    print(file_seek(rd, 1, 0));
    print(file_read(rd, got, 2));
    print(got[0]); print(got[1]);
    print(file_seek(rd, 0, 0));
    print(file_read_at(rd, got, 4, 2));
    print(got[4]); print(got[5]);
    print(file_close(rd));
}}
"#
    );
    let out = build_and_run("file_io", &src);
    if let Some(out) = out {
        assert_eq!(out, "3\n0\n3\n3\n65\n66\n67\n1\n2\n66\n67\n0\n2\n65\n66\n0");
    }
    let _ = std::fs::remove_file(&path);
}

fn build_and_run_with(
    name: &str,
    src: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Option<String> {
    let cc = cc()?;
    let dir = std::env::temp_dir().join(format!("magic_e2e_{}_{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).ok()?;
    let c = match magic::compile_to_c(src) {
        Ok(c) => c,
        Err(e) => panic!("MAGIC compilation failed for {name}: {e}"),
    };
    let c_path = dir.join("t.c");
    let exe_path = dir.join(if cfg!(windows) { "t.exe" } else { "t" });
    std::fs::write(&c_path, c).ok()?;
    let status = Command::new(&cc)
        .arg("-O1")
        .arg("-o")
        .arg(&exe_path)
        .arg(&c_path)
        .status()
        .ok()?;
    if !status.success() {
        panic!("C compilation failed for {name}");
    }
    let out = Command::new(&exe_path)
        .args(args)
        .envs(envs.iter().copied())
        .output()
        .ok()?;
    let _ = std::fs::remove_dir_all(&dir);
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n").trim().to_string();
    Some(norm(&out.stdout))
}

#[test]
fn args_and_env() {
    let src = r#"
fn main() {
    print(argc());
    print(arg(1));
    print(arg(2));
    print(env("MAGIC_E2E_VAR"));
}
"#;
    let out = build_and_run_with(
        "args_env",
        src,
        &["alpha", "beta"],
        &[("MAGIC_E2E_VAR", "hello")],
    );
    if let Some(out) = out {
        assert_eq!(out, "3\nalpha\nbeta\nhello");
    }
}

#[test]
fn print_bytes_writes_raw() {
    let src = r#"
fn main() {
    var v: [u8+] = [];
    v.push(65);
    v.push(66);
    v.push(67);
    print_bytes(v);
}
"#;
    if let Some(out) = build_and_run("print_bytes", src) {
        assert_eq!(out, "ABC");
    }
}

fn build_and_run_stdin(name: &str, src: &str, input: &str) -> Option<String> {
    let cc = cc()?;
    let dir = std::env::temp_dir().join(format!("magic_e2e_{}_{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).ok()?;
    let c = match magic::compile_to_c(src) {
        Ok(c) => c,
        Err(e) => panic!("MAGIC compilation failed for {name}: {e}"),
    };
    let c_path = dir.join("t.c");
    let exe_path = dir.join(if cfg!(windows) { "t.exe" } else { "t" });
    std::fs::write(&c_path, c).ok()?;
    let status = Command::new(&cc).arg("-O1").arg("-o").arg(&exe_path).arg(&c_path).status().ok()?;
    if !status.success() {
        panic!("C compilation failed for {name}");
    }
    let mut child = Command::new(&exe_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.as_mut()?.write_all(input.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    let _ = std::fs::remove_dir_all(&dir);
    Some(String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n").trim().to_string())
}

#[test]
fn input_read_reads_stdin() {
    let src = r#"
fn main() {
    var b: [u8] = [0; 8];
    var n = input_read(b, 5);
    print(n);
    print(b[0]);
}
"#;
    if let Some(out) = build_and_run_stdin("stdin", src, "hello") {
        assert_eq!(out, "5\n104");
    }
}

#[test]
fn tcp_roundtrip() {
    let Some(cc) = cc() else { return };
    let dir = std::env::temp_dir().join(format!("magic_e2e_{}_tcp", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let src = r#"
fn main() {
    var lfd = net_listen(0);
    print(net_port(lfd));
    flush();
    var cfd = net_accept(lfd);
    var buf: [u8] = [0; 16];
    var n = net_read(cfd, buf, 4);
    print(n);
    var out: [u8] = [0; 4];
    out[0] = 112; out[1] = 111; out[2] = 110; out[3] = 103;
    net_write(cfd, out, 4);
    net_close(cfd);
    net_close(lfd);
}
"#;
    let c = magic::compile_to_c(src).expect("compile");
    let c_path = dir.join("t.c");
    let exe = dir.join(if cfg!(windows) { "t.exe" } else { "t" });
    std::fs::write(&c_path, c).expect("write C");
    let status = Command::new(&cc).arg("-O1").arg("-o").arg(&exe).arg(&c_path).status().expect("cc");
    assert!(status.success(), "C compilation failed");
    let mut child = Command::new(&exe).stdout(Stdio::piped()).spawn().expect("spawn server");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read port");
    let port: u16 = line.trim().parse().expect("port number");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.write_all(b"ping").expect("send");
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).expect("recv");
    assert_eq!(&buf, b"pong");
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pg_wire_queries() {
    let Some(cc) = cc() else { return };
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/bench-db/pgserver_magic.magic"
    ))
    .expect("read pgserver source");
    let dir = std::env::temp_dir().join(format!("magic_e2e_{}_pg", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let heap = dir.join("heap.dat");
    let mut bytes = 3i64.to_le_bytes().to_vec();
    for (k, v) in [(1i64, 70i64), (2, 140), (3, 210)] {
        bytes.extend(k.to_le_bytes());
        bytes.extend(v.to_le_bytes());
    }
    std::fs::write(&heap, bytes).expect("write heap");

    let c = magic::compile_to_c(&src).expect("compile pgserver");
    let c_path = dir.join("t.c");
    let exe = dir.join(if cfg!(windows) { "t.exe" } else { "t" });
    std::fs::write(&c_path, c).expect("write C");
    let status = Command::new(&cc).arg("-O1").arg("-o").arg(&exe).arg(&c_path).status().expect("cc");
    assert!(status.success(), "C compilation failed");
    let mut child = Command::new(&exe)
        .arg("0")
        .arg(&heap)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn pgserver");
    let mut reader = BufReader::new(child.stdout.take().expect("stdout"));
    let mut line = String::new();
    reader.read_line(&mut line).expect("read port");
    let port: u16 = line.trim().parse().expect("port");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");

    let mut body = 196608i32.to_be_bytes().to_vec();
    body.push(0);
    let mut startup = ((body.len() + 4) as i32).to_be_bytes().to_vec();
    startup.extend(body);
    stream.write_all(&startup).expect("startup");

    let mut read_msg = |s: &mut TcpStream| -> (u8, Vec<u8>) {
        let mut head = [0u8; 5];
        s.read_exact(&mut head).expect("message head");
        let len = i32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
        let mut payload = vec![0u8; len - 4];
        s.read_exact(&mut payload).expect("message payload");
        (head[0], payload)
    };

    let (tag, _) = read_msg(&mut stream);
    assert_eq!(tag, b'R');
    let (tag, _) = read_msg(&mut stream);
    assert_eq!(tag, b'Z');

    let mut query = |s: &mut TcpStream, sql: &str| -> String {
        let mut q = sql.as_bytes().to_vec();
        q.push(0);
        let mut msg = b"Q".to_vec();
        msg.extend(((q.len() + 4) as i32).to_be_bytes());
        msg.append(&mut q);
        s.write_all(&msg).expect("query");
        let mut value = String::new();
        loop {
            let (tag, payload) = read_msg(s);
            if tag == b'D' {
                let len = i32::from_be_bytes([payload[2], payload[3], payload[4], payload[5]]) as usize;
                value = String::from_utf8_lossy(&payload[6..6 + len]).to_string();
            }
            if tag == b'Z' {
                break;
            }
        }
        value
    };

    assert_eq!(query(&mut stream, "SELECT 1"), "1");
    assert_eq!(query(&mut stream, "SELECT COUNT(*) FROM t"), "3");
    assert_eq!(query(&mut stream, "SELECT SUM(value) FROM t"), "420");
    assert_eq!(query(&mut stream, "SELECT value FROM t WHERE id = 2"), "140");
    assert_eq!(query(&mut stream, "SELECT value FROM t WHERE id = 99"), "0");
    drop(stream);
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
