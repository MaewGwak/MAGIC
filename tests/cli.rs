// Tests that run the actual `magicc` binary, so argument parsing, the build cache,
// chunking and the `--report` path are exercised end to end.
use std::path::{Path, PathBuf};
use std::process::Command;

fn magicc() -> &'static str {
    env!("CARGO_BIN_EXE_magicc")
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// A temp directory that deletes itself when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("magic_cli_{}_{}", std::process::id(), name));
        std::fs::create_dir_all(&dir).expect("temp dir");
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// True when a C compiler is actually available: probes by building a trivial program.
fn has_compiler() -> bool {
    let dir = TempDir::new("probe");
    let src = dir.path().join("p.magic");
    if std::fs::write(&src, "fn main() { print(1); }\n").is_err() {
        return false;
    }
    Command::new(magicc())
        .args(["build", src.to_str().unwrap()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn version_flag_prints_the_package_version() {
    let out = Command::new(magicc())
        .arg("--version")
        .output()
        .expect("run magicc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "stdout was: {text}");
}

#[test]
fn help_flag_prints_usage() {
    let out = Command::new(magicc()).arg("--help").output().expect("run magicc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("usage: magicc"), "stdout was: {text}");
    assert!(text.contains("--report"), "stdout was: {text}");
}

#[test]
fn check_accepts_a_valid_program() {
    let out = Command::new(magicc())
        .arg("check")
        .arg(examples().join("hello.magic"))
        .output()
        .expect("run magicc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("ok"), "stdout was: {text}");
}

#[test]
fn check_rejects_a_missing_file() {
    let out = Command::new(magicc())
        .args(["check", "definitely_not_here.magic"])
        .output()
        .expect("run magicc");
    assert!(!out.status.success());
}

#[test]
fn check_reports_the_memory_bound() {
    let dir = TempDir::new("report");
    let src = dir.path().join("r.magic");
    std::fs::write(&src, "fn main() { var a: [i32] = [0; 10]; }\n").unwrap();
    let out = Command::new(magicc())
        .args(["check", src.to_str().unwrap(), "--report"])
        .output()
        .expect("run magicc");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("memory") && text.contains("40"),
        "stdout was: {text}"
    );
}

#[test]
fn builds_several_source_files_as_one_program() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("multi");
    let lib = dir.path().join("lib.magic");
    let main = dir.path().join("main.magic");
    std::fs::write(&lib, "fn helper() -> i32 { return 7; }\n").unwrap();
    std::fs::write(&main, "fn main() { print(helper() * 6); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", main.to_str().unwrap(), lib.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = if cfg!(windows) {
        dir.path().join("main.exe")
    } else {
        dir.path().join("main.bin")
    };
    let run = Command::new(&exe).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
}

#[test]
fn report_shows_arena_rows() {
    let dir = TempDir::new("reportarena");
    let src = dir.path().join("a.magic");
    std::fs::write(
        &src,
        "fn main() {\narena {\nvar a: [i32] = [0; 4];\n}\n}\n",
    )
    .unwrap();
    let out = Command::new(magicc())
        .args(["check", src.to_str().unwrap(), "--report"])
        .output()
        .expect("run magicc");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("arena at line"), "stdout was: {text}");
    assert!(text.contains("16"), "stdout was: {text}");
}

#[test]
fn report_counts_struct_arrays() {
    let dir = TempDir::new("reportstruct");
    let src = dir.path().join("s.magic");
    std::fs::write(
        &src,
        "struct P { x: i32, y: i32 }\nfn main() { var a: [P] = [P { x: 0, y: 0 }; 10]; }\n",
    )
    .unwrap();
    let out = Command::new(magicc())
        .args(["check", src.to_str().unwrap(), "--report"])
        .output()
        .expect("run magicc");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(text.contains("80"), "stdout was: {text}");
}

#[test]
fn emit_prints_c() {
    let out = Command::new(magicc())
        .args(["emit", examples().join("hello.magic").to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("int main(void)"), "stdout was: {text}");
}

#[test]
fn build_creates_an_executable_that_runs() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("build");
    let src = dir.path().join("b.magic");
    std::fs::write(&src, "fn main() { print(21 * 2); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", src.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = if cfg!(windows) {
        dir.path().join("b.exe")
    } else {
        dir.path().join("b.bin")
    };
    assert!(exe.exists(), "expected {}", exe.display());
    let run = Command::new(&exe).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
}

#[test]
fn second_build_is_up_to_date() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("cache");
    let src = dir.path().join("c.magic");
    std::fs::write(&src, "fn main() { print(1); }\n").unwrap();
    let first = Command::new(magicc())
        .args(["build", src.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(first.status.success());
    let second = Command::new(magicc())
        .args(["build", src.to_str().unwrap()])
        .output()
        .expect("run magicc");
    let text = String::from_utf8_lossy(&second.stdout);
    assert!(text.contains("up to date"), "stdout was: {text}");
}

#[test]
fn dev_flag_builds() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("dev");
    let src = dir.path().join("d.magic");
    std::fs::write(&src, "fn main() { print(3); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", src.to_str().unwrap(), "--dev"])
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn imports_are_inlined() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("import");
    std::fs::write(
        dir.path().join("lib.magic"),
        "fn helper() -> i32 { return 9; }\n",
    )
    .unwrap();
    let main = dir.path().join("main.magic");
    std::fs::write(
        &main,
        "import \"lib.magic\";\nfn main() { print(helper() * 2); }\n",
    )
    .unwrap();
    let out = Command::new(magicc())
        .args(["build", main.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = if cfg!(windows) {
        dir.path().join("main.exe")
    } else {
        dir.path().join("main.bin")
    };
    let run = Command::new(&exe).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "18");
}

#[test]
fn missing_import_fails() {
    let dir = TempDir::new("importmissing");
    let main = dir.path().join("m.magic");
    std::fs::write(&main, "import \"nope.magic\";\nfn main() { print(1); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", main.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cannot read"), "stderr: {err}");
}

#[test]
fn errors_point_at_the_imported_file() {
    let dir = TempDir::new("importdiag");
    std::fs::write(
        dir.path().join("lib.magic"),
        "fn helper() -> i32 { return 1; }\nfn bad() -> i32 { return \"x\"; }\n",
    )
    .unwrap();
    let main = dir.path().join("main.magic");
    std::fs::write(
        &main,
        "import \"lib.magic\";\nfn main() { print(helper()); }\n",
    )
    .unwrap();
    let out = Command::new(magicc())
        .args(["build", main.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("lib.magic:2:"), "stderr: {err}");
}

#[test]
fn duplicate_function_names_the_file() {
    let dir = TempDir::new("dup");
    std::fs::write(
        dir.path().join("a.magic"),
        "fn helper() -> i32 { return 1; }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("b.magic"),
        "fn helper() -> i32 { return 2; }\n",
    )
    .unwrap();
    let main = dir.path().join("main.magic");
    std::fs::write(
        &main,
        "import \"a.magic\";\nimport \"b.magic\";\nfn main() { print(helper()); }\n",
    )
    .unwrap();
    let out = Command::new(magicc())
        .args(["build", main.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("b.magic:1: duplicate function 'helper'"),
        "stderr: {err}"
    );
}

#[test]
fn output_flag_names_the_binary() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("outflag");
    let src = dir.path().join("o.magic");
    std::fs::write(&src, "fn main() { print(11); }\n").unwrap();
    let out_name = if cfg!(windows) { "custom.exe" } else { "custom.bin" };
    let out_path = dir.path().join(out_name);
    let out = Command::new(magicc())
        .args(["build", src.to_str().unwrap(), "-o", out_path.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out_path.exists());
    let run = Command::new(&out_path).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "11");
}

#[test]
fn cc_flag_overrides_cc_env() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("ccflag");
    let src = dir.path().join("f.magic");
    std::fs::write(&src, "fn main() { print(6 * 7); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", src.to_str().unwrap(), "--cc", "gcc -O1"])
        .env("CC", "definitely-not-a-compiler")
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cc_env_may_include_arguments() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("ccargs");
    let src = dir.path().join("a.magic");
    std::fs::write(&src, "fn main() { print(6 * 7); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", src.to_str().unwrap()])
        .env("CC", "gcc -O1")
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = if cfg!(windows) {
        dir.path().join("a.exe")
    } else {
        dir.path().join("a.bin")
    };
    let run = Command::new(&exe).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
}
