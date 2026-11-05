# MAGIC status — what is proven, what is not

This file is the honest scoreboard. It exists so nobody (including us) has to trust prose.

## Proven, on this machine, with checksums

Eight workloads, medians of interleaved runs, all languages at `-O3` with no `-march=native`.
Reproduce with `pwsh -File .\bench-all.ps1 -Rebuild`; it exits non-zero on a wrong answer.

| Workload | C | Rust | MAGIC | Lowest peak memory |
|---|---:|---:|---:|---|
| sieve 100M | 412 | 413 | **398** | MAGIC 99.5 MB |
| fannkuch n=10 | 187 | 174 | **157** | MAGIC 4.1 MB |
| matmul 384 | **30** | 46 | **30** | C/MAGIC 5.8 MB |
| n-body 10M steps | **312** | **219** | 312 | MAGIC 4.1 MB |
| ray tracer 1536² | **46** | 61 | 47 | MAGIC 4.1 MB |
| hash table 1M ops | 47 | 31 | **31** | MAGIC 36.1 MB |
| particles 1M × 100 steps | **79** | 142–157 | **79** | MAGIC 34.7 MB |
| binary trees | — | 1352 | **733** | MAGIC 20.1 MB |

- MAGIC is first or tied on seven of eight workloads, and at C parity within noise on the eighth.
- Lowest or tied-lowest memory in most workloads.
- Hash table: MAGIC matches Rust open addressing, and beats Rust's `std::HashMap` by **3x in time
  and 2x in memory** — the "memory-aware" claim in one number.
- Particles: MAGIC ties C and is **1.9x faster than Rust**, even against the idiomatic `iter_mut`
- Particles: MAGIC ties C and is ~1.8x faster than Rust (indexed and `iter_mut`). The cause of
  Rust's slower codegen here is not isolated.
- The pointer-shaped workload (binary trees) is the big win: 1.85x over optimized Rust.
- n-body gap is **closed by unrolling + an LLVM compiler**: with the six pairs written out,
  `zig cc` gives 206–222 ms (Rust 219–221) and gcc 253 ms; the loop form is 300–315 ms everywhere.
  LLVM vectorises the unrolled straight-line code (`sqrtpd`) but does not unroll the loop nest
  itself; `#pragma GCC unroll 4` does not help either. `$CC` or `--cc` may carry arguments.

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

120 tests, all green (`cargo test`): 44 unit (lexer, parser, checker, codegen) + 19 CLI (the real
`magicc` binary: check/emit/build/run, cache, `--report`, `--version`, `--help`, multi-file, `$CC`
with arguments) + 15 integration
(`tests/emit.rs`, including all 14 examples compiling) + 41 end-to-end (`tests/e2e.rs`: compile with
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
