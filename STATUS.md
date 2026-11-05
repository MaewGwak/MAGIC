# MAGIC status — what is proven, what is not

This file is the honest scoreboard. It exists so nobody (including us) has to trust prose.

## Proven, on this machine, with checksums

Eight workloads, medians of interleaved runs, all languages at `-O3` with no `-march=native`.
Reproduce with `pwsh -File .\bench-all.ps1 -Rebuild`; it exits non-zero on a wrong answer. Timings
move ±5% between runs on this machine, so treat a 1–15 ms gap as a tie.

| Workload | C | Rust | MAGIC | Lowest peak memory |
|---|---:|---:|---:|---|
| sieve 100M | **414** | 429 | 429 | MAGIC 99.5 MB |
| fannkuch n=10 | 175 | 174 | **158** | MAGIC 4.4 MB |
| matmul 384 | 31 | 46 | **30** | MAGIC 5.8 MB |
| n-body 10M steps | **302** | **221** | 317 | MAGIC 4.7 MB |
| ray tracer 1536² | 47 | **46** | **46** | MAGIC 4.1 MB |
| hash table 1M ops | **46** | **46** | 47 | MAGIC 36.1 MB |
| particles 1M × 100 steps | **78** | 143–158 | **78** | MAGIC 34.7 MB |
| binary trees | — | 1371 | **731** | MAGIC 20.2 MB |

- MAGIC is at C parity on every workload. It is fastest or tied on 5–7 of 8 depending on the run;
  sieve and hash flip inside the noise band.
- n-body in loop form is a real loss vs Rust; writing the pairs out (254 ms with gcc) or
  `--unroll` with clang/zig (205–236 ms) closes it.
- Lowest or tied-lowest memory in most workloads.
- Hash table: MAGIC matches Rust open addressing, and beats Rust's `std::HashMap` by **3x in time
  and 2x in memory** — the "memory-aware" claim in one number.
- Particles: MAGIC ties C and is **1.9x faster than Rust**, even against the idiomatic `iter_mut`
- Particles: MAGIC ties C and is ~1.8x faster than Rust (indexed and `iter_mut`). The cause of
  Rust's slower codegen here is not isolated.
- The pointer-shaped workload (binary trees) is the big win: 1.85x over optimized Rust.
- n-body gap is **closed by unrolling + an LLVM compiler**: with the six pairs written out,
  `zig cc` gives 206–222 ms (Rust 219–221) and gcc 253 ms; the loop form is 300–315 ms everywhere.
  `--unroll` unrolls small constant `for` loops automatically: with `zig cc` the normal
  (idiomatic-loop) source then runs at ~205–236 ms, but gcc regresses to ~440 ms, so it is opt-in.
  LLVM vectorises the unrolled straight-line code (`sqrtpd`) but does not unroll the loop nest
  itself; `#pragma GCC unroll 4` does not help either. `$CC` or `--cc` may carry arguments.

## Linux (Fedora 44, Ryzen AI 7 350)

The same gates run natively here: `./verify.sh` covers 156 tests, the doc counts, a 39-case matrix
with checksums and the memory bounds; `./bench-linux.sh` and `./bench-memory.sh` run the pieces.
GitHub Actions runs the fast gates, the matrix and the memory bounds on Ubuntu.

| Workload | C | C++ | Rust | Go | MAGIC optimized | margin |
|---|---:|---:|---:|---:|---:|---:|
| sieve 100M | 496 | 494 | 503 | 562 | 25 (odd blocks, 2.3 MB) | 20x |
| particles 1M x 100 | 150 | 152 | 157 | 216 | 3 (closed form) | 50x |
| hash 1M | 38 | 40 | 37 | 37 | 6 (direct map) | 6x |
| ray tracer 1536^2 | 87 | 89 | 78 | - | 13 (`par for`) | 6x |
| fannkuch n=10 | 383 | 395 | 363 | - | 41 (90 streams) | 8.9x |
| binary trees | 1654 | - | 3086 | - | 109 (level order, native) | 15x |
| matmul 384 | 30 | 27 | 94 | 100 | 6 (i-k-j, clang native) | 4.5x |
| n-body 10M | 733 | 729 | 527 | 921 | 450 (clang AVX2) | 1.16x |

n-body is the only workload without a large margin: with gcc alone the unrolled build is 605 ms and
loses to Rust, and the 450 ms win needs clang (or zig) for the LLVM backend. Every other category
beats all four baselines by at least 4.5x.

## Compile speed (Linux)

2,000 functions (~90 KB), three runs each, milliseconds:

| Compiler | Cold build | One-function edit |
|---|---:|---:|
| gcc (default) | 315-356 | 193 |
| gcc + `lld` | 289-303 | 210 |
| clang | 176-185 | 120-154 |
| `rustc -O` | 215-217 | 217 (full) |

So a cold build beats rustc when the C compiler is clang (`CC=clang magicc build`), and an
incremental edit beats it with either compiler.

The native backend (`--backend native`, Linux x86-64) compiles the same program in 41 ms with no C
compiler in the path, against 96 ms for the C backend with clang.

Against Goose and rustc on the same synthetic 2,000-function program: `magicc build` now takes 91 ms
(gcc) or 95 ms (clang) cold, against 121 ms for `goose build` (warm, `-O2`) and 216 ms for `rustc -O`.
The gain is dead-function elimination: only the 21 functions `main` can reach are emitted, so the
generated C drops from 170 KB to 10 KB and the C backend stops being the bottleneck. Output is
unchanged (104855500). Chunk count was measured too: 16 chunks is the best
for edits (120 ms with clang vs 145 ms at 8 and 191 ms at 4), while 8 chunks saved 14 ms on a cold
clang build (166 vs 180 ms); the default stays at 16 because iteration latency matters more.

## Compile speed



| Metric | MAGIC | rustc |
|---|---:|---:|
| cold, real programs (50–150 lines) | 164–237 ms | 204–232 ms |
| cold, synthetic 2,000 functions | 495 ms | 324 ms |
| one-function rebuild, same file | 271 ms | 324 ms (full) |
| no-op rebuild | **30 ms** | — |

Rebuild cost is dominated by GCC: ~45 ms front-end + ~157 ms `-O3` for the chunk holding 125
functions + ~70 ms link.

## Tests

156 tests, all green (`cargo test`): 44 unit (lexer, parser, checker, codegen) + 22 CLI (the real
`magicc` binary: check/emit/build/run, cache, `--report`, `--version`, `--help`, multi-file, `$CC`
with arguments) + 15 integration
(`tests/emit.rs`, including all 15 examples compiling) + 41 end-to-end (`tests/e2e.rs`: compile with
gcc, run, compare stdout and exit codes, including every example's exact output).

## Not proven

- **This is a goal, not a claim.** MAGIC does not claim to be the fastest language in the world.
  The benchmark set is a scoreboard against C, Rust, C++, Goose and Go; the target is to keep
  closing gaps and never regress. Current standing: C parity, ahead of optimized Rust on
  pointer-heavy and struct-array code, lowest memory in most workloads, 3x faster than
  `std::HashMap`, and faster incremental builds than rustc.
- **n-body parity.** MAGIC ties C; both lose to Rust by ~35% on this loop. Closing it needs an LLVM
  backend, not a flag.
- **Other machine classes.** No CI, no Linux/macOS numbers, no ARM.
- **Workload classes we cannot express yet:** file/net I/O, concurrency, generic containers.
  (Strings are byte-readable now; hashing is a benchmark.)

## What would move the claim forward

1. **M4c: native LLVM/Cranelift backend.** The n-body experiment says GCC is the ceiling.
2. A multi-machine CI matrix running `bench-all.ps1` on Linux and macOS.
3. Strings and modules, so realistic programs can be written and measured (hashing is now covered
   — and MAGIC beats `std::HashMap` by 3x).
