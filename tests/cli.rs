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
fn version_flags_are_interchangeable() {
    let expected = format!("magicc {}", env!("CARGO_PKG_VERSION"));
    for flag in ["--version", "-V", "-v", "-version", "version"] {
        let out = Command::new(magicc())
            .arg(flag)
            .output()
            .expect("run magicc");
        assert!(out.status.success(), "{flag} failed");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), expected);
    }
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
        text.contains("memory") && text.contains("40") && text.contains("NOT peak RSS"),
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
        .args(["--backend", "c", "emit", examples().join("hello.magic").to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("int main(int magic_argc, char **magic_argv)"), "stdout was: {text}");
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
        .args(["--backend", "c", "build", src.to_str().unwrap(), "--dev"])
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
fn unroll_warns_for_gcc() {
    let gcc = Command::new("gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !gcc {
        return;
    }
    let dir = TempDir::new("unrollwarn");
    let src = dir.path().join("w.magic");
    std::fs::write(&src, "fn main() { var s = 0; for i in 0..3 { s = s + i; } print(s); }\n")
        .unwrap();
    let out = Command::new(magicc())
        .args(["--backend", "c", "build", src.to_str().unwrap(), "--unroll"])
        .output()
        .expect("run magicc");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("note: --unroll"), "stderr: {err}");
}

#[test]
fn unroll_flag_emits_straight_line_and_runs() {
    let dir = TempDir::new("unroll");
    let src = dir.path().join("u.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         var sum = 0;\n\
         for i in 0..4 { sum = sum + i; }\n\
         print(sum);\n\
         }\n",
    )
    .unwrap();
    let emitted = Command::new(magicc())
        .args(["--backend", "c", "emit", src.to_str().unwrap(), "--unroll"])
        .output()
        .expect("run magicc");
    let text = String::from_utf8_lossy(&emitted.stdout);
    assert!(
        !text.contains("for (int32_t i ="),
        "loop should be unrolled: {text}"
    );
    assert!(text.contains("const int32_t i = 0;"), "stdout: {text}");

    // break binds to a real loop, so that loop must stay a loop
    let src2 = dir.path().join("b.magic");
    std::fs::write(
        &src2,
        "fn main() {\n\
         var sum = 0;\n\
         for i in 0..4 { if i == 2 { break; } sum = sum + i; }\n\
         print(sum);\n\
         }\n",
    )
    .unwrap();
    let emitted2 = Command::new(magicc())
        .args(["--backend", "c", "emit", src2.to_str().unwrap(), "--unroll"])
        .output()
        .expect("run magicc");
    let text2 = String::from_utf8_lossy(&emitted2.stdout);
    assert!(
        text2.contains("for (int32_t i ="),
        "loop with break must not be unrolled: {text2}"
    );

    if !has_compiler() {
        return;
    }
    let out = Command::new(magicc())
        .args(["--backend", "c", "build", src.to_str().unwrap(), "--unroll"])
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = if cfg!(windows) {
        dir.path().join("u.exe")
    } else {
        dir.path().join("u.bin")
    };
    let run = Command::new(&exe).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "6");
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
        .args(["--backend", "c", "build", src.to_str().unwrap(), "--cc", "gcc -O1"])
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

#[test]
fn changing_the_cc_rebuilds_the_program() {
    if !has_compiler() {
        return;
    }
    let dir = TempDir::new("cc_stamp");
    let src = dir.path().join("hello.magic");
    std::fs::write(&src, "fn main() { print(1); }\n").expect("write source");
    let first = Command::new(magicc())
        .current_dir(dir.path())
        .args(["--backend", "c", "build", "hello.magic"])
        .output()
        .expect("run magicc");
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let stamp = dir.path().join("hello.flags");
    let before = std::fs::read_to_string(&stamp).expect("flags stamp");
    let default_cc = before
        .lines()
        .next()
        .unwrap_or("")
        .rsplit('|')
        .next()
        .unwrap_or("gcc")
        .trim()
        .to_string();
    let second = Command::new(magicc())
        .current_dir(dir.path())
        .args(["--backend", "c", "build", "hello.magic", "--cc", &format!("{default_cc} -O1")])
        .output()
        .expect("run magicc");
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
    let after = std::fs::read_to_string(&stamp).expect("flags stamp");
    assert_ne!(before, after, "the compiler must be part of the build identity");
    assert!(after.contains("-O1"), "stamp was {after:?}");
}

#[test]
fn native_backend_builds_and_runs() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    let as_ok = Command::new("as")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !as_ok {
        return;
    }
    let dir = TempDir::new("native");
    let src = dir.path().join("n.magic");
    std::fs::write(&src, "fn main() { var x = 40; print(x + 2); }\n").expect("write source");
    let exe = dir.path().join("n");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "native build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run native binary");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
}

#[test]
fn native_backend_supports_arrays() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return;
    }
    let dir = TempDir::new("nativearr");
    let src = dir.path().join("a.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         var a: [i32] = [0; 5];\n\
         var i = 0;\n\
         while i < 5 { a[i] = i + 1; i = i + 1; }\n\
         var s = 0;\n\
         i = 0;\n\
         while i < 5 { s = s + a[i]; i = i + 1; }\n\
         print(s);\n\
         }\n",
    )
    .expect("write source");
    let exe = dir.path().join("a");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "native build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "15");
}

#[test]
fn native_backend_passes_arrays_and_extra_args() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return;
    }
    let dir = TempDir::new("nativeargs");
    let src = dir.path().join("a.magic");
    std::fs::write(
        &src,
        "fn sum(xs: [i32], n: i32) -> i32 {\n\
         var i = 0; var t = 0;\n\
         while i < n { t = t + xs[i]; i = i + 1; }\n\
         return t;\n\
         }\n\
         fn seven(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32) -> i32 {\n\
         return a + g;\n\
         }\n\
         fn main() {\n\
         var a: [i32] = [0; 3];\n\
         a[0] = 10; a[1] = 20; a[2] = 30;\n\
         print(sum(a, 3));\n\
         print(seven(1, 2, 3, 4, 5, 6, 7));\n\
         }\n",
    )
    .expect("write source");
    let exe = dir.path().join("a");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "native build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "60\n8");
}

#[test]
fn native_backend_file_io_roundtrip() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return;
    }
    let dir = TempDir::new("nativeio");
    let data = dir.path().join("data.bin");
    let src = dir.path().join("io.magic");
    let text = format!(
        "fn main() {{\n\
         var fd = file_open(\"{}\", \"wb\");\n\
         var b: [u8] = [65; 3];\n\
         file_write(fd, b, 3);\n\
         file_close(fd);\n\
         var rf = file_open(\"{}\", \"rb\");\n\
         var c: [u8] = [0; 3];\n\
         var n = file_read(rf, c, 3);\n\
         file_close(rf);\n\
         print(n);\n\
         print(c[0]);\n\
         }}\n",
        data.display(),
        data.display()
    );
    std::fs::write(&src, text).expect("write source");
    let exe = dir.path().join("io");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "native build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "3\n65");
}

#[test]
fn native_backend_wraps_like_the_c_backend() {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return;
    }
    if !Command::new("as")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return;
    }
    let dir = TempDir::new("nativewrap");
    let src = dir.path().join("w.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         var x: i32 = 2147483647;\n\
         x = x + 1;\n\
         print(x);\n\
         var b: u8 = 255;\n\
         b = b + 1;\n\
         print(b);\n\
         }\n",
    )
    .expect("write source");
    let exe = dir.path().join("w");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "native build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "-2147483648\n0"
    );
}

#[test]
fn source_paths_cannot_inject_compiler_arguments() {
    let dir = TempDir::new("inject");
    std::fs::write(dir.path().join("@evil.magic"), "fn main() { print(7); }\n").unwrap();
    std::fs::write(dir.path().join("evil.c"), "-fplugin=/nonexistent/evil.so\n").unwrap();
    let exe = dir.path().join("out");
    let out = Command::new(magicc())
        .current_dir(dir.path())
        .arg("build")
        .arg("@evil.magic")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run magicc");
    assert!(
        out.status.success(),
        "@-named source failed to build: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = Command::new(&exe).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "7");
}

#[cfg(unix)]
#[test]
fn symlinked_artifacts_are_refused() {
    let dir = TempDir::new("symlink");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, "untouched").unwrap();
    std::fs::write(dir.path().join("t.magic"), "fn main() { print(1); }\n").unwrap();
    if std::os::unix::fs::symlink(&victim, dir.path().join("t.c")).is_err() {
        return;
    }
    let out = Command::new(magicc())
        .current_dir(dir.path())
        .args(["--backend", "c"])
        .arg("build")
        .arg("t.magic")
        .output()
        .expect("run magicc");
    assert!(!out.status.success(), "build wrote through a symlink");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn tampered_binary_is_rebuilt() {
    let dir = TempDir::new("tamper");
    let src = dir.path().join("t.magic");
    std::fs::write(&src, "fn main() { print(11); }\n").unwrap();
    let exe = dir.path().join("t");
    let first = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(first.status.success());
    let mut bytes = std::fs::read(&exe).unwrap();
    bytes.extend_from_slice(b"tampered");
    std::fs::write(&exe, bytes).unwrap();
    let second = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("rebuild");
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(stdout.contains("built"), "tampered binary was trusted: {stdout}");
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "11");
}

#[test]
fn deeply_nested_expressions_are_rejected() {
    let dir = TempDir::new("deep");
    let src = dir.path().join("deep.magic");
    let mut text = String::from("fn main() { print(");
    text.push_str(&"(".repeat(5000));
    text.push('1');
    text.push_str(&")".repeat(5000));
    text.push_str("); }\n");
    std::fs::write(&src, text).unwrap();
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .output()
        .expect("run magicc");
    assert!(!out.status.success());
    assert!(out.status.code().is_some(), "compiler died by signal");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("too deep"),
        "missing depth diagnostic"
    );
}

#[test]
fn trigraphs_are_escaped() {
    let dir = TempDir::new("trigraph");
    let src = dir.path().join("t.magic");
    std::fs::write(&src, "fn main() { print(\"a??/b\"); }\n").unwrap();
    let exe = dir.path().join("t");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "a??/b");
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

#[test]
fn native_inline_substitution_uses_caller_values() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("inlsub");
    let src = dir.path().join("i.magic");
    std::fs::write(
        &src,
        "fn add(a: i32, b: i32) -> i32 { return a + b; }\n\
         fn main() {\n\
         var b: i32 = 1000;\n\
         print(add(b, 1));\n\
         var a: i32 = 100;\n\
         var c: i32 = 5;\n\
         print(add(a, 2));\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("i");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "1001\n102");
}

#[test]
fn native_early_return_frees_nothing() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("early");
    let src = dir.path().join("e.magic");
    std::fs::write(
        &src,
        "fn g(x: i32) -> i32 { return x; }\n\
         fn f(flag: bool) -> i32 {\n\
         if flag { return 1; }\n\
         var a: [i32] = [0; 4];\n\
         a[0] = 7;\n\
         return a[0] + g(1);\n\
         }\n\
         fn main() { print(f(true)); print(f(false)); }\n",
    )
    .unwrap();
    let exe = dir.path().join("e");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert!(run.status.success(), "native binary crashed");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "1\n8");
}

#[test]
fn native_file_read_checks_the_buffer_length() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("fread");
    let src = dir.path().join("f.magic");
    std::fs::write(
        &src,
        format!(
            "fn main() {{\n\
             var fd = file_open(\"{}\", \"rb\");\n\
             var b: [u8] = [0; 16];\n\
             print(file_read(fd, b, 4096));\n\
             }}\n",
            dir.path().join("data").display()
        ),
    )
    .unwrap();
    std::fs::write(dir.path().join("data"), b"hello").unwrap();
    let exe = dir.path().join("f");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("length exceeds the buffer"),
        "missing bounds diagnostic"
    );
}

#[test]
fn native_i64_literals_and_u8_promotion_match_c() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("widths");
    let src = dir.path().join("w.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         var a: i64 = 4000000000 + 1;\n\
         print(a);\n\
         var b: u8 = 250;\n\
         print(b + 10);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("w");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "4000000001\n260"
    );
}

#[test]
fn native_division_by_zero_panics() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("divzero");
    let src = dir.path().join("d.magic");
    std::fs::write(&src, "fn main() { var a = 10; var b = 0; print(a / b); }\n").unwrap();
    let exe = dir.path().join("d");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(run.status.code(), Some(1), "expected a panic, not SIGFPE");
    assert!(String::from_utf8_lossy(&run.stderr).contains("division by zero"));
}

#[test]
fn native_null_env_and_arg_are_safe() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("nullstr");
    let src = dir.path().join("n.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         print(str_len(env(\"MAGIC_SURELY_UNSET_XYZ\")));\n\
         print(str_len(arg(999)));\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("n");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert!(run.status.success(), "null string crashed");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "0\n0");
}

#[test]
fn native_floats_match_the_c_backend() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("floats");
    let src = dir.path().join("f.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         var x = 1.5;\n\
         var y = 2.0;\n\
         print(x + y);\n\
         print(x < y);\n\
         print(sqrt(16.0));\n\
         print_prec(x * y, 2);\n\
         print(-x);\n\
         var a: [f64] = [0.0; 2];\n\
         a[0] = 0.5;\n\
         a[1] = a[0] * 4.0;\n\
         print(a[1]);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("f");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "3.5\ntrue\n4\n3.00\n-1.5\n2"
    );
}

#[test]
fn native_prints_bools_as_true_and_false() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("boolp");
    let src = dir.path().join("b.magic");
    std::fs::write(&src, "fn main() { print(1 < 2); print(2 < 1); }\n").unwrap();
    let exe = dir.path().join("b");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "true\nfalse");
}

#[test]
fn native_structs_match_the_c_backend() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("structs");
    let src = dir.path().join("s.magic");
    std::fs::write(
        &src,
        "struct Point { x: i32, y: i32 }\n\
         fn main() {\n\
         var p: Point = Point { x: 1, y: 2 };\n\
         p.x = p.x + 10;\n\
         print(p.x);\n\
         print(p.y);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("s");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "11\n2");
}

#[test]
fn native_struct_arrays_match_the_c_backend() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("structarr");
    let src = dir.path().join("p.magic");
    std::fs::write(
        &src,
        "struct Point { x: i32, y: i32 }\n\
         fn main() {\n\
         var pts: [Point] = [Point { x: 0, y: 0 }; 3];\n\
         for i in 0..3 {\n\
         pts[i].x = i * 10;\n\
         pts[i].y = i * 10 + 1;\n\
         }\n\
         print(pts[0].x);\n\
         print(pts[1].y);\n\
         print(pts[2].x);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("p");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("--backend")
        .arg("native")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "0\n11\n20");
}

#[test]
fn native_nested_structs_and_vectors_match_c() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("inv");
    let src = dir.path().join("i.magic");
    std::fs::write(
        &src,
        "struct Item { sku: i32, qty: i32 }\n\
         struct Bin { item: Item, count: i32 }\n\
         fn main() {\n\
         var bins: [Bin] = [Bin { item: Item { sku: 0, qty: 0 }, count: 0 }; 4];\n\
         for i in 0..4 {\n\
         bins[i].item.sku = 100 + i;\n\
         bins[i].item.qty = i * 5;\n\
         bins[i].count = 2;\n\
         }\n\
         var totals: [i32+] = [];\n\
         for i in 0..4 { totals.push(bins[i].item.qty * bins[i].count); }\n\
         var grand = 0;\n\
         for i in 0..(len(totals) as i32) { grand = grand + totals[i]; }\n\
         print(bins[3].item.sku);\n\
         print(len(totals));\n\
         print(grand);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("i");
    let out = Command::new(magicc())
        .arg("build").arg(&src).arg("--backend").arg("native").arg("-o").arg(&exe)
        .output().expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "103\n4\n60");
}

#[test]
fn native_vectors_of_structs_match_c() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("vecp");
    let src = dir.path().join("v.magic");
    std::fs::write(
        &src,
        "struct P { x: f64, y: f64 }\n\
         fn main() {\n\
         var ps: [P+] = [];\n\
         reserve(ps, 2);\n\
         ps.push(P { x: 1.0, y: 2.0 });\n\
         ps.push(P { x: 3.0, y: 4.0 });\n\
         var t = 0.0;\n\
         for i in 0..(len(ps) as i32) { t = t + ps[i].x + ps[i].y; }\n\
         print(t);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("v");
    let out = Command::new(magicc())
        .arg("build").arg(&src).arg("--backend").arg("native").arg("-o").arg(&exe)
        .output().expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "10");
}

#[test]
fn native_bool_arrays_are_bit_packed() {
    if !native_available() {
        return;
    }
    let dir = TempDir::new("bits");
    let src = dir.path().join("b.magic");
    std::fs::write(
        &src,
        "fn count(xs: [bool], n: i32) -> i32 {\n\
         var c = 0; var i = 0;\n\
         while i < n { if xs[i] { c = c + 1; } i = i + 1; }\n\
         return c;\n\
         }\n\
         fn main() {\n\
         var a: [bool] = [false; 130];\n\
         var i = 0;\n\
         while i < 130 { if i % 3 == 0 { a[i] = true; } i = i + 1; }\n\
         print(count(a, 130));\n\
         print(a[0]);\n\
         print(a[1]);\n\
         a[129] = true;\n\
         print(a[129]);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("b");
    let out = Command::new(magicc())
        .arg("build").arg(&src).arg("--backend").arg("native").arg("-o").arg(&exe)
        .output().expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "44\ntrue\nfalse\ntrue"
    );
}

#[test]
fn default_backend_builds_ir_and_runs() {
    let dir = TempDir::new("default_ir");
    let src = dir.path().join("d.magic");
    std::fs::write(&src, "fn main() { print(6 * 7); }\n").unwrap();
    let out = Command::new(magicc())
        .args(["build", src.to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("(ir)"), "stdout was: {text}");
    let exe = dir.path().join("d.bin");
    let run = Command::new(&exe).output().expect("run built program");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "42");
}

#[test]
fn default_emit_prints_assembly() {
    let out = Command::new(magicc())
        .args(["emit", examples().join("hello.magic").to_str().unwrap()])
        .output()
        .expect("run magicc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(".globl"), "expected assembly, got: {text}");
    assert!(!text.contains("int main(int magic_argc"), "unexpected C output");
}

#[test]
fn default_ir_refuses_symlinked_assembly() {
    let dir = TempDir::new("symlink_ir");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, "untouched").unwrap();
    std::fs::write(dir.path().join("t.magic"), "fn main() { print(1); }\n").unwrap();
    if std::os::unix::fs::symlink(&victim, dir.path().join("t.s")).is_err() {
        return;
    }
    let out = Command::new(magicc())
        .current_dir(dir.path())
        .arg("build")
        .arg("t.magic")
        .output()
        .expect("run magicc");
    assert!(!out.status.success(), "build wrote through a symlink");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
}

#[test]
fn string_len_and_concat_work_in_both_backends() {
    let dir = TempDir::new("strcat");
    let src = dir.path().join("s.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         let a = \"hello\";\n\
         let b = \" world\";\n\
         let c = str_cat(a, b);\n\
         print(len(a));\n\
         print(len(c));\n\
         print(c);\n\
         }\n",
    )
    .unwrap();

    let ir_exe = dir.path().join("s_ir");
    let out = Command::new(magicc())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&ir_exe)
        .output()
        .expect("run magicc");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&ir_exe).output().expect("run ir binary");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "5\n11\nhello world\n");

    if !has_compiler() {
        return;
    }
    let c_exe = dir.path().join("s_c");
    let out = Command::new(magicc())
        .args(["--backend", "c", "build", src.to_str().unwrap()])
        .arg("-o")
        .arg(&c_exe)
        .output()
        .expect("run magicc c");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&c_exe).output().expect("run c binary");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "5\n11\nhello world\n");
}

#[test]
fn index_arithmetic_overflow_is_rejected_both_backends() {
    let dir = TempDir::new("idxov");
    let src = dir.path().join("idx.magic");
    std::fs::write(
        &src,
        "fn main() {\n\
         var a: [i32] = [0; 4];\n\
         var x: i32 = 65536;\n\
         var y: i32 = 65536;\n\
         a[x * y] = 7;\n\
         print(a[0]);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("idx_ir");
    Command::new(magicc()).arg("build").arg(&src).arg("-o").arg(&exe).output().expect("build");
    let out = Command::new(&exe).output().expect("run");
    assert!(!out.status.success(), "IR accepted wrapped index");
    assert!(String::from_utf8_lossy(&out.stderr).contains("index out of bounds"));

    if !has_compiler() {
        return;
    }
    let cexe = dir.path().join("idx_c");
    let out = Command::new(magicc())
        .args(["--backend", "c", "build", src.to_str().unwrap()])
        .arg("-o")
        .arg(&cexe)
        .output()
        .expect("build c");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&cexe).output().expect("run c");
    assert!(!out.status.success(), "C accepted wrapped index");
    assert!(String::from_utf8_lossy(&out.stderr).contains("index out of bounds"));
}

#[test]
fn vec_view_alias_is_rejected() {
    let dir = TempDir::new("vecalias");
    let src = dir.path().join("a.magic");
    std::fs::write(
        &src,
        "fn f(a: [i32], v: [i32+]) { v.push(1); print(a[0]); }\n\
         fn main() { var v: [i32+] = []; v.push(77); f(v, v); }\n",
    )
    .unwrap();
    let out = Command::new(magicc())
        .args(["check", src.to_str().unwrap()])
        .output()
        .expect("check");
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("invalidat"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn vec_param_push_pop_keeps_length() {
    let dir = TempDir::new("vecpop");
    let src = dir.path().join("p.magic");
    std::fs::write(
        &src,
        "fn addn(v: [i32+]) { v.push(5); pop(v); print(len(v)); }\n\
         fn main() {\n\
           var v: [i32+] = [];\n\
           v.push(1); v.push(2);\n\
           addn(v);\n\
           print(len(v)); print(v[0]); print(v[1]);\n\
         }\n",
    )
    .unwrap();
    let exe = dir.path().join("p_ir");
    let out = Command::new(magicc()).arg("build").arg(&src).arg("-o").arg(&exe).output().expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&exe).output().expect("run");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "2\n2\n1\n2\n");
}
