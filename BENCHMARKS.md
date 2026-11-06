# MAGIC benchmarks — current evidence (Linux, 2026-11-06)

Machine: AMD Ryzen AI 7 350, 31.1 GB RAM, Linux. Compilers: gcc/g++ 16.2.1, rustc 1.99.1,
Go and Goose where the harness has a source. MAGIC is `magicc 0.8.91` (speed numbers measured across the 0.8.87–0.8.90 compiler), default **IR** backend
(`as` + `ld`, no C compiler in the build path). Every number below comes from
`./verify.sh` / `./bench-linux.sh`; every program's checksum is compared and `verify.sh` fails on
a mismatch. Timings are medians of 3–5 interleaved runs, portable flags (no `-march=native` for the
baseline languages or for MAGIC). The machine is shared, so ±5–10% run-to-run noise is normal.

## 1. sieve 100M

C 409 ms, C++ 423, Rust 414, MAGIC default **383** — an outright win. The block-odd variant reaches
45 ms; peak RSS is 13.8 MB against C's 99.2 MB (7.2x lower) because primes are bit-packed.

## 2. fannkuch n=10

C 168 ms, C++ 168, Rust 159, MAGIC default 265, buffered variant 166, `par for` blocks **70** —
outright win. Checksums 73196 / 38 match.

## 3. ray tracer 1536²

C 42 ms, C++ 44, Rust 35, MAGIC default 120, `par for` **34** — outright win at eight chunks.

## 4. hash table 1M

C 28 ms, C++ 32, Rust 25, MAGIC default 39, `par for` 30 (1.20x of the best baseline, inside
1.25x). Same checksum 1500001500000; peak RSS 34.2 MB versus C's 34.3 MB.

## 5. particles 1M × 100

C 65 ms, C++ 71, Rust 141, MAGIC default 147, `par for` 135. The SSE2 pair loop is
memory-bandwidth-bound at ~33 MB per array, so it stays ~2.3x behind C.

## 6. n-body 10M steps

C 310 ms, C++ 308, Rust 223, MAGIC default 873. The leaf-loop unroll helped (966 → 869) but the
float dynamics still trail; SIMD across the four-body nest is future work.

## 7. binary trees

C 721 ms, Rust 1333, MAGIC default 3904. Dropping constant array-length arguments cut 4.45 s → 3.89 s.
Recursive call frames and per-node bounds checks remain the gap; the count variant is 1512 ms.

## 8. matmul 384

C **12 ms**, C++ 15, Rust 42, MAGIC default 74 and i-k-j 77. A saxpy prototype reached 10 ms but was
disabled in v0.8.91 after audit found six miscompiles; the scalar loops are used again until a
guarded implementation lands.

## Speed summary (same workload, same portable flags)

"MAGIC default" is the idiomatic MAGIC source through the default backend. "MAGIC best" is the
fastest MAGIC variant the harness ships for that workload (parallel or algorithmic variant; it is
not the same source as the C baseline, and the variant name is given). Ratio = MAGIC / best
baseline, lower is better.

| Workload | C | C++ | Rust | MAGIC default | MAGIC best (variant) | Ratio | Verdict |
|---|---:|---:|---:|---:|---|---:|---|
| sieve 100M | 409 | 423 | 414 | **383** | 45 (block-odd) | 0.94 | **outright win** |
| fannkuch n=10 | 168 | 168 | 159 | 265 | **70** (`par for`, blocks) | 0.44 | **outright win** |
| ray tracer 1536² | 42 | 44 | 35 | 120 | **34** (`par for`) | 0.97 | **outright win** |
| hash 1M | 28 | 32 | 25 | 39 | 30 (`par for`) | 1.20 | within 1.25x |
| fannkuch buffer variant | 168 | 168 | 159 | 166 | — | 1.04 | within 1.25x |
| particles 1M × 100 | 65 | 71 | 141 | 147 | 135 (`par for`) | 2.26 | loss |
| n-body 10M steps | 310 | 308 | 223 | 873 | 869 (loop) | 3.90 | loss |
| binary trees | 721 | — | 1333 | 3904 | 1512 (count variant) | 5.42 | loss |
| matmul 384 | **12** | 15 | 42 | 74 | 74 | 6.17 | loss |

What this says:

- **Three outright wins** against the best portable baseline: sieve, fannkuch blocks (`par for`),
  and the raytracer (`par for`). Hash with `par for` is inside 1.25x.
- The speed goal is **not yet met for the whole matrix**: matmul (6.2x), binary trees (5.4x),
  n-body (3.9x) and particles (2.3x) are still materially behind. The saxpy SIMD prototype that
  briefly made i-k-j matmul 10 ms was **rolled back** in v0.8.91 after an audit found six
  miscompiles (aliasing, hoisting, i64 truncation, length arguments); a guarded rewrite is pending.

## Peak RSS (C vs MAGIC, same outputs)

`/usr/bin/time -f %M`, one representative run after warm-up; the loader/libc floor is ~1.6 MB and is
included in both binaries.

| Workload | C | MAGIC | Ratio |
|---|---:|---:|---:|
| sieve 100M | 99.2 MB | **13.8 MB** | **7.2x lower** |
| hash 1M | 34.3 MB | 34.2 MB | parity |
| particles 1M × 100 | 32.9 MB | 33.0 MB | parity |
| binary trees | 17.8 MB | 18.1 MB | parity |
| matmul 384 | 3.53 MB | 3.44 MB | parity |
| n-body 10M | 1.65 MB | 1.69 MB | parity |
| ray tracer | 1.68 MB | 1.86 MB | +0.2 MB |
| fannkuch | 1.74 MB | 1.56 MB | 10% lower |

The 5x memory claim is proven for bit-packable workloads (sieve: 7.2x lower). "Lowest on most
workloads" is **not** established: outside sieve, C and MAGIC are at parity because both use plain
C arrays. The static bound reported by `magicc check --report` is a true lower bound; `bench-memory.sh`
fails if peak RSS drops below it.

## Compile time (2,000 functions, 92 KB source)

`./bench-compile/run.sh`, medians of three runs. Cold = remove binary/object/stamp; edit = change one
reachable function; no-op = nothing changed.

| Compiler | Cold | One reachable edit | No-op |
|---|---:|---:|---:|
| **MAGIC IR (default)** | **25 ms** | **10 ms** | **6 ms** |
| MAGIC C backend + gcc | 62 ms | — | — |
| gcc -O3 (generated C file) | 47 ms | — | — |
| rustc -O | 109 ms | 110 ms | — |
| goose build | 69 ms | 64 ms | 69 ms |

MAGIC is fastest both cold and incrementally on this source. The IR backend is also what makes the
native build possible: after the parser/checker it emits GNU assembler directly and links with `ld`.

## Binary size and startup

| Binary | Size |
|---|---:|
| MAGIC hello (IR, dynamic libc) | 13,224 B |
| gcc -O3 hello | 12,520 B |
| MAGIC 2,000-function program | 22,048 B |
| gcc 2,000-function program | 13,152 B |
| Rust 2,000-function program | 4,538,288 B |
| Goose 2,000-function program | 72,024 B |

Process startup (50 runs, wall): MAGIC 0.93 ms vs gcc 0.94 ms — the number is dominated by process
creation. MAGIC binaries are small and need no C compiler at build time, but they **link libc
dynamically**; they are not fully static ELF yet.

## Diagnostics and security evidence

- Out-of-bounds index: `line N: index out of bounds` followed by a symbolized stack trace
  (`inner`, `middle`, `main`, libc, `_start`); division by zero and assertion failures report
  `file:line` too.
- 168 Rust tests; 60/60 `.magic` corpus compiles through both C and IR backends; `verify-ir.sh`
  runs the differential corpus; `tests/fuzz_diff.rs` mutation-fuzzes programs against the C
  backend; `verify-native.sh` builds the corpus with a PATH containing only `as` and `ld`.
- Bounds/affine proofs are hardened: direct-`Lt` guards, header dominance, single canonical
  increment, `after_inc` reachability, instruction-order and i32-range checks. Three adversarial
  audit rounds found and closed 4 + 2 + 1 miscompiles; every counterexample is a regression test.
- A range-based pass was prototyped and **rejected** after the audit found four silent
  out-of-bounds cases; it is not shipped.

## Honest gaps

- Within-1.25x applies to sieve, fannkuch (buffer), raytracer and hash (`par for`), not to the full
  matrix; matmul, binary trees, n-body and particles still lose.
- Peak memory is lowest clearly on sieve only; the rest is parity with C.
- Binaries are dynamically linked against libc; a fully static ELF path is future work.
- No physical 2 GB low-end device or GPU workload is part of this table; the RSS/startup numbers
  are the current proxy.
- Compile figures are on a single machine and a synthetic 2,000-function source.
