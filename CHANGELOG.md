# Changelog

All notable changes to MAGIC. Versions follow SemVer 2.0.0; before 1.0, minor bumps may break.

## Unreleased

**Toolchain**
- One-command installers (`install.ps1`, `install.sh`) that build from source and put `magic` and
  `magicc` on PATH.
- `-v`, `-version`, `-V`, `--version` and `version` all print the version.

**Docs**
- Dual MIT/Apache-2.0 license files, a `CONTRIBUTING.md`, and a compact full benchmark table in
  `README.md`.

## v0.4 — structs of structs, growable arrays, strings

**Data**
- Nested struct fields; field paths on variables (`o.i.a`) and on array elements (`os[1].i.a`), read and write.
- Growable arrays `[T+]` for numeric, `bool`, and struct elements, with `push`, `pop`, `reserve`, `clear`, `len`.
- Growable arrays as parameters (`fn f(v: [T+])` → `T **v, int64_t *v_len, int64_t *v_cap`), push/pop mutate the caller.
- A vec can be passed to a fixed-array parameter as a read-only `(ptr, len)` view.

**Strings** (read-only, `const char*`)
- `str_eq`, `str_len`, `str_byte` (bounds-checked), `str_byte_n` (O(1) per byte, caller supplies the length).

**Numeric**
**Language**
- `else if` chains: `if a { } else if b { } else { }` (the else branch may be another `if`).
- `abs` for any numeric type (call-free argument; negating the minimum integer overflows).
- `min(a, b)` and `max(a, b)` for matching numeric types (call-free arguments, ternary codegen).

**Toolchain**
- `install.ps1` (Windows) and `install.sh` (Linux/macOS) install with one command; they build from
  source and copy both `magic` and `magicc` onto PATH.
- `-v`, `-version`, `-V`, `--version` and `version` all print the version.
- Several source files compile as one program: `magicc build main.magic lib.magic`.
- `import "lib.magic";` pulls files in, deduplicated by canonical path and dependencies-first
  (no namespaces yet). Diagnostics are mapped back to the file and line that produced them.
- `$CC` may carry arguments, e.g. `zig cc`; the unrolled n-body reaches 206–222 ms under LLVM
  (Rust 219–221) and 253 ms under GCC. `--cc "zig cc"` overrides `$CC` per invocation, and
  `-o <path>` names the executable (the harness uses both for its zig-backed n-body case).
- `--unroll` fully unrolls small constant `for` loops (≤ 8 iterations, no `break`/`continue`).
  It is a win with clang/zig (n-body 205–236 ms vs Rust 219–221) and a loss with gcc (~440 ms),
  so it is opt-in.
- `magicc --version`, `--help`; the usage line reads the crate version.
- `--report` sizes array counts written as expressions (`n * n`) and struct elements (C layout, including padding).
- `-fno-asynchronous-unwind-tables -fno-ident` by default: `big.exe` 201,673 → 167,105 bytes, one-function rebuild 280 → 271 ms.

**Verification**
- 123 tests: 44 unit, 22 CLI, 15 integration, 42 end-to-end.
- `verify.ps1` (+ `-Fast`), `check-docs.ps1`, `bench-all.ps1` (44 cases, 43 without Zig), `bench-memory.ps1`.

## v0.3 — the memory model

- `arena { }`: one allocation, one free, 8-byte-aligned bump; per-arena row in `--report`.
- Printed static memory bound per function, verified as a lower bound against peak RSS.
- `expr as Type` casts; `assert cond;` (exit 1 with the line).
- Constant folding for array sizes inside arenas.

## v0.2 — surface and structs

- Struct declarations, literals, field read/write; struct parameters and return values.
- Array parameters lowered to `(ptr, len)`.
- `for i in a..b`, `break`, `continue`.
- Compile cache (content hashing) and 16-way parallel chunks for files with 32+ functions.

## v0.1 — arrays

- `var a: [T] = [v; n];` with bounds-checked indexing; compiler-inserted frees.
- `let`/`var`, `if`/`else`, `while`, functions, `print`.
- Byte-exact example outputs; sieve to 100M.

## Benchmarks at v0.4 (median, one Windows machine)

| Workload | C | Rust | MAGIC |
|---|---:|---:|---:|
| sieve 100M | 413 | 428 | **413** |
| fannkuch | 175 | 158 | **143** |
| matmul 384 | 31 | 47 | **30** |
| n-body 10M | **303** | **222** | 319 |
| ray tracer 1536² | 46 | 47 | 46 |
| hash 1M | 47 | 46 | 46 |
| particles 1M × 100 | **79** | 143 | **78** |
| binary trees | — | 1357 | **732** |

MAGIC is first or tied on seven of eight workloads and never meaningfully slower than C. n-body is
the one loss (~220 ms Rust vs ~300 ms C), and it is **not** a GCC-vs-LLVM gap: `zig cc` (clang/LLVM)
on the identical C is also ~300 ms. Provenance and caveats: `STATUS.md`.
