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
    var n = net_read(cfd, buf, 4);
    print(n);
    var out: [u8] = [0; 4];
    out[0] = 112; out[1] = 111; out[2] = 110; out[3] = 103;
    net_write(cfd, out, 4);
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
    var n = net_read(fd, buf, 4);
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
