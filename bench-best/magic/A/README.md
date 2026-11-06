# MAGIC best-of — variant A

Eight independent MAGIC implementations written for speed, not algorithm parity. The task
allows changed algorithms, closed forms and parallelism; each choice is called out below.
All sources are original for this variant.

Host used for the spot checks: AMD Ryzen AI 7 350 (8 cores / 16 threads), Linux, gcc 16.2.1,
clang 22 (`/home/krittapas/tools/clangroot/usr/bin/clang`), `magicc 1.0.0`
(`/home/krittapas/Projects/magic/target/release/magicc`).

## Build

Two build paths are reported separately. **Native IR** means the default `magicc build`
(IR backend, `as` + `ld`, no C compiler). **C backend** means `--backend c` with clang.

```sh
cd /home/krittapas/Projects/magic/bench-best/magic/A

MAGICC=/home/krittapas/Projects/magic/target/release/magicc
CLANG=/home/krittapas/tools/clangroot/usr/bin/clang
# clang itself and its OpenMP runtime both live under the clang root:
export LD_LIBRARY_PATH=/home/krittapas/tools/clangroot/usr/lib64

# fastest build (C backend, clang -O3 -march=native), one binary per workload
CCF="$CLANG -O3 -march=native -fno-math-errno -lm \
     -Wl,-rpath,/home/krittapas/tools/clangroot/usr/lib64"
for w in sieve fannkuch raytrace hash particles nbody bt matmul; do
  "$MAGICC" build "$w.magic" --backend c --cc "$CCF" -o "$w"_c
done

# native IR build (default backend, no C compiler in the path)
for w in sieve fannkuch raytrace hash particles nbody bt matmul; do
  "$MAGICC" build "$w.magic" -o "$w"_ir
done
```

`-Wl,-rpath,...` matters for `fannkuch` and `raytrace`: their C-backend `par for` loops become
OpenMP, and clang links `libomp.so` from the clang root, which is not on the default loader path.
The rpath makes the binaries runnable in a clean shell. The IR builds need no extra runtime (their
`par for` runtime uses libc pthreads).

`magicc` writes `<stem>.c/.o/.s/.flags` next to the source while building; they were removed here
and are recreated on demand.

If a harness rebuilds with clang but drops the rpath, the two OpenMP binaries (`fannkuch`,
`raytrace`) will need `LD_LIBRARY_PATH=/home/krittapas/tools/clangroot/usr/lib64` when run. As a
fallback that needs no loader configuration, build those two with the default gcc
(`--cc "gcc -O3 -march=native -lm"`) or use the native IR binaries; both produce the same outputs
(gcc is slower on fannkuch, ~70 ms vs ~16 ms, and on n-body, ~385 ms vs ~207 ms).

## Results

Outputs are from the final binaries run in a clean shell. Integer outputs are exact; floating
outputs are discussed under *Correctness and float tolerance*.

| workload | source | C backend output | native IR output | expected |
|---|---|---|---|---|
| sieve 100M | `sieve.magic` | `5761455` | `5761455` | 5761455 |
| fannkuch n=10 | `fannkuch.magic` | `73196`, `38` | `73196`, `38` | 73196, 38 |
| raytrace 1536² | `raytrace.magic` | `336141.941` | `336141.941` | 336141.941 |
| hash 1M | `hash.magic` | `1500001500000` | `1500001500000` | 1500001500000 |
| particles 1M×100 | `particles.magic` | `500094707852.886` | `500094707852.886` | 500094707846.643 (rel. 1e-9) |
| n-body 10M | `nbody.magic` | `11211.531383844` | `11211.531383831` | 11211.531383844 FMA / .831 native |
| binary trees | `bt.magic` | `568328208` | `568328208` | 568328208 |
| matmul 384 | `matmul.magic` | `326316140` | `326316140` | 326316140 |

### Predicted time

Medians of 3 warm runs, milliseconds, measured in one shared-host session (load average 6–11 from
other benchmark agents at the time, so these are conservative/noisy; earlier quiet-run spot checks
are noted where they were lower). Process startup is roughly 0.7–1.0 ms on this host and forms the
floor for the closed-form workloads.

| workload | C backend (ms) | native IR (ms) | quiet-run note |
|---|---:|---:|---|
| sieve 100M | 2.0–3.0 | 2.0–3.9 | C ≈ 2.0 ms, IR ≈ 2.6 ms |
| fannkuch n=10 | 16 | 43 | C 15–17 ms under load |
| raytrace 1536² | 4.9 | 9.5 | C ≈ 5.0 ms quiet |
| hash 1M | 0.9–1.2 | 0.9–1.2 | startup-bound |
| particles 1M×100 | 0.7–1.1 | 0.6–0.8 | startup-bound |
| n-body 10M | 207 | 703 | C ≈ 200–210 ms quiet |
| binary trees | 1.0–1.3 | 0.6–1.0 | startup-bound |
| matmul 384 | 0.8–1.2 | 1.2–1.8 | startup-bound |

The C backend is the fast path for the two compute-heavy kernels (n-body, fannkuch); the IR
backend is competitive for the short integer/closed-form programs and for raytrace within ~2×.
No C-backend number is reported as native IR anywhere in this README.

## Correctness and float tolerance

Integer outputs are exact on both backends. Floating outputs are compared within 1e-9 **relative**,
as the task allows, because algebraic reorderings legitimately differ in the last bits:

* **raytrace** — the reference prints `%.3f`; both our backends print `336141.941`, the same string.
  At 9 printed decimals our value is `336141.941102395` against the reference's
  `336141.941102400` (relative difference ≈ 1.5e-17). The difference comes from replacing the
  normalised-ray algebra with the equivalent unnormalised form (derivation in the source).
* **particles** — the reference integrates then sums sequentially and prints
  `500094707846.643`; our closed form prints `500094707852.886`. That is a relative difference of
  **1.25e-11**, well inside 1e-9. It is not a bug: the same closed form is what the shared
  best-of C reference (`bench-best/particles_best.c`) computes, and our output matches it exactly.
  The gap is the reference's own f64 sequential-summation error over 1e6 additions.
* **n-body** — C backend with FMA contraction prints `11211.531383844` (the expected FMA value);
  native IR prints `11211.531383831` (the expected native value). Their relative difference is
  ≈1.2e-12.

## Per-workload notes

### sieve 100M — `sieve.magic`
The workload's only output is π(100,000,000), so this is **not a marking sieve at all**: it is
Lucy_Hedgehog's O(n^(3/4)) combinatorial prime-counting recurrence. It keeps
`S(v) = π(v)` for the 2·√n distinct `v ∈ {n//i} ∪ {1..r}`, updates them in place over the primes
`p ≤ r = 10⁴` (descending traversal ⇒ the entries read are still the previous stage's), and prints
`S(n)`. n = 10⁸ means r = 10⁴, two 40 KB i32 arrays, and about 7·10⁵ inner steps — no 100 MB
bitmap and no 10⁸-element marking pass. Result exact.

### fannkuch n=10 — `fannkuch.magic`
720 independent streams (three rotations fix the last three entries, odometer over the first 7).
Each stream has 7! = 5040 permutations, an even count, so the local alternating checksum sign is
the global one and streams sum in any order. The inner flips run on ten scalar locals with the
ten possible flip lengths unrolled as an if-chain: no per-swap array stores or bounds checks.
~1.7× faster than the in-memory flip version in the same harness, and parallel across all cores.

### raytrace 1536² — `raytrace.magic`
Rows are independent (`par for`), and the algebra removes every normalisation the reference
performs. Using the unnormalised ray `D = (u,v,1.5)` and `A = |D|²`, the nearest-hit comparison
works on `s = B − sqrt(B² − A·cc)` (no per-pixel ray-length sqrt, no division); the shading cosine
is scale-invariant so the surface and light normalisations collapse into
`n' = sD − AC`, `l' = AL − sD`. Only `sqrt(disc)` and two shading `sqrt`s remain, and the fixed
scene makes every `B > 0`, so `disc > 0` implies a front hit. A row keeps its own sum.

### hash 1M — `hash.magic`
Keys are exactly 1..10⁶ with `value = 3·key`, and the hash table is a permutation of those pairs,
so the lookup sum is independent of hashing/probing: `3·n(n+1)/2`. Exact O(1) closed form.

### particles 1M × 100 — `particles.magic`
Constant velocity decay: `x_i = i + Σ_{s<100} 0.999^s`. The integration and the O(N) sum collapse to
`N(N−1)/2 + N·Σ r^s`; the 100-term geometric sum is accumulated with the same repeated multiply the
reference uses.

### n-body 10M — `nbody.magic`
The six pair interactions are written out over 24 f64 scalars (no arrays, no bounds checks, no
loops), with the reference's arithmetic order preserved (`dx*m[j]*mag`, then `vx[i] -= …`, then
`dt*vx` updates). This is what lets the C compiler keep state in registers and contract mul-adds;
native IR gets the same scalar form. The time-step chain is strictly serial, so there is no
parallelism to exploit. Output matches the expected FMA and native values respectively.

### binary trees — `bt.magic`
Every tree is perfect, so depth `d` contributes `2^(d+1) − 1` nodes per iteration and the workload
schedule has `2^(24−d)` iterations. The integer count is therefore a 17-term closed form; no arena
is allocated or walked. This is the same reduction the existing MAGIC optimized variant performs
(its arena writes are dead and removed by the compiler); we skip the dead writes explicitly.

### matmul 384 — `matmul.magic`
The program prints only the sum of `C[i][j]` over flat even indices. N = 384 is even, so that is
exactly "j even", and distributivity removes the N³ product:

```
total = Σ_i Σ_{j even} Σ_k A[i][k]·B[k][j] = Σ_k (Σ_i A[i][k]) · (Σ_{j even} B[k][j])
```

with `A[i][k] = (i+k)%10`, `B[k][j] = (k·j)%7`. That is ~295k exact i64 ops instead of 56.6M
multiply-adds; the C matrix is never materialised. The printed integer is identical because every
step is exact integer arithmetic.

## Caveats

* **Closed forms / changed algorithms are deliberate.** sieve is combinatorial prime counting;
  hash, particles, binary trees and matmul collapse to exact formulas (matmul only for the scalar
  the program prints). They do not perform the original loops, which the task explicitly permits.
  They are not comparable to the baselines as "same algorithm" — this is a best-of suite.
* **Parallelism.** `fannkuch` and `raytrace` use `par for`. With the C backend that lowers to
  OpenMP (`#pragma omp parallel for schedule(dynamic,1)`), thread count follows
  `OMP_NUM_THREADS` (default = logical CPUs). With IR it lowers to libc-thread chunks. Times above
  are with default thread counts on the 16-thread host.
* **Build flags.** The C-backend numbers use clang 22 with `-O3 -march=native -fno-math-errno
  -lm`; `-fno-math-errno` only lets `sqrt` inline, it does not change any result. `-march=native`
  means these binaries are host-specific.
* **Native IR vs C backend are separated everywhere.** The native IR path is the default
  `magicc build`; the C path is `--backend c`. They have different performance characteristics
  (IR is ~3.4× slower on n-body and ~2.7× on fannkuch; competitive elsewhere) and must not be conflated.
* **Timing noise.** The host was running other benchmark agents during sampling (load average
  6–11), so the predicted times are medians of only three runs and should be treated as rough.
  The neutral judge's sequential timing should be cleaner.
* **No long timing loops were run here.** The numbers come from short correctness runs and a
  three-run median per binary only.
