# Best-of-language benchmark — C variant A

Eight independent C implementations, written to be the fastest reasonable
version of each workload rather than a port of the naive baseline.  The suite
answers "what is the best C can do here?", not "same algorithm parity".

Everything in this directory was written for this submission.  No file was
copied from another agent's directory.

* Host: AMD Ryzen AI 7 350 (8 cores / 16 threads), 31 GB RAM, Linux.
* Compiler: gcc 16.2.1 (`gcc -O3 -march=native -fopenmp -lm`).
* This is a **C-only** submission.  No MAGIC source, `magicc` build, or
  `--backend c` artifact is part of it; the numbers below are gcc-built C
  binaries only.  Nothing here is presented as a MAGIC native-IR result.

## Build

One binary per source, all with the required flags:

```sh
gcc -O3 -march=native -fopenmp sieve.c     -o sieve     -lm
gcc -O3 -march=native -fopenmp fannkuch.c  -o fannkuch  -lm
gcc -O3 -march=native -fopenmp raytrace.c  -o raytrace  -lm
gcc -O3 -march=native -fopenmp hash.c      -o hash      -lm
gcc -O3 -march=native -fopenmp particles.c -o particles -lm
gcc -O3 -march=native -fopenmp nbody.c     -o nbody     -lm
gcc -O3 -march=native -fopenmp bintrees.c  -o bintrees  -lm
gcc -O3 -march=native -fopenmp matmul.c    -o matmul    -lm
```

`-fopenmp` is harmless for the three single-threaded sources.  The AVX2
sources (raytrace, nbody, matmul) also contain scalar fallbacks compiled when
`__AVX2__` is not defined; the fallbacks were checked to produce the same
outputs.

## Outputs and predicted times

Predicted time = median of 3–5 warm runs of the gcc-built binary on the host
above (`date +%s%N` around the process, so it includes process start-up,
which is ~0.5–1 ms).  It is a prediction for the neutral judge's sequential
timing, not a benchmark run: no long timing loops were executed.  The
reference C baselines listed for context are from `BENCHMARKS.md`'s
documented matrix (same host class).

| workload | source | observed output | expected | predicted | ref. C |
|---|---|---|---:|---:|---:|
| sieve 100M | `sieve.c` | `5761455` | 5761455 | **5–7 ms** | 403–432 ms |
| fannkuch n=10 | `fannkuch.c` | `73196` / `38` | 73196 / 38 | **15–18 ms** | 154–168 ms |
| ray tracer 1536² | `raytrace.c` | `336141.941` | 336141.941 | **3–4 ms** | 29–42 ms |
| hash table 1M | `hash.c` | `1500001500000` | 1500001500000 | **~1 ms** | 25–28 ms |
| particles 1M×100 | `particles.c` | `500094707852.886` | 500094707846.643 | **~1 ms** | 65–67 ms |
| n-body 10M | `nbody.c` | `11211.531383844` | 11211.531383831 (native) / …844 (FMA) | **185–200 ms** | 310–328 ms |
| binary trees | `bintrees.c` | `568328208` | 568328208 | **< 1 ms** | ~700 ms |
| matmul 384 | `matmul.c` | `326316140` | 326316140 | **1–2 ms** | 4–12 ms |

## Per-workload rationale

### sieve.c — segmented odd-only bit sieve, OpenMP
The real Eratosthenes sieve, not an analytic π(x) shortcut.  Odd numbers only
(one bit per odd slot, 50 M slots), split into 96 × 2¹⁹-slot segments
(≈2²⁰ numbers each) so each thread's 64 KiB bitmap stays in L2.  The small
primes (≤ 10000) are generated
once.  Two marking loops: for p < 64 a precomputed 64-bit pattern table
`masks[p][d]` is OR-ed into whole words at a time (one load/OR/store per 64
slots, phase advanced by `-64 mod p`); for p ≥ 64 a stride-p bit-set loop
(one mark per word).  Counting is POPCNT over the complement; the padding of
the last word is cleared so the count is exact.  Eight threads (SMT hurts:
16 threads measured slower than 8).

### fannkuch.c — exact parallel replay of the reference generator
The checksum uses the *global enumeration index* sign, so the order cannot be
replaced by an arbitrary permutation order and inversion parity alone was
verified to be wrong for n ≥ 5.  The reference rotation generator has a
closed-form state every 8! = 40320 emissions: for chunk t = 9b + a the state
is `perm1[i] = ((i+a) mod 9 + b) mod 10` (i < 9), `perm1[9] = (9+b) mod 10`,
`count[1..7] = 2..8`, `count[8] = 9-a`, `count[9] = 10-b`, `r = 1`.  This was
verified against a full serial run for all 90 boundaries, so the 90 chunks are
fully independent (no serial prefix pass).  Each task replays the exact
reference rotation sequence, adds ±flips with the true global index parity,
and the flip loop is a switch with fully unrolled reversals for k = 1..9.
Output is bit-identical to the reference.

### raytrace.c — exact formula + silhouette culling + AVX2 + OpenMP
The reference arithmetic is kept exactly (normalised ray, quadratic sphere
test, nearest hit, Lambert term).  Per row the *exact* u-interval in which a
sphere can be hit is derived from the silhouette cone (a quadratic in u), so
spheres that cannot touch a 4-pixel group are skipped entirely.  About 55% of
the screen lies outside every silhouette, contributes exactly 0, and is never
shaded.  Pixels are processed 4
at a time with AVX2 and rows are distributed over 8 threads.  The printed
value matches the reference to the printed digit.

### hash.c — closed form of the same computation
Keys 1..N are inserted once with value 3i and never deleted/rehashed, so every
lookup returns 3i.  The sum is `3·N(N+1)/2 = 1500001500000`, exact integer
arithmetic.  This is the same algebraic shortening as the shipped MAGIC
"direct" variant, taken one step further (the dense array is not needed).

### particles.c — closed form of the geometric integration
Per step `x += vx; vx *= 0.999`, so after 100 steps
`x_i = i + (1 + r + … + r^99)` and the total is `N(N-1)/2 + N·Σr^k`.  The
geometric sum is evaluated with the same 100-step recurrence.  The result
differs from the reference's sequential million-term sum by 6.2 absolute
(1.25e-11 relative — see the tolerance note below).

### nbody.c — same recurrence, four bodies in four SIMD lanes
The reference order is preserved: for source body j = 0..3 the code computes
the displacement and inverse-cube factor to all bodies at once and applies
`V[k] -= (X[k]-X[j])·m[j]·mag[k]`.  Body k therefore sees its contributions
in exactly the reference pair order (sources 0..k-1 added, then k+1..3
subtracted).  The AVX2 form replaces the 12 scalar square roots/divisions per
step with 4 vector square roots + 4 vector divisions and keeps all 24 state
doubles in registers (no spills).  The self lane is forced to d2 = 1 only to
keep `mag` finite; its displacement is exactly 0.  FMA contraction is enabled by `-march=native`,
which is why the printed value is the expected 11211.531383844 tail.

### bintrees.c — closed form of the same tree count
A perfect tree of depth d has 2^(d+1)-1 nodes, built 2^(24-d) times:
`Σ 2^(24-d)·(2^(d+1)-1) = 568328208`.  The loop evaluates that sum (nothing
hard-coded), which is the exact integer value of the reference workload.

### matmul.c — exact int16 SIMD dense product
A and B entries are 0..9 / 0..6 and every C entry is ≤ 20736, so 16-bit
products and 32-bit accumulators are exact.  B is packed into 32-bit words
`(b[2t][j], b[2t+1][j])`, the matching A pair is broadcast, and
`vpmaddwd(a_pair, b_pair)` accumulates both products into one int32 lane:
8 columns per vector, 32 columns per register block, 8 threads over rows.
The final checksum is computed exactly as in the reference.

## Floating-point tolerance

Integer outputs are exact.  Floating outputs are within the allowed
**1e-9 relative** tolerance:

* `raytrace.c` prints exactly `336141.941` (identical to the reference).
* `nbody.c` prints the FMA tail `11211.531383844`; the non-FMA scalar
  fallback prints `11211.531383831`.  The two reference tails themselves
  differ by 1.2e-12 relative, and both are inside 1e-9 of each other.
* `particles.c` prints `500094707852.886` vs the reference
  `500094707846.643`: 6.24 absolute = **1.25e-11 relative**, six orders of
  magnitude inside the gate.  The difference is only the summation order of
  the million-term total (the reference adds each particle's x sequentially;
  the closed form adds the exact `Σi` and the exact `N·S`), not a different
  mathematical result.

## Caveats

* **Closed forms are deliberate and are the same class of shortcut the
  project already ships** (MAGIC's `particles_magic_closed` and
  `hash_magic_direct`).  Under the benchmark's stated rule ("algorithms may
  be changed/closed-form/parallel") the hash, particles and binary-trees
  sources reduce the workload algebraically; the sieve, raytracer, fannkuch,
  n-body and matmul sources still perform their computation.
* The sieve is a genuine Eratosthenes sieve.  A Lehmer/Meissel prime-counting
  routine could compute π(10⁸) in ~1 ms, but that is a different algorithm
  for the same number and was deliberately not used.
* Thread counts are chosen for this host (sieve 8, fannkuch 12, raytrace 8,
  matmul 8); 16 threads measured *slower* for these kernels because of SMT
  contention and all-core clock behaviour.  The sources still build and run
  on fewer cores (OpenMP simply uses fewer threads).
* Timings above are short single-process measurements on a shared host; the
  neutral judge's sequential run is authoritative.
* `particles` and `nbody` are sensitive to floating-point association by
  design of the workload; the reported tails are the accepted reference
  tails (the tolerance is what makes this legal).
* No MAGIC result is claimed, native or C-backend.  If MAGIC comparisons are
  wanted, they belong in `BENCHMARKS.md`/the parity gate, which explicitly
  distinguishes native IR from `--backend c`.
