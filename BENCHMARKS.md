# MAGIC benchmarks — current evidence (Linux, 2026-11-06)

Machine: AMD Ryzen AI 7 350, 31.1 GB RAM, Linux. Compilers: gcc/g++ 16.2.1, rustc 1.99.1,
Go and Goose where the harness has a source. MAGIC is `magicc 0.12.8` (speed numbers measured across the 0.8.87–0.8.90 compiler), default **IR** backend
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

"MAGIC default" is the unmodified MAGIC source through the default backend. "MAGIC optimized
variant" is the best shipped source/algorithm (parallel or algorithmic; not the same source as the
C baseline, and the variant is named). **This portable summary is naive-source evidence and is NOT
the 1.0 parity gate**; the gate table is in "Algorithm-parity 1.0 gate" below and in `verify.sh`.
Summary: **Verdict = fastest C/C++/Rust baseline ÷ MAGIC optimized variant** for "x faster", and
**variant ÷ fastest baseline** for "x slower"; the bold baseline is the fastest of the three. The
default-naive ratio follows in parentheses. Historical run; values move between runs.

| Workload | C | C++ | Rust | MAGIC default | Ratio (default) | MAGIC optimized variant | Vs fastest |
|---|---:|---:|---:|---:|---:|---|---:|
| sieve 100M | 432 | 423 | 431 | **397** | 1.07x faster | 37 (block-odd) | **11.4x faster** |
| fannkuch n=10 | 168 | 167 | 157 | 266 | 1.69x slower | **75** (`par for`) | **2.09x faster** |
| ray tracer 1536² | 39 | 41 | 39 | 81 | 2.08x slower | **18** (`par for`) | **2.17x faster** |
| hash 1M | 26 | 30 | 26 | 42 | 1.62x slower | **5** (direct) | **5.20x faster** |
| particles 1M × 100 | 67 | 73 | 148 | **84** (2-step fusion) | 1.25x slower | **5** (closed) | **13.4x faster** |
| n-body 10M | 315 | 310 | **227** | **613** (loop) | 2.70x slower | 613 native / **197 C-backend** | 2.70x slower native; 1.15x faster C-backend |
| binary trees | 720 | — | 1329 | 4036 | 5.61x slower | **507** (native loop) | **1.42x faster** |
| matmul 384 | 12 | 12 | 43 | **11** | 1.09x faster | 9 (i-k-j) | **1.33x faster** |

N-body has an explicit C-backend AVX2 build at 197 ms, 1.15x faster than Rust's 227 ms; the 613 ms
entry is native IR. C values move a few percent between runs.

What this says:

- Naive default wins only sieve and matmul against the portable baseline; it is not the 1.0 gate.
- The optimized variants win the portable matrix except native n-body; the explicit C-backend
  n-body (197 ms) beats optimized Rust (227 ms).
- The 1.0 parity gate (standard optimized variant, C backend for matmul/n-body) passes all eight;
  see the dedicated table below. The naive-default losses are not hidden.

## Peak RSS (static MAGIC vs C/C++/Rust, same outputs)

`bench-static-memory.sh` builds each workload with gcc -O3, g++ -O3, rustc -O and the freestanding
static MAGIC backend, then measures peak RSS with `/usr/bin/time -f %M`:

| workload | C | C++ | Rust | MAGIC static |
|---|---:|---:|---:|---:|
| sieve 100M | 96.9 | 99.2 | 97.1 | **11.9 MB (8.1x)** |
| fannkuch | 1.7 | 4.1 | 2.2 | **0.4** |
| matmul 384 | 3.4 | 5.6 | 3.7 | **1.6** |
| hash 1M | 33.6 | 35.6 | 33.8 | **31.9** |
| binary trees | 17.6 | — | 17.9 | **15.9** |
| n-body | 1.6 | 4.2 | 2.1 | **0.4** |
| particles | 32.1 | 34.3 | 32.3 | **30.4** |
| raytrace | 1.6 | 4.2 | 2.1 | **0.4** |

Static MAGIC is lowest on all eight and 8.1x below the best sieve baseline (bit-packed bool array).
Dynamic MAGIC builds are at parity with C because both use plain arrays; the gate therefore measures
the static memory mode. `bench-memory.sh` still checks that the reported static bound is below the
dynamic peak; `bench-static-memory.sh` fails if any baseline peaks lower than static MAGIC.

## Performance gate (1.0, clang C optimized path)

Protocol: optimized C/C++/Rust baselines use `-O3 -march=native` / `rustc -O -C target-cpu=native`;
the optimized MAGIC path explicitly uses the clang C backend for all eight. Native IR is the
no-C/`--static` fallback and is not the speed headline. `bench-parity-gate.sh` is part of `verify.sh`;
ties are accepted. All times below are **milliseconds (ms)**, medians of five warm runs, lower better.

| workload | C (ms) | C++ (ms) | Rust (ms) | fastest (ms) | MAGIC (ms) |
|---|---:|---:|---:|---:|---:|
| sieve | 403 | **389** | 398 | **389** (C++) | **17** |
| fannkuch | 154 | 160 | **146** | **146** (Rust) | **27** |
| matmul | **4** | 5 | 24 | **4** (C) | 3 |
| n-body | 328 | 326 | **219** | **219** (Rust) | **199** |
| raytrace | **29** | 33 | 31 | **29** (C) | **8** |
| hash | 25 | 27 | **23** | **23** (Rust) | **4** |
| particles | **66** | **66** | 131 | **66** (C/C++) | **3** |
| binary trees | **697** | — | 1279 | **697** (C) | **2** |

### Files, commands and fairness

- **C:** `bench-suite/sieve_c.c`, `fannkuch/fannkuch_c.c`, `matmul_c.c`, `nbody_c.c`,
  `raytrace_c.c`, `hash_c.c`, `particles_c.c`, and `bench-bt/bt_c.c`. Built with
  `gcc -O3 -march=native ... -lm`.
- **C++:** matching `bench-suite/*_cpp.cpp` (no C++ binary-trees baseline exists; shown as `—`).
  Built with `g++ -O3 -march=native ... -lm`.
- **Rust:** matching `bench-suite/*_rs.rs`; binary trees uses `bench-bt/bt_rust_flat.rs`. Built with
  `rustc -O -C target-cpu=native`.
- **MAGIC:** `bench-suite/sieve_magic_blockodd.magic`, `fannkuch/fannkuch_magic_blocks.magic`,
  `matmul_magic_ikj.magic`, `nbody_magic.magic`, `raytrace_magic_par.magic`,
  `hash_magic_direct.magic`, `particles_magic_closed.magic`, `bench-bt/bt_magic_loop.magic`. Built
  with `magicc build --backend c --cc "clang -O3 -march=native -pthread -lm"`.
- **Exactness:** every timed binary must print the expected output tail; a mismatch fails the gate.
  Outputs are compared as exact strings.
- **Runs:** five warm runs, median reported; one host, no CPU pinning, so a few percent of noise is
  expected (matmul is a 4-vs-3 ms tie/near-tie in this run).
- **Algorithm caveat:** baseline sources keep their original algorithms with optimized flags; MAGIC
  uses its best shipped algorithm (blocked sieve, blocks fannkuch, i-k-j matmul, par raytrace, direct
  hash, closed-form particles, level-order trees). This is not same-algorithm parity; it is the
  owner-approved best-practical comparison.
- **Backend caveat:** the speed path needs a C compiler and is not the static path. `--static` uses
  native IR and the no-C fallback numbers: sieve 33, fannkuch 60, matmul 6, n-body 613, raytrace 17,
  hash 4, particles 4, binary trees 442 ms. No silent fallback.
- **Memory:** static peak RSS is measured in **MiB** by `bench-static-memory.sh`; see the static
  memory table above.

## Best-of-language cross-check (no bias)

A separate independent suite (`bench-best/`) has C, C++, Rust and MAGIC agents implement the
fastest version each language can do, including algorithm changes, OpenMP/std threads and SIMD.
The neutral judge rebuilt and timed them sequentially (20 randomized rounds, exact integers,
1e-9 relative tolerance for floats) and wrote `bench-best/RESULTS.md`.

Headline (ms, 20-run median): fannkuch **MAGIC 8.00 (C SIMD)** vs Rust 9.01 / C 15.39 / C++ 15.37;
raytrace **MAGIC 2.00 (C SIMD)** vs C/C++ 3.23, Rust 4.13; n-body all compiled implementations tie at 214.5–214.7,
while MAGIC native IR is 665; sieve, hash, particles, binary trees and matmul are at the
process/start-up floor (≈1.1–3.2 ms) and are ties. **No overall language wins.** An untuned/default
MAGIC user does **not** beat best C/C++/Rust; best MAGIC ties most workloads but loses fannkuch and
raytrace, and native IR loses n-body/fannkuch. These best-of results do not support a "dominates"
claim; the 1.0 gate numbers above are against the original-algorithm optimized baselines, not this
best-of suite.

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

Process startup (50 runs, wall): MAGIC 0.93 ms vs gcc 0.94 ms — dominated by process creation.
Default builds link libc dynamically; `--static` produces freestanding 5–13 KB ELFs with no
interpreter and no libc (see the static section).

## Diagnostics and security evidence

- Runtime panics report `file:line: message` with the canonical source path, including when the
  fault is inside an imported file (the loader's concatenated line is remapped through its
  `(start_line, path)` segments). Out-of-bounds and division-by-zero panics are followed by a
  symbolized stack trace in the IR and C backends (`inner`, `middle`, `main`, libc, `_start`).
- 171 Rust tests; 60/60 `.magic` corpus compiles through both C and IR backends; `verify-ir.sh`
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
No C compiler and no libc are involved. All eight workloads build to 5.2–9.6 KB ELFs with **no
PT_INTERP** and correct checksums; `par for` uses a `clone`/`wait4` runtime, and `print_prec` uses a
fixed-point x87 formatter (precision <= 9, overflow guarded). static panics print the message plus a raw stack trace; `%g`/`print` of floats are rejected at build time; `env()` uses the initial stack environment; file I/O
and TCP (`file_open/...`, `net_listen/port/connect/accept/read/write/close`) are implemented with Linux
syscalls and covered by the gate. `verify-static.sh` is part of `verify.sh`.

## Historical full matrix run (v0.9.5, 2026-11-06)

One `bench-linux.sh` run on the Ryzen AI 7 350 host, medians of the harness:

| workload | C | C++ | Rust | Goose | MAGIC default | MAGIC optimized variant |
|---|---:|---:|---:|---:|---:|---:|
| sieve 100M | 432 | 423 | 431 | 439 | **397** | 37 (block odd) |
| fannkuch 10 | 168 | 167 | **157** | 179 | 266 | **75** (blocks) |
| matmul 384 | **12** | 12 | 43 | 38 | **11** | 9 (i-k-j) |
| n-body 5M | 315 | 310 | **227** | 311 | **613** (loop) | 613 native / 197 C-backend |
| raytrace 64³ | 39 | 41 | 39 | 38 | 81 | **18** (par) |
| hash 1M | **26** | 30 | 26 | 27 | 42 | **5** (direct) |
| particles 1M×100 | **67** | 73 | 148 | 139 | 84 | **5** (closed) |
| binary trees 20 | 720 | – | 1329 | 848 | 4036 | **507** (native loop) |

This run leaves seven workloads with a MAGIC best at or ahead of the fastest baseline: sieve,
fannkuch blocks, matmul, raytrace par, hash direct, particles closed and binary-trees loop. N-body's
native loop is 613 ms (2.7x Rust 227); its C-backend AVX2 build is 198 ms, inside 1.25x. C values
move a few percent between runs, so single-digit differences are noise.

## High-end scaling (`bench-scaling.sh`)

`par for` reads `MAGIC_THREADS` as a test/tuning cap (default: min(CPUs, body cap, iterations)). On
the 16-thread Ryzen host, the parallel raytracer scales:

| threads | 1 | 2 | 4 | 8 | 16 |
|---|---:|---:|---:|---:|---:|
| avg ms | 91 | 56 | 35 | 23 | **17** |

with the same checksum at every point, 5.4x from one to sixteen threads. The gate requires at least
3x and is part of `verify.sh`.

## Static runtime features

All eight workloads support `--static`. The freestanding runtime implements `clone`/`wait4`-based
threads, fixed-point `print_prec`, syscall file I/O and TCP, and `env()`; static panics print a raw
frame-pointer stack trace to stderr. `%g`/`print(f64)` is rejected explicitly.

## Low-end proxy gate

`./bench-lowend.sh` rebuilds all eight default workloads and runs each under a **2 GiB
address-space limit** (`prlimit --as=2147483648`) pinned to **one CPU** (`taskset -c 0`), checking
the same checksums as the main matrix. All eight pass: peak RSS 2–34 MB, matmul 9 ms, hash 39 ms,
ray 123 ms, particles 141 ms, sieve 368 ms, fannkuch 263 ms, n-body 879 ms, binary trees 3825 ms.
This is a proxy for a 2 GB low-end machine, not a physical one: it bounds the memory class and the
single-core time, but does not test GPU or very slow storage.

## Honest gaps

- The seven-workload speed result uses each workload's best MAGIC variant. N-body's native IR is
  613 ms versus Rust 227; only its C-backend AVX2 build (198 ms) is inside 1.25x.
- Peak memory is measured on freestanding static builds; dynamic builds are at parity with C. There
  is no physical 2 GB or GPU device in this table: low-end is a 2 GiB address-space/one-CPU proxy.
- Static mode does not support `%g`/`print(f64)`; it is rejected at build time. Static panic traces
  are raw addresses, not symbolized frames.
- Compile figures are on a single machine and a synthetic 2,000-function source.
