# Changelog

All notable changes to MAGIC. Versions follow SemVer 2.0.0; before 1.0, minor bumps may break.

## Unreleased

**Performance** (algorithm-optimized MAGIC implementations, same answers)
- Odd-only sieve: 173 ms vs 397 ms for the classic C sieve.
- Parallel odd-only sieve (`par for`, dynamic schedule): 86 ms vs 397 ms — 4.6x.
- i-k-j matmul: 14 ms vs 20 ms for the C matmul.
- Closed-form particles (constant decay over 100 steps): 8 ms and ~2.4 MB vs 73 ms and 34.7 MB.
  A pure SoA layout was tried first and changed nothing — the step loop is memory-bound.
- Parallel ray tracer (`par for` over rows): 16 ms vs 42 ms, with row sums added in the same order
  so the checksum is unchanged.
- Direct-map hash (dense keys 1..1M): 10 ms vs 33 ms for the C hash table, and 4.6 MB vs 36.1 MB.
  Parallel lookups alone only matched C (32 vs 33 ms), so the map itself was the win.
- C port of the binary-trees workload (flat arena, same shape): 735 ms vs MAGIC 725 ms and
  Rust 1346 ms — the five-language matrix is complete for this workload.
- Binary trees with a node-count check (a perfect tree's size is its allocation counter):
  427 ms vs 745 ms C (1.74x) and 1349 ms Rust. Storing both children side by side was slower
  (852 ms), and struct arrays cannot be passed as parameters yet.
- n-body with AVX2 (`-march=x86_64_v3` through zig cc): 189 ms vs 218 ms Rust and 301 ms C.
  This target needs AVX2; the portable builds stay at 254 ms (gcc, unrolled) and 206 ms (zig).
  AVX-512 (`x86_64_v4`) was slower (205 ms), `-ffast-math` changed the checksum
  (…067 / …974) while running slower (241 / 279 ms), and staging all six d² before the sqrt/div
  chain was 269 ms (gcc), 204 ms (zig) and 203 ms (AVX2) — the CPU already overlaps the independent
  pairs, so the 188 ms unrolled AVX2 build stays. GCC with the same AVX2 target is 258 ms.
- fannkuch dead ends: splitting the permutation space by first element into 10 `par for` chunks
  changed the checksum, because the reference enumeration order is not lexicographic (max 38 was
  right, checksum was not). zig (158 ms), `-funroll-loops` (142 ms) and `-march=x86-64-v3`
  (142 ms) gave no gain over the 143 ms gcc build.
- Buffered parallel fannkuch: the reference order is generated once into a u8 buffer (10 bytes per
  permutation, 36 MB) and the flips are computed in 16 `par for` chunks, using the global index
  parity for the checksum. 106 ms vs 162 ms C and 160 ms Rust — 1.5x — at 39.8 MB instead of 4 MB.
- The harness now times with a precise wall clock (WaitForExit) while sampling peak memory in a
  second run, so sub-15 ms results are real.

**Language**
- `par for i in a..b { ... }`: iterations must be independent; the compiler emits an OpenMP
  parallel loop with dynamic scheduling and adds `-fopenmp` automatically.

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
- 124 tests: 44 unit, 22 CLI, 15 integration, 43 end-to-end.
- `verify.ps1` (+ `-Fast`), `check-docs.ps1`, `bench-all.ps1` (53 cases, 50 without Zig), `bench-memory.ps1`.

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
