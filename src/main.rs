//! magicc - the MAGIC compiler driver.
//!
//!   magicc check <file>   parse + type check only
//!   magicc emit  <file>   print the generated C to stdout
//!   magicc build <file>   write <file>.c and compile it
//!   magicc run   <file>   build, then run

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("magicc: {}", msg);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args: Vec<String> = env::args().collect();
    let native = args.iter().any(|a| a == "--native");
    let report_bound = args.iter().any(|a| a == "--report");
    let dev = args.iter().any(|a| a == "--dev");
    args.retain(|a| a != "--native" && a != "--report" && a != "--dev");
    let mut cc_override: Option<String> = None;
    let unroll = args.iter().any(|a| a == "--unroll");
    args.retain(|a| a != "--unroll");
    if let Some(pos) = args.iter().position(|a| a == "--cc") {
        if pos + 1 >= args.len() {
            return Err("--cc needs a command, e.g. --cc \"zig cc\"".to_string());
        }
        cc_override = Some(args[pos + 1].clone());
        args.drain(pos..=pos + 1);
    }
    let static_link = args.iter().any(|a| a == "--static");
    args.retain(|a| a != "--static");
    let mut backend = "ir".to_string();
    if let Some(pos) = args.iter().position(|a| a == "--backend") {
        if pos + 1 >= args.len() {
            return Err("--backend needs a value: c, native or ir".to_string());
        }
        backend = args[pos + 1].clone();
        args.drain(pos..=pos + 1);
    }
    if backend != "c" && backend != "native" && backend != "ir" {
        return Err(format!("unknown backend `{}` (use c, native or ir)", backend));
    }
    if static_link && backend != "ir" {
        return Err("--static requires the ir backend".to_string());
    }
    if backend != "c" {
        if unroll {
            return Err("--unroll is only supported with --backend c".to_string());
        }
        if dev {
            return Err("--dev is only supported with --backend c".to_string());
        }
        if native {
            return Err("--native (gcc -march=native) is only supported with --backend c".to_string());
        }
        if cc_override.is_some() {
            return Err("--cc is only supported with --backend c".to_string());
        }
    }
    let mut out_override: Option<PathBuf> = None;
    if let Some(pos) = args.iter().position(|a| a == "-o") {
        if pos + 1 >= args.len() {
            return Err("-o needs a path, e.g. -o out.exe".to_string());
        }
        out_override = Some(PathBuf::from(&args[pos + 1]));
        args.drain(pos..=pos + 1);
    }
    if args.len() == 2
        && matches!(
            args[1].as_str(),
            "--version" | "-V" | "-v" | "-version" | "version"
        )
    {
        println!("magicc {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        println!(
            "🔮 magicc {} - the MAGIC compiler\n\n\
             usage: magicc <check|emit|build|run> <file.magic> [--report] [--backend c|native|ir] [-o out]\n\n\
             \x20 check   parse + type check only\n\
             \x20 emit    print generated assembly (IR/native) or C (--backend c)\n\
             \x20 build   build a native executable (default IR backend, as+ld)\n\
             \x20 run     build, then run\n\n\
             \x20 --backend c|native|ir   code generator (default ir; c emits C11)\n\
             \x20 --native  add -march=native to gcc (C backend only)\n\
             \x20 --report  print the static memory bound\n\
             \x20 --dev     compile at -O0 for fast iteration (C backend only)\n\
             \x20 --cc      C compiler command, may carry arguments (e.g. \"zig cc\")\n\
             \x20 --unroll  unroll small constant for-loops (C backend only)\n\n\
             \x20 --static  freestanding static ELF for integer/bool programs (ir backend)\n\
             \x20 -o        output executable path (default: <file>.exe)",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    if args.len() < 3 {
        eprintln!(
            "🔮 magicc {} - the MAGIC compiler",
            env!("CARGO_PKG_VERSION")
        );
        return Err(
            "usage: magicc <check|emit|build|run> <file.magic> [--report] [--backend c|native|ir]"
                .to_string(),
        );
    }
    let command = args[1].as_str();
    let src_path = Path::new(&args[2]);

    // Entry files plus everything they `import`, each included once, dependencies first.
    let mut source = String::new();
    let mut segments: Vec<(usize, PathBuf)> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    for p in &args[2..] {
        load_source(Path::new(p), &mut visited, &mut source, &mut segments)?;
    }

    if report_bound {
        let rows =
            magic::memory_report(&source).map_err(|e| map_diagnostic(e, &segments, src_path))?;
        for (name, bytes) in rows {
            if bytes > 0 {
                println!("🔮 memory: {} <= {} bytes", name, bytes);
            }
        }
    }

    match command {
        "check" => {
            match backend.as_str() {
                "ir" => {
                    magic::compile_to_asm_ir(&source)
                        .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                }
                "native" => {
                    magic::compile_to_asm(&source)
                        .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                }
                _ => {
                    magic::compile_to_c_opts(&source, unroll)
                        .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                }
            }
            println!("🔮 ok: {}", src_path.display());
            Ok(())
        }
        "emit" => {
            if backend == "ir" {
                let asm = magic::compile_to_asm_ir(&source)
                    .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                print!("{}", asm);
            } else if backend == "native" {
                let asm = magic::compile_to_asm(&source)
                    .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                print!("{}", asm);
            } else {
                let c = magic::compile_to_c_opts(&source, unroll)
                    .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                print!("{}", c);
            }
            Ok(())
        }
        "build" | "run" => {
            if static_link {
                let asm = magic::compile_to_asm_ir(&source)
                    .map_err(|e| map_diagnostic(e, &segments, src_path))?;
                if asm.lines().any(|l| l.contains("lea .Lfmt_g(") || l.contains("lea .Lfmt_pf(")) {
                    return Err("--static does not support float printing".to_string());
                }
                let exe_path = out_override.clone().unwrap_or_else(|| with_ext(src_path, "bin"));
                build_static_ir(&asm, &exe_path)?;
                println!("\u{1F52E} built {} (static ir)", exe_path.display());
                if command == "run" {
                    let status = Command::new(&exe_path)
                        .status()
                        .map_err(|e| format!("failed to run {}: {}", exe_path.display(), e))?;
                    if !status.success() {
                        return Err("program exited with a non-zero status".to_string());
                    }
                }
                return Ok(());
            }
            if backend == "native" || backend == "ir" {
                return build_native(
                    &src_path,
                    &source,
                    out_override,
                    command == "run",
                    cc_override.as_deref(),
                    backend == "ir",
                )
                .map_err(|e| map_diagnostic(e, &segments, src_path));
            }
            let c = magic::compile_to_c_opts(&source, unroll)
                .map_err(|e| map_diagnostic(e, &segments, src_path))?;
            let dir = src_path.parent().unwrap_or_else(|| Path::new("."));
            let exe_path = out_override.clone().unwrap_or_else(|| {
                if cfg!(windows) {
                    with_ext(src_path, "exe")
                } else {
                    with_ext(src_path, "bin")
                }
            });
            let stem = if out_override.is_some() {
                exe_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "out".to_string())
            } else {
                src_path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "out".to_string())
            };
            // The C compiler is part of the build identity: switching --cc must
            // rebuild instead of silently reusing the previous executable.
            let cc = cc_override
                .clone()
                .or_else(find_cc)
                .ok_or("no C compiler found (tried $CC, gcc, clang, cc)")?;
            let flags = format!(
                "v3|{}|{:x}|{}{}-nou|{}|{}",
                env!("CARGO_PKG_VERSION"),
                fnv64(source.as_bytes()),
                if dev { "O0" } else { "O3" },
                if native { "-native" } else { "" },
                compiler_hash(),
                cc
            );
            let stamp = dir.join(format!("{}.flags", stem));
            let stamp_text = fs::read_to_string(&stamp).unwrap_or_default();
            let flags_changed = stamp_text
                .lines()
                .next()
                .map(|l| l.trim() != flags)
                .unwrap_or(true);
            let recorded_exe = stamp_text.lines().nth(1).unwrap_or("").trim().to_string();
            let exe_matches = !recorded_exe.is_empty()
                && recorded_exe == fnv_file(&exe_path);
            let chunks = magic::compile_parts_opts(&source, 16, unroll)
                .map_err(|e| map_diagnostic(e, &segments, src_path))?;
            let needs_omp = chunks.iter().any(|c| c.contains("#pragma omp"));
            if unroll && cc.to_lowercase().contains("gcc") {
                eprintln!(
                    "note: --unroll usually helps clang/zig and can regress gcc; \
                     measure before keeping it"
                );
            }

            if chunks.len() == 1 {
                let c_path = with_ext(src_path, "c");
                let unchanged = fs::read_to_string(&c_path)
                    .map(|old| old == chunks[0])
                    .unwrap_or(false);

                if unchanged && exe_path.exists() && !flags_changed && exe_matches {
                    println!("🔮 up to date {}", exe_path.display());
                } else {
                    refuse_symlink(&c_path)?;
                    refuse_symlink(&exe_path)?;
                    write_atomic(&c_path, chunks[0].as_bytes())
                        .map_err(|e| format!("cannot write {}: {}", c_path.display(), e))?;
                    let mut cmd = cc_command(&cc);
                    cmd.arg(if dev { "-O0" } else { "-O3" })
                        .arg("-std=c11")
                        .arg("-fwrapv")
                        .arg("-fno-strict-overflow")
                        .arg("-fno-ident");
                    if needs_omp {
                        cmd.arg("-fopenmp");
                    }
                    if cfg!(windows) {
                        cmd.arg("-lws2_32");
                    } else {
                        cmd.arg("-rdynamic");
                    }
                    if native {
                        cmd.arg("-march=native");
                    }
                    let status = cmd
                        .arg("-o")
                        .arg(&exe_path)
                        .arg(abs_path(&c_path))
                        .status()
                        .map_err(|e| format!("failed to run {}: {}", cc, e))?;
                    if !status.success() {
                        return Err(format!("{} failed while compiling {}", cc, c_path.display()));
                    }
                    println!("🔮 built {}", exe_path.display());
                    refuse_symlink(&stamp)?;
                    let stamp_body = format!("{}\n{}", flags, fnv_file(&exe_path));
                    let _ = write_atomic(&stamp, stamp_body.as_bytes());
                }
            } else {
                let mut objs = Vec::new();
                let mut jobs = Vec::new();
                for (i, chunk) in chunks.iter().enumerate() {
                    let c_path = dir.join(format!("{}.{}.c", stem, i));
                    let o_path = dir.join(format!("{}.{}.o", stem, i));
                    let same = fs::read_to_string(&c_path)
                        .map(|old| &old == chunk)
                        .unwrap_or(false);
                    if !same {
                        write_with_retry(&c_path, chunk)?;
                    }
                    let dirty = !same || !o_path.exists() || flags_changed;
                    jobs.push((c_path, o_path.clone(), dirty));
                    objs.push(o_path);
                }

                let dirty_count = jobs.iter().filter(|(_, _, d)| *d).count();
                if dirty_count == 0 && exe_path.exists() && !flags_changed {
                    println!("🔮 up to date {}", exe_path.display());
                } else {
                    let mut handles = Vec::new();
                    for (c_path, o_path, dirty) in jobs {
                        if !dirty {
                            continue;
                        }
                        let cc2 = cc.clone();
                        let native2 = native;
                        let omp2 = needs_omp;
                        handles.push(std::thread::spawn(move || {
                            let mut cmd = cc_command(&cc2);
                            cmd.arg(if dev { "-O0" } else { "-O3" })
                                .arg("-std=c11")
                                        .arg("-fno-ident");
                            if omp2 {
                                cmd.arg("-fopenmp");
                            }
                            if native2 {
                                cmd.arg("-march=native");
                            }
                            cmd.arg("-c")
                                .arg("-fwrapv")
                                .arg("-fno-strict-overflow")
                                .arg("-o")
                                .arg(abs_path(&o_path))
                                .arg(abs_path(&c_path));
                            cmd.status()
                        }));
                    }
                    for h in handles {
                        let res = h
                            .join()
                            .map_err(|_| "compile thread panicked".to_string())?;
                        let status =
                            res.map_err(|e| format!("failed to run compiler: {}", e))?;
                        if !status.success() {
                            return Err("C compilation failed".to_string());
                        }
                    }
                    let mut link = cc_command(&cc);
                    if needs_omp {
                        link.arg("-fopenmp");
                    }
                    if cfg!(windows) {
                        link.arg("-lws2_32");
                    }
                    link.arg("-o").arg(&exe_path);
                    for o in &objs {
                        link.arg(o);
                    }
                    let status = link.status().map_err(|e| format!("failed to link: {}", e))?;
                    if !status.success() {
                        return Err("link failed".to_string());
                    }
                    println!(
                        "🔮 built {} ({} units, {} compiled)",
                        exe_path.display(),
                        chunks.len(),
                        dirty_count
                    );
                    let stamp_body = format!("{}\n{}", flags, fnv_file(&exe_path));
                    let _ = write_atomic(&stamp, stamp_body.as_bytes());
                }
            }

            if command == "run" {
                let status = Command::new(abs_path(&exe_path))
                    .status()
                    .map_err(|e| format!("cannot run {}: {}", exe_path.display(), e))?;
                if !status.success() {
                    return Err(format!("program exited with {}", status));
                }
            }
            Ok(())
        }
        other => Err(format!(
            "unknown command '{}' (use check, emit, build, run)",
            other
        )),
    }
}

fn write_with_retry(path: &Path, content: &str) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..5 {
        match write_atomic(path, content.as_bytes()) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = format!("cannot write {}: {}", path.display(), e);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Err(last)
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn fnv_file(path: &Path) -> String {
    match fs::read(path) {
        Ok(b) => format!("{:x}", fnv64(&b)),
        Err(_) => "0".to_string(),
    }
}

/// Refuse to write generated artifacts through a symlink committed in the repo.
fn refuse_symlink(path: &Path) -> Result<(), String> {
    if let Ok(m) = fs::symlink_metadata(path) {
        if m.file_type().is_symlink() {
            return Err(format!(
                "refusing to write through symlink {}: remove it or build in another directory",
                path.display()
            ));
        }
    }
    Ok(())
}

/// Absolute path for anything handed to a child process. An absolute path can
/// never be read as a command-line option or a GCC response file (`@file`).
fn abs_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|d| d.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Write through a same-directory temporary file and rename it into place, so a
/// symlink committed in the repository cannot redirect generated C/asm/stamps
/// into another file.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

fn with_ext(path: &Path, ext: &str) -> PathBuf {
    let mut p = path.to_path_buf();
    p.set_extension(ext);
    p
}

fn cc_command(cc: &str) -> Command {
    // $CC may carry arguments, e.g. "zig cc" or "clang --target=x86_64-pc-windows-gnu".
    let mut parts = cc.split_whitespace();
    let prog = parts.next().unwrap_or("cc");
    let mut cmd = Command::new(prog);
    for a in parts {
        cmd.arg(a);
    }
    cmd
}

fn compiler_hash() -> String {
    std::env::current_exe()
        .ok()
        .map(|p| fnv_file(&p))
        .unwrap_or_default()
}

fn sectionize_static(asm: &str) -> String {
    let asm = asm.replace(".Lir_panic:", ".Lir_panic_emitted:");
    let asm = asm.replace("call .Lir_panic\n", "call .Lir_panic_static\n");
    let mut out = String::new();
    let mut sec = 0usize;
    let mut prev = String::new();
    let mut in_cfi = false;
    for ln in asm.lines() {
        let st = ln.trim();
        let mut entry = false;
        if st == ".cfi_startproc" {
            entry = true;
            in_cfi = true;
        }
        if st == ".cfi_endproc" {
            in_cfi = false;
        }
        if !in_cfi
            && prev.starts_with(".p2align")
            && st.ends_with(':')
            && st.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '.')
        {
            entry = true;
        }
        if entry {
            out.push_str(&format!(".section .text.magic_{},\"ax\",@progbits\n", sec));
            sec += 1;
        }
        out.push_str(ln);
        out.push('\n');
        if !st.is_empty() {
            prev = st.to_string();
        }
    }
    out
}

fn build_static_ir(asm: &str, exe: &Path) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "magic-static-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    let prog_s = dir.join("prog.s");
    let rt_s = dir.join("runtime.s");
    let prog_o = dir.join("prog.o");
    let rt_o = dir.join("runtime.o");
    std::fs::write(&prog_s, sectionize_static(asm)).map_err(|e| e.to_string())?;
    std::fs::write(&rt_s, include_str!("../runtime/static_runtime.s")).map_err(|e| e.to_string())?;
    let as_cmd = env::var("AS").unwrap_or_else(|_| "as".to_string());
    let ld_cmd = env::var("LD").unwrap_or_else(|_| "ld".to_string());
    for (src, obj) in [(&prog_s, &prog_o), (&rt_s, &rt_o)] {
        let st = Command::new(&as_cmd)
            .args(["--64", src.to_str().unwrap_or(""), "-o", obj.to_str().unwrap_or("")])
            .status()
            .map_err(|e| format!("failed to run {}: {}", as_cmd, e))?;
        if !st.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("assembler failed for {}", src.display()));
        }
    }
    let st = Command::new(&ld_cmd)
        .args([
            "-static",
            "-nostdlib",
            "--gc-sections",
            "-e",
            "_start",
            prog_o.to_str().unwrap_or(""),
            rt_o.to_str().unwrap_or(""),
            "-o",
            exe.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| format!("failed to run {}: {}", ld_cmd, e))?;
    let _ = std::fs::remove_dir_all(&dir);
    if !st.status.success() {
        let msg = String::from_utf8_lossy(&st.stderr);
        if let Some(sym) = msg.split("undefined reference to `").nth(1) {
            let sym = sym.split('\'').next().unwrap_or(sym);
            return Err(format!(
                "--static does not support this program (missing freestanding runtime symbol `{}`)",
                sym
            ));
        }
        return Err(format!("static link failed: {}", msg.trim()));
    }
    Ok(())
}

fn build_native(
    src_path: &Path,
    source: &str,
    out_override: Option<PathBuf>,
    run: bool,
    cc_override: Option<&str>,
    use_ir: bool,
) -> Result<(), String> {
    if !(cfg!(target_os = "linux") && cfg!(target_arch = "x86_64")) {
        return Err("the native backend currently targets Linux x86-64 only; use the C backend".to_string());
    }
    let dir = src_path.parent().unwrap_or_else(|| Path::new("."));
    let exe_path = out_override.clone().unwrap_or_else(|| {
        if cfg!(windows) {
            with_ext(src_path, "exe")
        } else {
            with_ext(src_path, "bin")
        }
    });
    let stem = if out_override.is_some() {
        exe_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "out".to_string())
    } else {
        src_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "out".to_string())
    };
    let flags = format!(
        "v2|{}|{:x}|{}|{}|{}|{}|{}",
        env!("CARGO_PKG_VERSION"),
        fnv64(source.as_bytes()),
        if use_ir { "ir" } else { "native" },
        env::var("AS").unwrap_or_default(),
        env::var("LD").unwrap_or_default(),
        env::var("MAGIC_CRT_DIR").unwrap_or_default(),
        compiler_hash()
    );
    let stamp = dir.join(format!("{}.flags", stem));
    let stamp_text = fs::read_to_string(&stamp).unwrap_or_default();
    let flags_changed = stamp_text
        .lines()
        .next()
        .map(|l| l.trim() != flags)
        .unwrap_or(true);
    let recorded_exe = stamp_text.lines().nth(1).unwrap_or("").trim().to_string();
    let exe_matches = exe_path.exists()
        && !recorded_exe.is_empty()
        && recorded_exe == fnv_file(&exe_path);
    if !flags_changed && exe_matches {
        println!("\u{1F52E} up to date {}", exe_path.display());
        if run {
            let status = Command::new(&exe_path)
                .status()
                .map_err(|e| format!("failed to run {}: {}", exe_path.display(), e))?;
            if !status.success() {
                return Err("program exited with a non-zero status".to_string());
            }
        }
        return Ok(());
    }
    let asm = if use_ir {
        magic::compile_to_asm_ir(source)?
    } else {
        magic::compile_to_asm(source)?
    };
    let asm_path = with_ext(src_path, "s");
    let obj_path = with_ext(src_path, "o");
    refuse_symlink(&asm_path)?;
    refuse_symlink(&obj_path)?;
    refuse_symlink(&exe_path)?;
    write_atomic(&asm_path, asm.as_bytes()).map_err(|e| format!("cannot write {}: {}", asm_path.display(), e))?;
    let as_cmd = env::var("AS").unwrap_or_else(|_| "as".to_string());
    let status = Command::new(&as_cmd)
        .arg("--64")
        .arg("-o")
        .arg(abs_path(&obj_path))
        .arg(abs_path(&asm_path))
        .status()
        .map_err(|e| format!("failed to run {}: {}", as_cmd, e))?;
    if !status.success() {
        return Err(format!("{} failed while assembling {}", as_cmd, asm_path.display()));
    }
    // Link with `ld` directly: the native path must not execute a C compiler
    // or compiler driver. crt objects and the dynamic linker are discovered in
    // the usual multiarch locations, and can be overridden with MAGIC_CRT_DIR
    // and MAGIC_LDSO.
    let dirs: Vec<PathBuf> = if let Ok(d) = env::var("MAGIC_CRT_DIR") {
        vec![PathBuf::from(d)]
    } else {
        ["/usr/lib64", "/usr/lib/x86_64-linux-gnu", "/lib64", "/usr/lib", "/lib"]
            .iter()
            .map(PathBuf::from)
            .collect()
    };
    let find = |name: &str| -> Option<PathBuf> {
        dirs.iter().map(|d| d.join(name)).find(|p| p.exists())
    };
    let crt1 = find("crt1.o").ok_or_else(|| {
        "native backend: cannot find crt1.o (set MAGIC_CRT_DIR); use the C backend"
            .to_string()
    })?;
    let crti = find("crti.o");
    let crtn = find("crtn.o");
    let ldso = env::var("MAGIC_LDSO").unwrap_or_else(|_| {
        ["/lib64/ld-linux-x86-64.so.2", "/lib/ld-linux-x86-64.so.2"]
            .iter()
            .find(|p| Path::new(p).exists())
            .map(|p| p.to_string())
            .unwrap_or_else(|| "/lib64/ld-linux-x86-64.so.2".to_string())
    });
    let ld_cmd = env::var("LD").unwrap_or_else(|_| "ld".to_string());
    let mut link = Command::new(&ld_cmd);
    link.arg("--eh-frame-hdr");
    link.arg("-E");
    link.arg("-o").arg(abs_path(&exe_path)).arg(&crt1);
    if let Some(c) = &crti {
        link.arg(c);
    }
    link.arg(abs_path(&obj_path));
    if let Some(c) = &crtn {
        link.arg(c);
    }
    link.arg("-lc")
        .arg("-lpthread")
        .arg("-dynamic-linker")
        .arg(&ldso);
    for d in &dirs {
        link.arg(format!("-L{}", d.display()));
    }
    let status = link
        .status()
        .map_err(|e| format!("failed to run {}: {}", ld_cmd, e))?;
    if !status.success() {
        return Err(format!("{} failed while linking {}", ld_cmd, exe_path.display()));
    }
    let stamp_body = format!("{}\n{}", flags, fnv_file(&exe_path));
    let _ = write_atomic(&stamp, stamp_body.as_bytes());
    println!(
        "\u{1F52E} built {} ({})",
        exe_path.display(),
        if use_ir { "ir" } else { "native" }
    );
    if run {
        let status = Command::new(&exe_path)
            .status()
            .map_err(|e| format!("failed to run {}: {}", exe_path.display(), e))?;
        if !status.success() {
            return Err(format!("{} exited with {:?}", exe_path.display(), status.code()));
        }
    }
    Ok(())
}

fn load_source(
    path: &Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    out: &mut String,
    segments: &mut Vec<(usize, PathBuf)>,
) -> Result<(), String> {
    let canon = fs::canonicalize(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    if !visited.insert(canon.clone()) {
        return Ok(());
    }
    let text = fs::read_to_string(&canon)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let imports = magic::imports_of(&text)
        .map_err(|e| format!("{}: {}", path.display(), e))?;
    let dir = canon.parent().unwrap_or_else(|| Path::new("."));
    for imp in imports {
        load_source(&dir.join(imp), visited, out, segments)?;
    }
    let start = out.bytes().filter(|b| *b == b'\n').count() + 1;
    segments.push((start, canon.clone()));
    out.push_str(&text);
    if !text.ends_with('\n') {
        out.push('\n');
    }
    Ok(())
}

/// Rewrite `line: message` from the concatenated source into `file:line: message`.
fn map_diagnostic(err: String, segments: &[(usize, PathBuf)], fallback: &Path) -> String {
    if let Some((head, rest)) = err.split_once(": ") {
        if let Ok(line) = head.parse::<usize>() {
            if let Some((start, path)) = segments.iter().rev().find(|(s, _)| *s <= line) {
                return format!("{}:{}: {}", display_path(path), line - start + 1, rest);
            }
        }
    }
    format!("{}: {}", fallback.display(), err)
}

/// `canonicalize` on Windows returns `\\?\C:\...`; hide that from diagnostics.
fn display_path(path: &Path) -> String {
    let s = path.display().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

fn find_cc() -> Option<String> {
    if let Ok(cc) = env::var("CC") {
        if !cc.trim().is_empty() {
            return Some(cc);
        }
    }
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
