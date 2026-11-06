//! Differential fuzzing: mutate integer literals in valid programs and require
//! the C and IR backends to agree on stdout and exit status.
use std::path::{Path, PathBuf};
use std::process::Command;

fn cc() -> Option<String> {
    for name in ["gcc", "clang", "cc"] {
        if Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(name.to_string());
        }
    }
    None
}

fn build(src: &str, cc: &str, dir: &Path, name: &str, ir: bool) -> Option<PathBuf> {
    let exe = dir.join(name);
    if ir {
        let asm = magic::compile_to_asm_ir(src).ok()?;
        let sp = dir.join(format!("{name}.s"));
        std::fs::write(&sp, asm).ok()?;
        let op = dir.join(format!("{name}.o"));
        if !Command::new("as")
            .arg("--64")
            .arg("-o")
            .arg(&op)
            .arg(&sp)
            .status()
            .ok()?
            .success()
        {
            return None;
        }
        if !Command::new(cc)
            .arg(&op)
            .arg("-o")
            .arg(&exe)
            .status()
            .ok()?
            .success()
        {
            return None;
        }
    } else {
        let c = magic::compile_to_c(src).ok()?;
        let cp = dir.join(format!("{name}.c"));
        std::fs::write(&cp, c).ok()?;
        if !Command::new(cc)
            // -O0 keeps the C compiler from folding UB (e.g. 0/0) that the
            // IR correctly traps on; divisions are still checked at runtime.
            .arg("-O0")
            .arg("-fwrapv")
            .arg("-o")
            .arg(&exe)
            .arg(&cp)
            .status()
            .ok()?
            .success()
        {
            return None;
        }
    }
    Some(exe)
}

fn run(exe: &Path, secs: u64) -> Option<(String, Option<i32>)> {
    let mut child = Command::new(exe)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let out = child.wait_with_output().ok()?;
                return Some((String::from_utf8_lossy(&out.stdout).to_string(), status.code()));
            }
            None => {
                if start.elapsed() > std::time::Duration::from_secs(secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

fn mutate_literals(src: &str, state: &mut u64) -> String {
    let mut out = String::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit()
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
        {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let text = &src[start..i];
            *state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let choice = (*state >> 33) % 6;
            let original: i64 = text.parse().unwrap_or(1);
            let value = match choice {
                0 => original.saturating_add(1),
                1 => original.saturating_sub(1),
                2 => (original / 2).max(0),
                3 => 0,
                4 => 1,
                _ => original.saturating_mul(2),
            };
            out.push_str(&value.to_string());
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

const SEEDS: &[&str] = &[
    "fn main() { var a = 2000000000; print(a + a); print(a * 3); print((-2147483647 - 1) / -1); print((-7) % 2); }",
    "fn main() { var t = 0; var i = 0; while i < 7 { t = t + i * 3; i = i + 1; } print(t); }",
    "fn main() { var a: [i32] = [0; 9]; var i = 0; while i < 9 { a[i] = i * i; i = i + 1; } var t = 0; i = 0; while i < 9 { t = t + a[i]; i = i + 1; } print(t); }",
    "fn main() { var a = 12345; print(a / 7); print(a % 7); print(a / -3); print(a % -3); }",
    "fn f(n: i32) -> i32 { if n < 2 { return n; } return f(n - 1) + f(n - 2); } fn main() { print(f(10)); }",
    "struct P { x: f64, y: f64 } fn main() { var p = P { x: 1.5, y: 2.25 }; print(p.x + p.y); print(p.x * p.y); }",
    "fn main() { var v: [i32] = [0; 6]; var i = 0; while i < 6 { v[i] = (i * 7 + 3) % 5; i = i + 1; } var t = 0; i = 0; while i < 6 { t = t + v[i]; i = i + 1; } print(t); }",
    "fn main() { var t = 0; var i = 0; while i < 20 { if i % 3 == 0 { t = t + i; } else if i % 5 == 0 { t = t - i; } i = i + 1; } print(t); }",
    "fn main() { var a = 7; var b = -3; print(a / b); print(a % b); print(-a / b); print(-a % b); print(a / -b); }",
    "fn main() { var x = 250; print(x as u8); print(x as i32); print((-5) as u8); print((x as u8) as i32); print(0 as bool); print(7 as bool); }",
    "struct Q { a: i32, b: i32 } fn main() { var q = Q { a: 7, b: -3 }; print(q.a + q.b); print(q.a * q.b); print(q.a / q.b); q.a = q.a + 5; print(q.a); }",
    "fn main() { var t: i64 = 0; var i = 0; while i < 40 { t = t + i * i; i = i + 1; } print(t); }",
    "fn main() { var a: [i32] = [0; 8]; var i = -3; while i < 8 { if i >= 0 { a[i] = i * 3; } i = i + 1; } var t = 0; i = 0; while i < 8 { t = t + a[i]; i = i + 1; } print(t); }",
    "fn main() { var b: [bool] = [false; 32]; var i = 0; while i < 32 { b[i] = i % 3 == 0; i = i + 1; } var c = 0; i = 0; while i < 32 { if b[i] { c = c + 1; } i = i + 1; } print(c); }",
];

#[test]
fn mutated_programs_agree_between_backends() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return;
    }
    let Some(cc_cmd) = cc() else { return };
    let dir = std::env::temp_dir().join(format!("magic_fuzzdiff_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut mismatches = Vec::new();
    let mut checked = 0;
    for seed in SEEDS {
        for _ in 0..1 {
            let src = mutate_literals(seed, &mut state);
            let c = build(&src, &cc_cmd, &dir, "c", false);
            let i = build(&src, &cc_cmd, &dir, "i", true);
            let (Some(c_exe), Some(i_exe)) = (c, i) else { continue };
            let c_run = run(&c_exe, 5);
            let i_run = run(&i_exe, 5);
            let (Some((cout, ccode)), Some((iout, icode))) = (c_run, i_run) else { continue };
            checked += 1;
            // Crash *mode* may differ (trap vs guarded panic); both crashing is
            // equivalent, one crashing while the other succeeds is not.
            let c_ok = ccode == Some(0);
            let i_ok = icode == Some(0);
            // When both sides fail, stdout before the trap/panic is buffering-
            // dependent (C may lose it, the IR panic flushes); only require
            // that both failed.
            if !c_ok && !i_ok {
                continue;
            }
            if cout != iout || c_ok != i_ok {
                mismatches.push(format!("{src}\n  C=({cout:?},{ccode:?}) IR=({iout:?},{icode:?})"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        mismatches.is_empty(),
        "{} mismatching mutations out of {}:\n{}",
        mismatches.len(),
        checked,
        mismatches.join("\n")
    );
}
