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
        ("arith", "fn main() { print(1 + 2 * 3); print(100 / 7); print(100 % 7); print(-5 + 2); }"),
        ("loop", "fn main() { var t = 0; var i = 0; while i < 10 { t = t + i; i = i + 1; } print(t); }"),
        ("forloop", "fn main() { var t = 0; for j in 0..7 { t = t + j * 2; } print(t); }"),
        ("ifelse", "fn main() { var x = 5; if x < 3 { print(1); } else { print(2); } if x >= 5 { print(3); } }"),
        ("fib", "fn fib(n: i32) -> i32 { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); } fn main() { print(fib(12)); }"),
        ("calls", "fn add(a: i32, b: i32) -> i32 { return a + b; } fn main() { print(add(20, 22)); }"),
        ("casts", "fn main() { print(3 as i32); print(250 as u8); print(0 as bool); print(2 as bool); }"),
        ("boolops", "fn main() { print(1 < 2); print(1 == 2); print(1 <= 1); print(2 > 3); }"),
        ("nested", "fn main() { var t = 0; for i in 0..4 { for j in 0..3 { t = t + i * j; } } print(t); }"),
        ("divmod", "fn main() { var a = 12345; print(a / 10); print(a % 10); print(a / -3); print(a % -3); }"),
        ("floats", "fn main() { var x: f64 = 1.5; var y = 2.0; print(x + y); print(x * y); print(x / y); print(x - y); print(x < y); print(x == y); print(sqrt(16.0)); print_prec(x * y, 3); var a: [f64] = [0.0; 4]; a[2] = 3.25; print(a[2]); print(-x); print(x as i32); print(3 as f64); var t = 0.0; var i = 0; while i < 5 { t = t + (i as f64) * 0.5; i = i + 1; } print(t); }"),
        ("immops", "fn main() { var a = 10; var b = 3; print(5 - b); print(5 < b); print(5 > b); print(5 <= 5); print(5 == a); print(a + 7); print(a * 3); print(a - 4); print(a / 2); print(a % 3); print(1000 - a); print(b - 10); }"),
        ("fill1", "fn main() { var a: [i32] = [7; 1]; print(a[0]); }"),
        ("fill3", "fn main() { var a: [i32] = [7; 3]; print(a[0] + a[1] + a[2]); }"),
        ("fill5", "fn main() { var a: [i32] = [7; 5]; var t = 0; var i = 0; while i < 5 { t = t + a[i]; i = i + 1; } print(t); }"),
        ("fill9u8", "fn main() { var a: [u8] = [3; 9]; var t = 0; var i = 0; while i < 9 { t = t + (a[i] as i32); i = i + 1; } print(t); }"),
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
    let mm = std::fs::read_to_string(root.join("bench-suite/matmul_magic.magic")).unwrap();
    corpus.push(("matmul".to_string(), mm));
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
