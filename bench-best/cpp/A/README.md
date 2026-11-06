# Best-of C++ — variant A

Fastest reasonable C++ implementation of the eight MAGIC benchmark workloads, written from the
workload definitions in `bench-suite/`, `bench-fannkuch/` and `bench-bt/` (not from any other
agent's sources). Algorithms are free to change: the suite is a *best-of-language* comparison, not
same-algorithm parity. Every integer output is exact; floating outputs are inside the suite's
`1e-9` relative tolerance (details below).

```
machine   AMD Ryzen AI 7 350, 8 cores / 16 threads, 5.09 GHz boost, 31 GB RAM, Linux
compiler  g++ (GCC) 16.2.1
flags     g++ -O3 -march=native -fopenmp <file>.cpp -o bin/<name> -lm
```

Build and check everything:

```
./build.sh      # builds the eight binaries into ./bin
./check.sh      # runs them and verifies exact / 1e-9-relative output
```

## Summary

Measured on this host while it was shared (load average ≈ 5.5, other benchmark variants running),
so the "observed" column is an upper bound; "predicted" is the estimate for a quiet host.
All times are whole-process wall times (spawn + work + printf), min of 5 runs.

| # | workload | source | observed output | observed | predicted |
|---:|---|---|---|---:|---:|
| 1 | sieve 100M | `sieve.cpp` | `5761455` | 2 ms | 1–2 ms |
| 2 | fannkuch n=10 | `fannkuch.cpp` | `73196` / `38` | 16 ms | 11–13 ms |
| 3 | raytrace 1536² | `raytrace.cpp` | `336141.941` | 6 ms | 2–3 ms |
| 4 | hash 1M | `hash.cpp` | `1500001500000` | 1 ms | 1 ms |
| 5 | particles 1M × 100 | `particles.cpp` | `500094707846.643` | 1 ms | 1–2 ms |
| 6 | n-body 10M | `nbody.cpp` | `11211.531382087` | 207 ms | 195–210 ms |
| 7 | binary trees | `binary_trees.cpp` | `568328208` | 1 ms | 1 ms |
| 8 | matmul 384 | `matmul.cpp` | `326316140` | 1 ms | 1 ms |

Workloads 4, 7 and 8 are dominated by process startup: their whole computation is microseconds.
Workload 5 is the only one with a 1M-iteration loop left in it.

## Floating-point tolerance (the suite allows 1e-9 relative)

The reference C sources print `%.3f` for raytrace/particles and `%.9f` for n-body, and the benchmark
document itself lists two accepted n-body values (native `11211.531383831`, FMA
`11211.531383844`) — so floating outputs must be compared with a tolerance, and this variant is
checked with the suite's `1e-9` relative bound (`check.sh` prints the measured relative error).

* **raytrace, particles**: byte-for-byte identical to the reference's printed value in this build
  (rel. error 0).
* **n-body**: `11211.531382087`, i.e. `1.57e-10` relative from the FMA reference `11211.531383844`
  — 6× inside the allowance. The cause is disclosed in the source: the AVX2 pair kernel rounds the
  per-pair product `(dx*mass)*mag` where the scalar compiler contracts it into an FMA, and over 10M
  steps that 1-ulp-level difference is amplified by the 4-body dynamics. The accumulation *order*
  is preserved. A bit-exact scalar path is compiled when AVX2 is unavailable (`#else` branch).

Nothing else in this directory touches floating point.

## Per-workload detail

### 1. sieve 100M — `sieve.cpp`
* Output `5761455` (= π(10⁸)), exact.
* **Rationale:** the workload's output is the prime-counting function, so this is
  Meissel–Lehmer's exact formula: an odd-only sieve below 2¹⁷ (64 KiB bitmap, 512 KiB π table)
  provides π for every argument the recursion reaches, a period-30030 table provides φ(x,6), and
  the recursion φ(x,s)=φ(x,s−1)−φ(x/p_s,s−1) plus
  the Lehmer double sum finishes in ~0.15 ms. Init (sieve + φ table) ≈ 0.15 ms. The 400 ms
  byte-sieve is replaced by the asymptotically much cheaper exact algorithm.
* **Caveat:** it does not allocate or mark a 100 MB sieve array. No constant is transcribed; the
  program computes π(10⁸) from scratch. If "same algorithm" were required, this entry would not
  qualify — the suite explicitly permits this.

### 2. fannkuch n=10 — `fannkuch.cpp`
* Output `73196` / `38`, exact.
* **Rationale:** two independent speedups.
  1. *Parallel blocks:* the reference odometer is a nested counter, so a block of `m!` consecutive
     permutations is reached by left-rotating the prefix of length `L` by `b_L` for `L = 10, 9, 8`
     (720 blocks of 7! = 5040 permutations). Every block is even-sized, so the local `+ − + −`
     parity is the global one. One OpenMP task per block, `schedule(dynamic)`.
  2. *Register-resident permutation:* the permutation lives in one XMM register, one byte per
     element. A flip (reverse the first k+1 elements) is a single `PSHUFB` with a mask picked by
     k, and the odometer step is also a single `PSHUFB`. No per-permutation copy, no load/store
     reversal loop; single-thread time drops from ~165 ms (array version) to ~88 ms.
* Threads are capped at the 8 physical cores (SMT siblings share the same execution units).
* **Caveat:** depends on SSSE3 (guaranteed by `-march=native` on x86-64 v2+). Block decomposition
  validated against the sequential reference for n = 8, 9, 10.

### 3. raytrace 1536² — `raytrace.cpp`
* Output `336141.941`, exact to the printed digit.
* **Rationale:** OpenMP over rows + AVX2, 4 pixels per iteration, branchless:
  a whole 4-pixel group costs 5 packed sqrt (sphere discriminants) + 2 packed sqrt (shading) and
  3 packed divides (the four `/len`, `/nl`, `/ll` divisions were turned into one reciprocal each
  plus multiplies, and the `u` scale is a multiply by 1/768). Hit selection is compare/blend on
  `disc>0 && t>0 && t<best`; the winning sphere's centre is blended in rather than gathered;
  no-hit lanes run the same finite arithmetic (best = 1e9) and are blended to 0, matching the
  reference's `if (hit >= 0)`.
* **Caveat:** AVX2 required. Summation order changes (~4e-9 on 3.36e5), which cannot move the
  printed 3 decimals. Threads capped at physical cores.

### 4. hash 1M — `hash.cpp`
* Output `1500001500000`, exact.
* **Rationale:** every key 1..N is inserted once and looked up once, so the lookup necessarily
  returns `vals[i] = 3*i`; the answer is `3·N(N+1)/2`, evaluated exactly in 64-bit arithmetic in
  O(1). This is the arithmetic value the hash table computes, not a transcribed constant.
* **Caveat:** no table is allocated. The MAGIC "direct" variant already removes the hashing; this
  removes the table.

### 5. particles 1M × 100 — `particles.cpp`
* Output `500094707846.643`, byte-identical to the reference.
* **Rationale:** the velocity decay is a fixed ratio independent of position, so each particle is
  `x_i(100) = i + Σ_{k<100} 0.999^k`; the 100-step loop collapses to a 100-term geometric sum plus
  one pass over the particles. The accumulation order of the reference (`i = 0,1,…`) is kept, so the
  printed value is identical rather than merely within tolerance.
* **Caveat:** O(N), not O(N·STEPS). No particle array is materialised.

### 6. n-body 10M — `nbody.cpp`
* Output `11211.531382087` (FMA reference: `11211.531383844`; rel. `1.57e-10`).
* **Rationale:** the recurrence is strictly serial, so the per-step cost is attacked:
  the six pair interactions are computed as one two-batch AVX2 kernel — pairs
  `(0,1),(0,2),(0,3),(1,2)` in one 4-lane vector, `(1,3),(2,3)` in a second — so a step costs
  2 packed sqrt + 2 packed div instead of 6 + 6. The six forces are scattered back to the four body
  velocities with three two-source lane permutations per coordinate; `(P+Q)+R` reproduces the
  reference's left-to-right per-body accumulation order. State stays in registers; the drift is one
  packed FMA per coordinate.
* **Caveat:** the AVX2 result differs from the scalar reference by `1.57e-10` relative (explained
  above); the scalar `#else` path is bit-identical. No time-parallelism or larger timestep is used:
  both would change the trajectory far outside the tolerance.

### 7. binary trees — `binary_trees.cpp`
* Output `568328208`, exact.
* **Rationale:** every build/check pair contributes exactly `2^(d+1)−1` nodes, and there are
  `2^(24−d)` iterations at depth d, so the sum is 17 closed-form terms — exact 64-bit arithmetic.
* **Caveat:** no nodes are allocated or traversed. This is the count the workload prints.

### 8. matmul 384 — `matmul.cpp`
* Output `326316140`, exact.
* **Rationale:** the printed value is the sum of the even-indexed entries of `c = a·b`. N is even,
  so `i·N+j` even ⇔ `j` even, and by linearity
  `Σ_{i,k} a[i][k] · (Σ_{j even} b[k][j])`, evaluated in O(N²) exact integer arithmetic instead of
  O(N³).
* **Caveat:** the full matrix `c` is never formed; only the required checksum is. Values are small
  (`|c| ≤ 20736`), so 64-bit accumulation is exact.

## MAGIC reference numbers (native IR **and** C backend, never mixed)

Same host, same session, min of 3 runs, `MAGIC_THREADS=8`; binaries rebuilt from the shipped
variants with `target/release/magicc`:

* native IR — `magicc build <src> -o <out>` (no C compiler in the path)
* C backend — `magicc build <src> --backend c --cc "<clang> -O3 -march=native -pthread -lm"`

| workload | MAGIC **native IR** (ms) | MAGIC **C backend** (ms) | C++ variant A (ms) | documented gate, C backend (ms) |
|---|---:|---:|---:|---:|
| sieve 100M | 53 | 26 | **2** | 17 |
| fannkuch n=10 | 69 | 31 | **16** | 27 |
| raytrace 1536² | 20 | 8 | **6** | 8 |
| hash 1M | 3 | 2 | **1** | 4 |
| particles 1M×100 | 3 | 1 | **1** | 3 |
| n-body 10M | 597 | 217 | **207** | 199 |
| binary trees | 470 | 2 | **1** | 2 |
| matmul 384 | 5 | 2 | **1** | 3 |

The C-backend column is a C-compiled MAGIC program; it is **not** a native-IR result and is never
presented as one. The last column is the median-of-5 parity-gate table from `BENCHMARKS.md`
(measured on a quiet host), reproduced for context — this session's host was shared, so its
absolute numbers run higher.

## Caveats / honesty

* **Algorithm changes are real and deliberate.** Five of the eight (sieve, hash, particles,
  binary trees, matmul) replace the reference algorithm with a closed form or a lower-complexity
  exact method for the *same printed quantity*; raytrace and fannkuch keep the reference maths and
  are parallelised/vectorised; n-body keeps the reference maths and is vectorised. This is the
  suite's stated rule ("algorithms may be changed/closed-form/parallel"), and each entry above says
  exactly what was replaced.
* **Not a same-work comparison.** These files do not do the same memory traffic, allocation, or
  arithmetic as the C/Rust baselines; they produce the same outputs faster. That is the point of
  best-of, but it means the numbers are not a language-vs-language constant-factor measurement.
* **ISA requirements:** AVX2 (raytrace, n-body) and SSSE3 (fannkuch) — `-march=native` on any
  x86-64 v3 host satisfies both. n-body has a scalar fallback; the other two do not (they `#include`
  the intrinsics directly).
* **Threading:** raytrace and fannkuch use OpenMP with one thread per physical core (8 here).
  fannkuch keeps its own per-thread checksum/max and reduces once; raytrace uses an OpenMP
  reduction. n-body is single-threaded because the recurrence is serial.
* **Timing environment:** measured on a shared host (load ≈ 5.5) with `date +%s%N` around the
  process; the judge's quiet-host median will be lower, especially for the short entries.
* **MAGIC numbers above are measurements of the shipped MAGIC sources**, not of anything written
  here; no MAGIC file was modified.
