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
             usage: magicc <check|emit|build|run> <file.magic> [--native] [--report] [--dev] [--cc \"cmd\"] [-o out]\n\n\
             \x20 check   parse + type check only\n\
             \x20 emit    print the generated C to stdout\n\
             \x20 build   write <file>.c and compile it\n\
             \x20 run     build, then run\n\n\
             \x20 --native  add -march=native to gcc\n\
             \x20 --report  print the static memory bound\n\
             \x20 --dev     compile at -O0 for fast iteration\n\
             \x20 --cc      C compiler command, may carry arguments (e.g. \"zig cc\")\n\
             \x20 --unroll  unroll small constant for-loops (wins with clang/zig, loses with gcc)\n\
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
            "usage: magicc <check|emit|build|run> <file.magic> [--native] [--report] [--dev]"
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

    let c = magic::compile_to_c_opts(&source, unroll)
        .map_err(|e| map_diagnostic(e, &segments, src_path))?;

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
            println!("🔮 ok: {}", src_path.display());
            Ok(())
        }
        "emit" => {
            print!("{}", c);
            Ok(())
        }
        "build" | "run" => {
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
                "{}{}-nou",
                if dev { "O0" } else { "O3" },
                if native { "-native" } else { "" }
            );
            let stamp = dir.join(format!("{}.flags", stem));
            let flags_changed = fs::read_to_string(&stamp)
                .map(|s| s.trim() != flags)
                .unwrap_or(true);
            let chunks = magic::compile_parts_opts(&source, 16, unroll)
                .map_err(|e| map_diagnostic(e, &segments, src_path))?;
            let cc = cc_override
                .clone()
                .or_else(find_cc)
                .ok_or("no C compiler found (tried $CC, gcc, clang, cc)")?;
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

                if unchanged && exe_path.exists() && !flags_changed {
                    println!("🔮 up to date {}", exe_path.display());
                } else {
                    fs::write(&c_path, &chunks[0])
                        .map_err(|e| format!("cannot write {}: {}", c_path.display(), e))?;
                    let mut cmd = cc_command(&cc);
                    cmd.arg(if dev { "-O0" } else { "-O3" })
                        .arg("-std=c11")
                        .arg("-fno-asynchronous-unwind-tables")
                        .arg("-fno-ident");
                    if native {
                        cmd.arg("-march=native");
                    }
                    let status = cmd
                        .arg("-o")
                        .arg(&exe_path)
                        .arg(&c_path)
                        .status()
                        .map_err(|e| format!("failed to run {}: {}", cc, e))?;
                    if !status.success() {
                        return Err(format!("{} failed while compiling {}", cc, c_path.display()));
                    }
                    println!("🔮 built {}", exe_path.display());
                    let _ = fs::write(&stamp, &flags);
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
                        handles.push(std::thread::spawn(move || {
                            let mut cmd = cc_command(&cc2);
                            cmd.arg(if dev { "-O0" } else { "-O3" })
                                .arg("-std=c11")
                                .arg("-fno-asynchronous-unwind-tables")
                                .arg("-fno-ident")
                                .arg("-c");
                            if native2 {
                                cmd.arg("-march=native");
                            }
                            cmd.arg("-o").arg(&o_path).arg(&c_path);
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
                    let _ = fs::write(&stamp, &flags);
                }
            }

            if command == "run" {
                let status = Command::new(&exe_path)
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
        match fs::write(path, content) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = format!("cannot write {}: {}", path.display(), e);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Err(last)
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
