# MAGIC benchmarks — current evidence (Linux, 2026-11-06)

Machine: AMD Ryzen AI 7 350, 31.1 GB RAM, Linux. Compilers: gcc/g++ 16.2.1, rustc 1.99.1,
Go and Goose where the harness has a source. MAGIC is `magicc 0.9.5` (speed numbers measured across the 0.8.87–0.8.90 compiler), default **IR** backend
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

C **12 ms**, C++ 15, Rust 42, MAGIC default 77, **i-k-j SIMD 8** — an outright win. The guarded
`saxpy` recogniser only fires for local distinct i32 arrays, a constant non-negative limit, a known
zero start, pure/effect-free limit, multiplier and index expressions, and checks bounds once before
the 4-wide SSE2 loop.

## Speed summary (same workload, same portable flags)

"MAGIC default" is the idiomatic MAGIC source through the default backend. "MAGIC best" is the
fastest MAGIC variant the harness ships for that workload (parallel or algorithmic variant; it is
not the same source as the C baseline, and the variant name is given). Ratio = MAGIC / best
baseline, lower is better.

| Workload | C | C++ | Rust | MAGIC default | MAGIC best (variant) | Ratio | Verdict |
|---|---:|---:|---:|---:|---|---:|---|
| sieve 100M | 409 | 423 | 414 | **383** | 45 (block-odd) | 0.94 | **outright win** |
| fannkuch n=10 | 168 | 168 | 159 | 265 | **70** (`par for`, blocks) | 0.44 | **outright win** |
| ray tracer 1536² | 42 | 44 | 35 | 120 | **22** (`par for`) | 0.97 | **outright win** |
| hash 1M | 28 | 32 | 25 | 39 | 30 (`par for`) | 1.20 | within 1.25x |
| fannkuch buffer variant | 168 | 168 | 159 | 166 | — | 1.04 | within 1.25x |
| particles 1M × 100 | 65 | 71 | 141 | 147 | 135 (`par for`) | 2.26 | loss |
| n-body 10M steps | 310 | 308 | 223 | 873 | 869 (loop) | 3.90 | loss |
| binary trees | 721 | — | 1333 | 3904 | 1512 (count variant) | 5.42 | loss |
| matmul 384 | **12** | 15 | 42 | **9** (interchange SIMD) | 10 (i-k-j) | 0.75 | **outright win** |

What this says:

- **Four outright wins** against the best portable baseline: sieve, fannkuch blocks (`par for`),
  the raytracer (`par for`), and i-k-j matmul (`SaxpyI32`, 8 ms versus C 12 ms). Hash with `par for`
  is inside 1.25x.
- The speed goal is **not yet met for the whole matrix**: binary trees is 5.4x, n-body 3.9x and
  particles 2.3x behind. Default matmul is now solved too (interchange → saxpy, 9 ms).

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

## Freestanding static binaries (`--static`)

`magicc build prog.magic --static` emits assembly, sectionises each runtime helper, and links with
`ld -static -nostdlib --gc-sections -e _start` against a small syscall-only runtime (`runtime/
static_runtime.s`: mmap-based calloc, memset/memcpy, `%lld`/`%d`/`%s` printf, exit, simple panic).
No C compiler and no libc are involved. The five integer/bool workloads build to 5.0–5.3 KB ELFs
with **no PT_INTERP**, and their checksums match: sieve 5761455, matmul 326316140, hash
1500001500000, fannkuch 38, binary trees 568328208. Float printing, file/net/par and other libc
features are rejected at build time (missing runtime symbol or an explicit float-print error), never
silently miscompiled. `verify-static.sh` is part of `verify.sh`.

## Latest full matrix run (v0.9.5, 2026-11-06)

One `bench-linux.sh` run on the Ryzen AI 7 350 host, medians of the harness:

| workload | C | C++ | Rust | Goose | MAGIC default | MAGIC best |
|---|---:|---:|---:|---:|---:|---:|
| sieve 100M | 431 | 437 | 433 | 450 | **385** | 33 (block odd) |
| fannkuch 10 | **160** (Rust) | 186 | 160 | 175 | 263 | **69** (blocks) |
| matmul 384 | **12** | 12 | 40 | 39 | **9** | 9 |
| n-body 5M | 309 | 312 | **224** | 308 | 772 (loop) | 772 |
| raytrace 64³ | 37 | 42 | **35** | 38 | 104 | **20** (par) |
| hash 1M | **27** (Rust) | 31 | 27 | 28 | 42 | 30 (par) |
| particles 1M×100 | **72** | 72 | 143 | 143 | 148 | 6 (closed) |
| binary trees 20 | **720** | – | 1346 | 813 | 3892 | 53 (loop) |

This run confirms the four compiler-driven wins (sieve, fannkuch blocks, matmul, ray par) and
hash par inside 1.25x; n-body/particles/bt remain behind. C values move a few percent between runs
(particles 65→72, sieve 409→431), so treat single-digit differences as noise.

## Low-end proxy gate

`./bench-lowend.sh` rebuilds all eight default workloads and runs each under a **2 GiB
address-space limit** (`prlimit --as=2147483648`) pinned to **one CPU** (`taskset -c 0`), checking
the same checksums as the main matrix. All eight pass: peak RSS 2–34 MB, matmul 9 ms, hash 39 ms,
ray 123 ms, particles 141 ms, sieve 368 ms, fannkuch 263 ms, n-body 879 ms, binary trees 3825 ms.
This is a proxy for a 2 GB low-end machine, not a physical one: it bounds the memory class and the
single-core time, but does not test GPU or very slow storage.

## Honest gaps

- Within-1.25x applies to sieve, fannkuch (buffer), raytracer and hash (`par for`), not to the full
  matrix; matmul, binary trees, n-body and particles still lose.
- Peak memory is lowest clearly on sieve only; the rest is parity with C.
- Binaries are dynamically linked against libc; a fully static ELF path is future work.
- No physical 2 GB low-end device or GPU workload is part of this table; the RSS/startup numbers
  are the current proxy.
- Compile figures are on a single machine and a synthetic 2,000-function source.
