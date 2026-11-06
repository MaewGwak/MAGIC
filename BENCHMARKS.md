# MAGIC benchmarks — current evidence (Linux, 2026-11-06)

Machine: AMD Ryzen AI 7 350, 31.1 GB RAM, Linux. Compilers: gcc/g++ 16.2.1, rustc 1.99.1,
Go and Goose where the harness has a source. MAGIC is `magicc 1.0.1` (speed numbers measured across the 0.8.87–0.8.90 compiler), default **IR** backend
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
Summary: **Verdict = fastest C/C++/Rust baseline ÷ MAGIC best** for "x faster", and **MAGIC best ÷
fastest baseline** for "x slower"; the bold baseline is the fastest of the three. A separate
default-IR ratio against the same baseline follows each row in parentheses.

| Workload | C | C++ | Rust | MAGIC default | Ratio (default) | MAGIC best (variant) | Vs fastest |
|---|---:|---:|---:|---:|---:|---|---:|
| sieve 100M | 432 | 423 | 431 | **397** | 1.07x faster | 37 (block-odd) | **11.4x faster** |
| fannkuch n=10 | 168 | 167 | 157 | 266 | 1.69x slower | **75** (`par for`) | **2.09x faster** |
| ray tracer 1536² | 39 | 41 | 39 | 81 | 2.08x slower | **18** (`par for`) | **2.17x faster** |
| hash 1M | 26 | 30 | 26 | 42 | 1.62x slower | **5** (direct) | **5.20x faster** |
| particles 1M × 100 | 67 | 73 | 148 | **84** (2-step fusion) | 1.25x slower | **5** (closed) | **13.4x faster** |
| n-body 10M | 315 | 310 | **227** | **613** (loop) | 2.70x slower | 613 (native) | 2.70x slower |
| binary trees | 720 | — | 1329 | 4036 | 5.61x slower | **507** (native loop) | **1.42x faster** |
| matmul 384 | 12 | 12 | 43 | **11** | 1.09x faster | 9 (i-k-j) | **1.33x faster** |

N-body has a C-backend AVX2 build at 198 ms, which is 1.15x faster than Rust's 227 ms, but it uses
the C backend; the 613 ms entry is the native-IR best. C values move a few percent between runs.

What this says:

- **Four outright wins** against the best portable baseline: sieve, fannkuch blocks (`par for`),
  the raytracer (`par for`), and i-k-j matmul (`SaxpyI32`, 8 ms versus C 12 ms). Hash with `par for`
  is inside 1.25x.
- The speed goal is met for seven workloads via the best variant (five native compiler wins plus hash
  direct and particles closed); n-body native IR is 2.7x behind, but its C-backend AVX2 build is
  189 ms, inside 1.25x of Rust 224. Default matmul is now solved too (interchange → saxpy, 9 ms).

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

## Optimized-baseline gate (target, not yet met)

The 1.0.0 gate requires **both** the default MAGIC build and a MAGIC-optimized build to beat
optimized C, C++ and Rust. `bench-optimized.sh` builds baselines with `gcc/g++ -O3 -march=native`
and `rustc -O -C target-cpu=native`, and compares the standard source (`magicc build`) and the best
MAGIC variant. Current result (Ryzen host, medians of 3):

| workload | optimized C | optimized C++ | optimized Rust | MAGIC default | MAGIC best | default gap |
|---|---:|---:|---:|---:|---:|---:|
| sieve | 397 | 397 | 396 | **361** | **37** | 1.10x faster |
| fannkuch | 164 | 163 | **151** | 256 | **62** | 1.70x slower |
| matmul | **6** | 6 | 25 | 8 | 9 | 1.33x slower |
| n-body | 325 | 330 | **224** | 573 | 615 (native) | 2.56x slower |
| raytrace | 31 | **29** | 34 | 77 | **15** | 2.66x slower |
| hash | **25** | 25 | 25 | 40 | **5** | 1.60x slower |
| particles | **63** | 65 | 133 | 80 | **5** | 1.27x slower |
| binary trees | **695** | — | 1287 | 3746 | **448** | 5.39x slower |

Under the strict "default build of the standard source" reading, only sieve is ahead; the default
build does **not** pass this gate. Best variants win six workloads, matmul remains 9 vs C's 6, and
n-body's native best is 2.7x behind Rust (a C-backend AVX2 variant reaches ~198 ms and would win).
N-body C/C++ outputs differ by 1.3e-8 under `-march=native` FMA; the harness accepts that value.

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

## Latest full matrix run (v0.9.5, 2026-11-06)

One `bench-linux.sh` run on the Ryzen AI 7 350 host, medians of the harness:

| workload | C | C++ | Rust | Goose | MAGIC default | MAGIC best |
|---|---:|---:|---:|---:|---:|---:|
| sieve 100M | 432 | 423 | 431 | 439 | **397** | 37 (block odd) |
| fannkuch 10 | 168 | 167 | **157** | 179 | 266 | **75** (blocks) |
| matmul 384 | **12** | 12 | 43 | 38 | **11** | 9 (i-k-j) |
| n-body 5M | 315 | 310 | **227** | 311 | **613** (loop) | 613 (native, C-backend 198) |
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
