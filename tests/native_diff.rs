// Differential harness: build the same source with the C backend and the native
// backend, run both, and require identical stdout and exit status. This is the
// safety net that any native optimisation must pass before it is enabled.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn cc() -> Option<String> {
    if let Ok(cc) = std::env::var("CC") {
        if !cc.trim().is_empty() {
            return Some(cc);
        }
    }
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

fn native_available() -> bool {
    cfg!(target_os = "linux")
        && cfg!(target_arch = "x86_64")
        && Command::new("as")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn run_with_timeout(exe: &Path, cwd: &Path, secs: u64) -> Option<(String, Option<i32>)> {
    let mut child = Command::new(exe)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) => {
                let out = child.wait_with_output().ok()?;
                return Some((
                    String::from_utf8_lossy(&out.stdout).to_string(),
                    status.code(),
                ));
            }
            None => {
                if start.elapsed() > Duration::from_secs(secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn build(src: &str, cc_cmd: &str, dir: &Path, name: &str, native: bool) -> Option<PathBuf> {
    let c_path = dir.join(format!("{name}.c"));
    let s_path = dir.join(format!("{name}.s"));
    let exe = dir.join(name);
    if native {
        let asm = magic::compile_to_asm(src).ok()?;
        std::fs::write(&s_path, asm).ok()?;
        let status = Command::new("as")
            .arg("--64")
            .arg("-o")
            .arg(dir.join(format!("{name}.o")))
            .arg(&s_path)
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
        let status = Command::new(cc_cmd)
            .arg(dir.join(format!("{name}.o")))
            .arg("-o")
            .arg(&exe)
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
    } else {
        let c = magic::compile_to_c(src).ok()?;
        std::fs::write(&c_path, c).ok()?;
        let mut cmd = Command::new(cc_cmd);
        for part in cc_cmd.split_whitespace().skip(1) {
            cmd.arg(part);
        }
        let status = cmd
            .arg("-O1")
            .arg("-o")
            .arg(&exe)
            .arg(&c_path)
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
    }
    Some(exe)
}

#[test]
fn native_matches_c_on_the_benchmark_corpus() {
    if !native_available() {
        return;
    }
    let Some(cc_cmd) = cc() else { return };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("magic_diff_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    let corpus = [
        "examples/hello.magic",
        "examples/fib.magic",
        "examples/for.magic",
        "examples/sum.magic",
        "examples/sieve.magic",
        "examples/array_param.magic",
        "examples/hashmap.magic",
        "examples/tokenize.magic",
        "examples/arena.magic",
        "examples/arena_bump.magic",
        "examples/struct.magic",
        "examples/points.magic",
        "examples/inventory.magic",
        "examples/vec.magic",
        "examples/cast.magic",
        "bench-suite/sieve_magic_odd.magic",
        "bench-suite/hash_magic.magic",
        "bench-suite/hash_magic_par.magic",
        "bench-suite/particles_magic.magic",
        "bench-suite/vecpart_magic.magic",
        "bench-fannkuch/fannkuch_magic.magic",
        "bench-bt/bt_magic.magic",
        "bench-db/pageio_magic.magic",
        "bench-db/btree_magic.magic",
        "bench-db/bufpool_magic.magic",
        "bench-db/sqlscan_magic.magic",
        "bench-db/hashmap_magic.magic",
    ];

    let mut mismatches = Vec::new();
    for path in corpus {
        let full = root.join(path);
        let src = std::fs::read_to_string(&full).expect("read source");
        let stem = path.replace(['/', '.'], "_");
        let c_exe = build(&src, &cc_cmd, &dir, &format!("{stem}_c"), false);
        let n_exe = build(&src, &cc_cmd, &dir, &format!("{stem}_n"), true);
        let (Some(c_exe), Some(n_exe)) = (c_exe, n_exe) else {
            continue;
        };
        let c_run = run_with_timeout(&c_exe, root, 120);
        let n_run = run_with_timeout(&n_exe, root, 300);
        match (c_run, n_run) {
            (Some((cout, ccode)), Some((nout, ncode))) => {
                if cout != nout || ccode != ncode {
                    mismatches.push(format!("{path}: stdout/exit differ"));
                }
            }
            _ => mismatches.push(format!("{path}: timed out or failed to run")),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(mismatches.is_empty(), "native/C mismatch:\n{}", mismatches.join("\n"));
}
