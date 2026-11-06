use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn cc() -> Option<String> {
    for name in ["gcc", "clang", "cc"] {
        if Command::new(name).arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
            return Some(name.to_string());
        }
    }
    None
}

fn run(src: &str, dir: &Path, name: &str, ir: bool, cc_cmd: &str) -> Option<(String, Option<i32>)> {
    let exe = dir.join(name);
    if ir {
        let asm = magic::compile_to_asm_ir(src).ok()?;
        let sp = dir.join(format!("{name}.s"));
        std::fs::write(&sp, asm).ok()?;
        let op = dir.join(format!("{name}.o"));
        if !Command::new("as").arg("--64").arg("-o").arg(&op).arg(&sp).status().ok()?.success() {
            return None;
        }
        if !Command::new(cc_cmd).arg(&op).arg("-o").arg(&exe).status().ok()?.success() {
            return None;
        }
    } else {
        let c = magic::compile_to_c(src).ok()?;
        let cp = dir.join(format!("{name}.c"));
        std::fs::write(&cp, c).ok()?;
        if !Command::new(cc_cmd).arg("-O1").arg("-o").arg(&exe).arg(&cp).status().ok()?.success() {
            return None;
        }
    }
    let mut child = Command::new(&exe).stdout(std::process::Stdio::piped()).spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let out = child.wait_with_output().ok()?;
                return Some((String::from_utf8_lossy(&out.stdout).to_string(), status.code()));
            }
            None => {
                if start.elapsed() > Duration::from_secs(60) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn ir_matches_c_on_small_integer_programs() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return;
    }
    let Some(cc_cmd) = cc() else { return };
    let dir = std::env::temp_dir().join(format!("magic_ir_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cases: &[(&str, &str)] = &[
        (
            "deep_cast_index_chain",
            r#"
fn main() {
    var src: [i32] = [0; 1]; src[0] = 5;
    var x: i32 = src[0];
    var a: [i32] = [0; 16]; a[5] = 42;
    if 1 < 2 { a[x as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32 as i32] = 7; }
    print(a[5]);
}
"#,
        ),
        (
            "cast_i64_to_i32_index_wraps",
            r#"
fn main() {
    var a: [i32] = [0; 16];
    if 1 < 2 { a[(4294967301) as i32] = 7; }
    print(a[5]);
}
"#,
        ),
        (
            "inlined_index_and_compare",
            r#"
fn f(x: i32) -> i32 { return (x + 100) + ((x < 5) as i32); }
fn main() {
    var a: [i32] = [0; 128]; var i: i32 = 0;
    while i < 128 { a[i] = i; i = i + 1; }
    var n: i32 = -1; var r: i32 = 0; var k: i32 = 0;
    while k < 1 { r = a[f(n * 2)]; k = k + 1; }
    print(r);
}
"#,
        ),
        (
            "inlined_index_and_float_cast",
            r#"
fn g(x: i32) -> i32 { return (x + 100) + (((x as f64) * 0.5) as i32); }
fn main() {
    var a: [i32] = [0; 128]; var i: i32 = 0;
    while i < 128 { a[i] = i; i = i + 1; }
    var n: i32 = -1;
    print(a[g(n * 2)]);
}
"#,
        ),
        (
            "min_max_and_len_array",
            r#"
fn main() {
    print(min(3, 5)); print(max(3, 5));
    print(min(-3, 4)); print(max(-3, 4));
    print_prec(min(2.5, 1.5), 1); print_prec(max(2.5, 1.5), 1);
    var a: [i32] = [0; 4];
    print(len(a));
}
"#,
        ),
        (
            "string_return_and_env",
            r#"
fn greet() -> str { return "hello"; }
fn main() {
    var s = greet();
    print(str_len(s));
    print(str_byte(s, 0));
    var p = env("PATH");
    print(str_len(p) > 0);
}
"#,
        ),
        (
            "struct_vec_push_and_fill",
            r#"
struct P { x: i32, y: i32 }
fn main() {
    var p = P { x: 1, y: 2 };
    var v: [P+] = [];
    v.push(p);
    v.push(P { x: 3, y: 4 });
    print(v[0].x + v[0].y);
    print(v[1].x * v[1].y);
    var arr: [P] = [P { x: 5, y: 6 }; 3];
    print(arr[0].x + arr[2].y);
    var q = P { x: 7, y: 8 };
    var arr2: [P] = [q; 2];
    print(arr2[0].x + arr2[1].y);
}
"#,
        ),
        (
            "struct_return_values",
            r#"
struct P { x: i32, y: i32 }
fn make(a: i32) -> P { return P { x: a, y: a + 1 }; }
fn ident(p: P) -> P { return p; }
fn sum(p: P) -> i32 { return p.x + p.y; }
fn pick(a: i32) -> P {
    if a > 2 { return P { x: a, y: 10 }; }
    return P { x: 1, y: 2 };
}
fn main() {
    var p = make(3);
    print(p.x + p.y);
    print(sum(p));
    print(sum(P { x: 5, y: 6 }));
    var q = ident(P { x: 7, y: 8 });
    print(q.x * q.y);
    print(sum(make(10)));
    var r = pick(9);
    print(r.x + r.y);
    var s = pick(0);
    print(s.x + s.y);
}
"#,
        ),
        (
            "struct_return_multi_slot",
            r#"
struct Q { a: i64, b: i32, c: i32, d: i64 }
fn mk(k: i64) -> Q { var q = Q { a: k, b: 2, c: 3, d: k * 10 }; return q; }
fn total(q: Q) -> i64 { return q.a + q.d + (q.b as i64) + (q.c as i64); }
fn main() { var q = mk(5); print(total(q)); print(q.b); var r = mk(7); print(total(r)); }
"#,
        ),
        (
            "increment_block_reentered_by_inner_loop",
            r#"
fn main() {
    var a: [i32] = [0; 10];
    var i = 0;
    while i < 10 {
        var k = 0;
        while k < 3 { a[i] = 1; i = i + 1; k = k + 1; }
    }
    print(a[0]);
}
"#,
        ),
        (
            "access_after_increment_same_block",
            r#"
fn main() {
    var a: [i32] = [0; 10];
    var i = 0;
    while i < 10 { i = i + 1; if i < 5 { print(1); } a[i] = 7; }
    print(a[0]);
}
"#,
        ),
        (
            "nested_loop_reenters_access",
            r#"
fn main() {
    var a: [i32] = [0; 6];
    var i = 0;
    while i < 3 {
        a[i] = 1;
        var k = 0;
        while k < 3 { i = i + 1; k = k + 1; }
    }
    print(a[0]);
}
"#,
        ),
        (
            "div_mod_power_of_two",
            r#"
fn main() {
    for x in -20..20 {
        print(x % 8); print(x / 8);
        print(x % 4); print(x / 4);
        print(x % 16); print(x / 16);
    }
    var big: i64 = 9223372036854775807;
    print(big % 1024); print(big / 1024);
    print(big % 1099511627776); print(big / 1099511627776);
}
"#,
        ),
        (
            "range_seed_requires_canonical_step",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var i = 0;
    while i < 10 { i = i + 5; a[i] = 1; }
    print(a[0]);
}
"#,
        ),
        (
            "range_seed_rejects_negative_init",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var i = -3;
    while i < 10 { a[i] = 1; i = i + 1; }
    print(a[0]);
}
"#,
        ),
        (
            "range_le_loop",
            r#"
fn main() {
    var a: [i32] = [0; 16];
    var i = 0;
    var t = 0;
    while i <= 15 { a[i] = i * 2; t = t + a[i]; i = i + 1; }
    print(t);
}
"#,
        ),
        (
            "i32_variable_traffic",
            r#"
fn main() {
    var x: i32 = -7;
    var y: i32 = 2147483647;
    print(x); print(y);
    y = y + 1; print(y);
    x = x * -3; print(x);
    var big: i64 = 5000000000;
    print(big);
    var a: [i32] = [0; 4];
    var i: i32 = -1;
    print(i + 2);
    a[0] = i; print(a[0]);
}
"#,
        ),
        (
            "iv_mutated_before_access",
            r#"
fn main() {
    var a: [i32] = [0; 11];
    for i in 0..11 { i = i + 1; a[i] = 7; }
    print(a[0]);
}
"#,
        ),
        (
            "iv_affine_mutated_before_access",
            r#"
fn main() {
    var a: [i32] = [0; 20];
    var i = 0;
    while i < 10 { i = i + 1; a[i * 2] = 7; }
    print(a[0]);
}
"#,
        ),
        (
            "iv_incremented_in_inner_loop",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var i = 0;
    while i < 4 {
        var k = 0;
        while k < 1 { i = i + 1; k = k + 1; }
        a[i] = 1;
    }
    print(a[0]);
}
"#,
        ),
        (
            "or_guard_is_not_a_bound",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var i = 0;
    var spin = 0;
    while i < 10 || spin < 100 { a[i] = 1; i = i + 1; spin = spin + 1; }
    print(a[0]);
}
"#,
        ),
        (
            "trunc_negative_iv_init",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var i: i32 = 2147483648 as i32;
    while i < 10 { a[i] = 1; i = i + 1; }
    print(a[0]);
}
"#,
        ),
        (
            "affine_sub_keeps_bounds",
            r#"
fn main() {
    var a: [i32] = [0; 50];
    var n = 10;
    var i = 6;
    var j = 5;
    print(a[n * i - j]);
}
"#,
        ),
        (
            "affine_nested_loops",
            r#"
fn main() {
    var n = 24;
    var a: [i32] = [0; 576];
    for i in 0..n {
        for j in 0..n {
            a[i * n + j] = i * 100 + j;
        }
    }
    var t = 0;
    for i in 0..n { for j in 0..n { t = t + a[i * n + j]; } }
    print(t);
}
"#,
        ),
        (
            "saxpy_idiom",
            r#"
fn main() {
    var c: [i32] = [0; 10];
    var b: [i32] = [0; 10];
    for i in 0..10 { b[i] = i + 1; }
    var av = 3;
    var j = 0;
    while j < 10 {
        c[j] = c[j] + av * b[j];
        j = j + 1;
    }
    for i in 0..10 { print(c[i]); }
    print(j);
    var k = 7;
    while k < 5 {
        c[k] = c[k] + av * b[k];
        k = k + 1;
    }
    print(k);
}
"#,
        ),
        (
            "saxpy_idiom_oob",
            r#"
fn main() {
    var c: [i32] = [0; 4];
    var b: [i32] = [0; 4];
    var j = 0;
    while j < 8 {
        c[j] = c[j] + 2 * b[j];
        j = j + 1;
    }
    print(c[0]);
}
"#,
        ),
        (
            "scalarize_const_arrays",
            r#"
fn main() {
    var a: [i32] = [7; 4];
    var b: [f64] = [0.0; 3];
    a[0] = a[1] + 5;
    a[2] = a[0] * 2;
    b[0] = 1.5; b[1] = b[0] * 2.0; b[2] = b[1] + b[0];
    print(a[0]); print(a[2]); print(a[3]);
    print_prec(b[0] + b[1] + b[2], 3);
}
"#,
        ),
        (
            "const_small_loop_unroll",
            r#"
fn main() {
    var a: [i32] = [0; 8];
    for i in 0..4 { a[i] = i * 2; }
    var t = 0;
    for i in 0..4 { t = t + a[i]; }
    for i in 0..8 { if i == 3 { break; } a[i] = a[i] + i; }
    print(t); print(a[0]); print(a[3]); print(a[7]);
}
"#,
        ),
        (
            "const_array_len_params",
            r#"
fn sum(a: [i32], n: i32) -> i32 {
    if n <= 0 { return 0; }
    return a[n - 1] + sum(a, n - 1);
}
fn main() {
    var a: [i32] = [0; 8];
    for i in 0..8 { a[i] = i + 1; }
    print(sum(a, 8));
}
"#,
        ),
        (
            "aos_pair_checked_oob",
            r#"
struct P { x: f64, y: f64, vx: f64, vy: f64 }
fn main() {
    var ps: [P+] = [];
    reserve(ps, 2);
    ps.push(P { x: 0.0, y: 0.0, vx: 1.0, vy: 0.25 });
    ps.push(P { x: 1.0, y: 0.5, vx: 1.0, vy: 0.25 });
    var i = 5;
    ps[i].x = ps[i].x + ps[i].vx;
    ps[i].y = ps[i].y + ps[i].vy;
    ps[i].vx = ps[i].vx * 0.5;
    ps[i].vy = ps[i].vy * 0.5;
    print_prec(ps[0].x, 3);
}
"#,
        ),
        (
            "aos_pair_simd",
            r#"
struct P { x: f64, y: f64, vx: f64, vy: f64 }
fn main() {
    var ps: [P] = [P { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0 }; 4];
    for i in 0..4 {
        ps[i].x = i as f64;
        ps[i].y = i as f64 * 0.5;
        ps[i].vx = 1.0;
        ps[i].vy = 0.25;
    }
    for s in 0..3 {
        for i in 0..4 {
            ps[i].x = ps[i].x + ps[i].vx;
            ps[i].y = ps[i].y + ps[i].vy;
            ps[i].vx = ps[i].vx * 0.5;
            ps[i].vy = ps[i].vy * 0.5;
        }
        print_prec(ps[0].x, 3); print_prec(ps[0].y, 3);
        print_prec(ps[0].vx, 3); print_prec(ps[0].vy, 3);
    }
}
"#,
        ),
        (
            "par_for_struct_fields",
            r#"
struct P { x: f64, y: f64 }
fn main() {
    var ps: [P] = [P { x: 0.0, y: 0.0 }; 1000];
    par for i in 0..1000 {
        ps[i].x = i as f64;
        ps[i].y = ps[i].x * 2.0;
    }
    var t = 0.0;
    for i in 0..1000 { t = t + ps[i].x + ps[i].y; }
    print_prec(t, 1);
}
"#,
        ),
        (
            "par_for_int_and_scalar",
            r#"
fn main() {
    var a: [i32] = [0; 20000];
    var k = 3;
    par for i in 0..20000 {
        a[i] = i * k + 7;
    }
    var t: i64 = 0;
    for i in 0..20000 { t = t + a[i]; }
    print(t);
    print(a[0]); print(a[19999]);
}
"#,
        ),
        (
            "par_for_bool_sieve",
            r#"
fn main() {
    let n = 1000; let half = n / 2; let root = 32;
    var composite: [bool] = [false; half];
    par for i in 1..root / 2 {
        if !composite[i] {
            let p = 2 * i + 1;
            var j = (p * p) / 2;
            while j < half { composite[j] = true; j = j + p; }
        }
    }
    var count = 1; var k = 1;
    while k < half { if !composite[k] { count = count + 1; } k = k + 1; }
    print(count);
}
"#,
        ),
        (
            "break_continue_loops",
            r#"
fn main() {
    var i = 0; var t = 0;
    while i < 20 {
        i = i + 1;
        if i == 7 { break; }
        if i % 2 == 0 { continue; }
        t = t + i;
    }
    print(i); print(t);
    var s = 0;
    for k in 0..10 {
        if k % 3 == 0 { continue; }
        if k == 8 { break; }
        s = s + k;
    }
    print(s);
    var n = 0; var a = 0;
    while a < 5 {
        a = a + 1;
        var b = 0;
        while b < 5 {
            b = b + 1;
            if b == 3 { break; }
            n = n + 1;
        }
    }
    print(n);
}
"#,
        ),
        (
            "bool_array_param",
            r#"
fn count(b: [bool], n: i32) -> i32 {
    var i = 0; var c = 0;
    while i < n { if b[i] { c = c + 1; } i = i + 1; }
    return c;
}
fn main() {
    var b: [bool] = [false; 8];
    b[1] = true; b[5] = true;
    print(count(b, 8));
}
"#,
        ),
        (
            "i64_index_not_narrowed",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var y: i64 = 4294967296;
    var i = 0;
    while i < 1 { a[y + 2] = 7; i = i + 1; }
    print(a[2]);
}
"#,
        ),
        (
            "i32_index_overflow_wraps",
            r#"
fn main() {
    var a: [i32] = [0; 4];
    var i: i32 = 2147483647;
    i = i + 2;
    a[i] = 7;
    print(a[0]);
}
"#,
        ),
        (
            "vec_param_read",
            r#"
fn sum(v: [i32+]) -> i32 {
    var i = 0; var t = 0;
    while i < len(v) { t = t + v[i]; i = i + 1; }
    return t;
}
fn main() {
    var v: [i32+] = [];
    v.push(1); v.push(2); v.push(3);
    print(sum(v));
    print(len(v));
}
"#,
        ),
        (
            "vec_param_push",
            r#"
fn grow(v: [i32+], x: i32) { v.push(x); v.push(x + 1); }
fn main() {
    var v: [i32+] = [];
    v.push(1); v.push(2);
    grow(v, 10);
    print(len(v));
    print(v[2]); print(v[3]);
}
"#,
        ),
        (
            "vec_param_nested",
            r#"
fn grow(v: [i32+], x: i32) { v.push(x); v.push(x + 1); }
fn outer(v: [i32+], x: i32) -> i32 {
    grow(v, x);
    var n = len(v);
    return v[n - 3] + v[n - 2] + v[n - 1];
}
fn main() {
    var v: [i32+] = [];
    v.push(1); v.push(2);
    print(outer(v, 10));
    print(len(v));
    print(v[2]); print(v[3]);
}
"#,
        ),
        (
            "struct_params_read_only",
            r#"
struct P { x: i32, y: i32, z: i64 }
fn sum(p: P) -> i32 { return p.x + p.y + (p.z as i32); }
fn twice(p: P) -> i32 { return sum(p) * 2; }
fn main() {
    var q = P { x: 3, y: 4, z: 5 };
    print(sum(q));
    print(twice(q));
    print(q.x);
}
"#,
        ),
        (
            "struct_param_float",
            r#"
struct V { a: f64, b: f64 }
fn total(v: V) -> f64 { return v.a + v.b; }
fn main() {
    var v = V { a: 1.5, b: 2.5 };
    print_prec(total(v), 2);
    print_prec(v.b, 2);
}
"#,
        ),
    ];
    let mut fails = Vec::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut corpus: Vec<(String, String)> = cases
        .iter()
        .map(|(n, s)| (n.to_string(), s.to_string()))
        .collect();
    for ex in ["fib", "for", "sum", "array_param", "sieve"] {
        let src = std::fs::read_to_string(root.join(format!("examples/{ex}.magic"))).unwrap();
        corpus.push((format!("example_{ex}"), src));
    }
    for bench in ["matmul_magic", "raytrace_magic", "particles_magic", "vecpart_magic", "nbody_magic"] {
        let src = std::fs::read_to_string(root.join(format!("bench-suite/{bench}.magic"))).unwrap();
        corpus.push((format!("bench_{bench}"), src));
    }
    for db in ["btree_magic", "hashmap_magic", "sqlscan_magic"] {
        let src = std::fs::read_to_string(root.join(format!("bench-db/{db}.magic"))).unwrap();
        corpus.push((format!("db_{db}"), src));
    }
    for (name, src) in &corpus {
        let c = run(src, &dir, &format!("{name}_c"), false, &cc_cmd);
        let i = run(src, &dir, &format!("{name}_i"), true, &cc_cmd);
        match (c, i) {
            (Some((co, cc)), Some((io, ic))) if co == io && cc == ic => {}
            (Some((co, cc)), Some((io, ic))) => fails.push(format!(
                "{}: C {:?} vs IR {:?}",
                name,
                (co, cc),
                (io, ic)
            )),
            _ => fails.push(format!("{}: build/run failed", name)),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(fails.is_empty(), "IR/C mismatch:\n{}", fails.join("\n"));
}

#[test]
fn ir_tcp_roundtrip_and_connect() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return;
    }
    let Some(cc_cmd) = cc() else { return };
    let dir = std::env::temp_dir().join(format!("magic_ir_tcp_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let build = |src: &str, name: &str| -> Option<std::path::PathBuf> {
        let asm = magic::compile_to_asm_ir(src).ok()?;
        let sp = dir.join(format!("{name}.s"));
        std::fs::write(&sp, asm).ok()?;
        let op = dir.join(format!("{name}.o"));
        if !Command::new("as").arg("--64").arg("-o").arg(&op).arg(&sp).status().ok()?.success() {
            return None;
        }
        let exe = dir.join(name);
        if !Command::new(&cc_cmd).arg(&op).arg("-o").arg(&exe).status().ok()?.success() {
            return None;
        }
        Some(exe)
    };
    let server_src = r#"
fn main() {
    var lfd = net_listen(0);
    print(net_port(lfd));
    flush();
    var cfd = net_accept(lfd);
    var buf: [u8] = [0; 16];
    var n = net_read_at(cfd, buf, 2, 4);
    print(n);
    var out: [u8] = [0; 4];
    out[0] = 112; out[1] = 111; out[2] = 110; out[3] = 103;
    net_write_at(cfd, out, 0, 4);
    net_close(cfd);
    net_close(lfd);
}
"#;
    let Some(server) = build(server_src, "server") else { return };
    let mut child = Command::new(&server)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn server");
    let mut reader = BufReader::new(child.stdout.take().expect("stdout"));
    let mut line = String::new();
    reader.read_line(&mut line).expect("read port");
    let port: u16 = line.trim().parse().expect("port number");
    let client_src = format!(
        r#"
fn main() {{
    var fd = net_connect("127.0.0.1", {port});
    if fd < 0 {{ print(-1); return; }}
    var out: [u8] = [0; 4];
    out[0] = 112; out[1] = 105; out[2] = 110; out[3] = 103;
    net_write(fd, out, 4);
    var buf: [u8] = [0; 4];
    var n = net_read_at(fd, buf, 0, 4);
    print(n);
    print(buf[0]);
    net_close(fd);
}}
"#
    );
    let Some(client) = build(&client_src, "client") else { return };
    let out = Command::new(&client).output().expect("run client");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.starts_with("4\n112"), "client output: {text:?}");
    let mut reply = String::new();
    reader.read_line(&mut reply).expect("server n");
    assert_eq!(reply.trim(), "4");
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ir_panic_reports_file_line_and_stack_trace() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return;
    }
    let Some(cc_cmd) = cc() else { return };
    let dir = std::env::temp_dir().join(format!("magic_ir_trace_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = r#"
fn inner(i: i32) -> i32 { var a: [i32] = [0; 2]; return a[i]; }
fn middle(x: i32) -> i32 { return inner(x) + 1; }
fn main() { print(middle(5)); }
"#;
    let asm = magic::compile_to_asm_ir(src).expect("compile");
    let sp = dir.join("t.s");
    std::fs::write(&sp, asm).expect("write asm");
    let op = dir.join("t.o");
    assert!(Command::new("as").arg("--64").arg("-o").arg(&op).arg(&sp).status().expect("as").success());
    let exe = dir.join("t");
    assert!(Command::new(&cc_cmd)
        .arg("-rdynamic")
        .arg(&op)
        .arg("-o")
        .arg(&exe)
        .status()
        .expect("cc")
        .success());
    let out = Command::new(&exe).output().expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "exit status: {:?}", out.status);
    assert!(out.stdout.is_empty(), "stdout: {:?}", String::from_utf8_lossy(&out.stdout));
    assert!(stderr.contains("index out of bounds") && stderr.contains("line "), "stderr: {stderr}");
    for frame in ["inner", "middle", "main"] {
        assert!(stderr.contains(frame), "missing frame {frame} in:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn c_panic_reports_file_line_and_symbols() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    let Some(cc_cmd) = cc() else { return };
    let dir = std::env::temp_dir().join(format!("magic_c_trace_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = r#"
fn inner(i: i32) -> i32 { var a: [i32] = [0; 1]; return a[i]; }
fn middle(x: i32) -> i32 { return inner(x) + 1; }
fn main() { print(middle(argc())); print(0); }
"#;
    let c = magic::compile_to_c(src).expect("compile");
    let cp = dir.join("t.c");
    std::fs::write(&cp, c).expect("write c");
    let exe = dir.join("t");
    assert!(Command::new(&cc_cmd)
        .arg("-O3")
        .arg("-rdynamic")
        .arg("-o")
        .arg(&exe)
        .arg(&cp)
        .status()
        .expect("cc")
        .success());
    let out = Command::new(&exe).output().expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr.contains("index out of bounds"), "stderr: {stderr}");
    assert!(stderr.contains("main+"), "stderr: {stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
