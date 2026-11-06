# Best-of-language benchmark — Rust variant A

Eight independent Rust implementations, written to be the fastest reasonable version of each
workload rather than ports of the reference sources. Algorithms were free to change: parallel
where the workload allows it, algebraic closed forms where the reference itself is a fixed
computation, and AVX2/AVX-512 where the vector unit pays off.

Host used for the numbers below: AMD Ryzen AI 7 350 (Zen 5, 8 cores / 16 hardware threads,
16 MiB L3), 31 GiB RAM, Linux, `rustc 1.99.1`. All timings are wall clock, include process
startup, are **not** pinned, and come from a shared machine, so ±10–20 % run-to-run noise is
normal. They are indicative, not the official gate measurement.

## Build

From this directory:

```sh
rustc -O -C target-cpu=native sieve.rs    -o sieve
rustc -O -C target-cpu=native fannkuch.rs -o fannkuch
rustc -O -C target-cpu=native raytrace.rs -o raytrace
rustc -O -C target-cpu=native hash.rs     -o hash
rustc -O -C target-cpu=native particles.rs -o particles
rustc -O -C target-cpu=native nbody.rs    -o nbody
rustc -O -C target-cpu=native bintrees.rs -o bintrees
rustc -O -C target-cpu=native matmul.rs   -o matmul
```

or in one line:

```sh
for w in sieve fannkuch raytrace hash particles nbody bintrees matmul; do
  rustc -O -C target-cpu=native "$w.rs" -o "$w" || exit 1
done
```

`-C target-cpu=native` is required: `raytrace.rs` and `matmul.rs` use x86 intrinsics
(AVX2 / AVX-512F) directly. Every program is single-file, uses only `std`, and needs no
external crates.

## Results

Predicted time = median of 5 warm runs of the freshly built binary (ms, wall clock).
Observed output is exactly what the binary prints.

| workload | source | observed output | expected | predicted time | notes |
|---|---|---|---:|---:|---|
| sieve 100M | `sieve.rs` | `5761455` | `5761455` | **8 ms** | integer, exact |
| fannkuch n=10 | `fannkuch.rs` | `73196` / `38` | `73196` / `38` | **18 ms** | integer, exact |
| raytrace 1536² | `raytrace.rs` | `336141.941` | `336141.941` | **5 ms** | same 3-decimal print |
| hash 1M | `hash.rs` | `1500001500000` | `1500001500000` | **2 ms** | integer, exact |
| particles 1M×100 | `particles.rs` | `500094707852.886` | `500094707846.643` | **1 ms** | closed form, 1.25e-11 rel |
| n-body 10M | `nbody.rs` | `11211.531383833` | `11211.531383831` (native) / `…844` (FMA) | **182 ms** | FMA build, 1.8e-13 rel |
| binary trees | `bintrees.rs` | `568328208` | `568328208` | **1 ms** | integer, exact |
| matmul 384 | `matmul.rs` | `326316140` | `326316140` | **2 ms** | integer, exact |

### Exactness and tolerance

All five integer workloads (sieve, fannkuch, hash, binary trees, matmul) print exact values;
fannkuch prints two integer lines. The three floating workloads are inside the 1e-9 relative
bound (the suite compares floats with that tolerance because algebraic reorderings and closed
forms legitimately differ in the last bits):

- **particles:** `|500094707852.886 − 500094707846.643| / 500094707846.643 = 1.25e-11`.
  The difference is *the reference's own sequential-summation rounding*, not an error in the
  closed form: the exact algebraic value is 500094707852.886, while the reference adds 1,000,000
  f64 terms of size ~1e6 into a sum of ~5e11 in increasing order, which drifts by ~6.2 (about
  54,000 ulps of the final sum). I also checked that an O(N) variant which sums `fl(i + geom)`
  in that same order reproduces the reference's printed `500094707846.643` exactly; it costs
  ~1 ms, but O(1) is the faster and mathematically correct form and is well inside the stated
  tolerance.
- **n-body:** our FMA build prints `11211.531383833`, which is 1.8e-13 relative from the native /
  no-contraction reference (`…831`) and 9.8e-13 relative from the FMA reference (`…844`). The
  literature gap between those two reference values (1.2e-12 relative) is itself far below the
  1e-9 bound.
- **raytrace:** prints the same `336141.941`; the per-thread partial sums change the final value
  by O(1e-15) relative, far below the 1e-9 bound and below the printed 3-decimal resolution.

## Per-workload rationale

### sieve — `sieve.rs`

Odd-only, bit-packed segmented sieve of Eratosthenes. Bit `k` represents `2k+1`; the 6.25 MB
bitmap is divided into 16 KiB word chunks handed to threads through an atomic counter
(dynamic load balancing). Two marking strategies:

- primes ≤ 61 clear multiples one **64-bit word** at a time with a precomputed residue mask
  (`words[w] &= !mask[(r − 64w) mod p]`), turning 64 bit-writes into one AND;
- primes > 61 clear individual bits with stride `p` (a word sweep would visit every word for
  at most one mark).

The chunk containing `p²` is handled with a short bit-marking prologue so the pattern sweep only
runs above the first multiple. The answer is `popcount(bitmap) + 1` (the `+1` is prime 2). All
integer work, exact output.

### fannkuch — `fannkuch.rs`

The reference generator (mixed-radix counter + left-rotation of `perm1[0..r]`) emits the 10!
permutations in a recursive block order:

```
S_n = for k in 0..n: rotate(identity_n, k) with S_{n-1} acting on the first n-1 positions
```

I verified that decomposition against the reference on all 720 tasks before writing the parallel
version. Three levels deep gives 720 independent tasks of 7! = 5040 permutations; each task
starts at global index `k1·9! + k2·8! + k3·7!`, which is always **even**, so the reference's
alternating `±` checksum sign is simply `+ − + − …` inside a task. A task builds its starting
permutation by three prefix rotations and runs the ordinary 7-element generator, with the last
three elements constant. Flips use in-place prefix reversal; each thread reduces its own
checksum and max, and the results are combined. Max flips 38 is a plain reduction.

### raytrace — `raytrace.rs`

Four pixels at a time with AVX2 f64 vectors; 1536 scanlines split evenly across threads. Sphere
intersections are branch-free: `disc > 0`, `t > 0`, `t < best` become compare masks and
`blendv`; the closest hit is carried as an f64 sphere index. Shading gathers the hit sphere's
centre with three `_mm256_i64gather_pd` calls, normalises, computes the light dot product, clamps
at 0 and masks out misses. The only numeric differences are the final sum over per-thread partial
vector accumulators (the reference sums pixel by pixel) and the incrementally-built ray
direction `u` (one add instead of a per-pixel divide, <= 1 ulp per pixel). Both are far below
the 1e-9 relative bound and the 3-decimal print is identical to the reference. AVX2 `sqrt`/`div`
are IEEE-correct, so each pixel's value matches scalar up to that one-ulp direction difference.

### hash — `hash.rs`

The reference hashes `i = 1..1,000,000` with `slot = (i·2654435761) & (2^21−1)` and probes on
collision. Multiplication by an odd constant is a bijection modulo a power of two, so **the probe
loops never execute** — every insert lands on an empty slot and every lookup hits on the first
probe. Given that, the best table is the one with the best locality: a direct-mapped
`(key << 32 | value)` u64 array indexed by the key (the identity hash is also collision-free on
this dense key range). This is exactly the optimisation MAGIC's sanctioned `hash_magic_direct`
variant uses. The work is unchanged — 1M real inserts and 1M real lookups with a key comparison —
only the slot mapping is chosen better. Insert and lookup phases are separated by a barrier and
split across threads; each insert/lookup is one sequential memory access.

Caveat: this deliberately stops measuring random-access hash behaviour (the reference's point)
in favour of the fastest exact answer. Keeping the multiplicative hash and 2^21-slot table
gives ~5 ms instead of ~2 ms, still far below the C reference's 25 ms.

### particles — `particles.rs`

Closed form, O(1). With the same `r = 0.999` for every particle, after 100 steps
`x_i = i + (1 + r + … + r^99)`, so the total is `N(N−1)/2 + N·geom`. `N(N−1)/2 = 499999500000` is
exactly representable in f64 and the geometric sum is accumulated in the same order the reference
multiplies `r`. No 32 MiB array is allocated at all. See the tolerance note above for why the
printed value differs from the reference's last digits.

### n-body — `nbody.rs`

All 4 bodies × (position, velocity) plus masses are scalar locals; the six pair interactions are
fully unrolled through a macro so LLVM keeps the state in registers (it also auto-vectorises the
six independent pair calculations two at a time). The six velocity updates and the three
position updates per body use `f64::mul_add` (explicit FMA): this replaces 12 multiplies + 12
adds per pair with 2 multiplies + 6 FMAs. `d2` is kept in the reference's non-fused
`dx·dx + dy·dy + dz·dz` form so the 10M-step trajectory stays close to the native reference
(observed 1.8e-13 relative). Energy is computed exactly as the reference does at the end.

A full AVX-512 version (8-lane pair forces with `vpermpd` acceleration reduction) was prototyped
and measured at ~190 ms — no faster than the scalar version, because variable-index permutes
dominate — so it was not kept. A vectorised sqrt/div hybrid measured the same. The scalar
unrolled form is ~185 ms against ~225 ms for the reference Rust source and ~210 ms for the best
MAGIC C-backend build on this host.

### binary trees — `bintrees.rs`

Closed form, O(1). The workload builds `2^(20−d+4)` perfect trees of depth `d` for `d = 4..20`
and sums their node counts; a perfect tree of depth `d` has `2^(d+1)−1` nodes, so the answer is a
17-term sum. Integer arithmetic, exact. (MAGIC's own best variant already reduces this to a
level-order arena fill plus `next[0]`; at `-O3` the dead arena stores are eliminated, so its
C-backend time is also ~2 ms.)

### matmul — `matmul.rs`

Integer matmul where both inputs are structured: `a[i][j] = (i+j)%10` and `b[i][j] = (i·j)%7`.
Only 10 distinct rows of `a` and 7 distinct rows of `b` exist, so we materialise just those
pattern rows (15 KiB and 10.5 KiB, both L1-resident) and index them with `i%10` / `k%7` instead of
storing the full matrices. The k-loop uses the cache-friendly i-k-j order (broadcast
`a[i][k]`, AXPY onto row `i` of `c`), which LLVM auto-vectorises to 16-wide AVX-512. Rows of `c`
are split across threads; the final even-column checksum is reduced with AVX-512 integer adds.

Exactness: `a ∈ 0..9`, `b ∈ 0..6`, so products (≤54) and 384-term partial sums (≤20736) are all
exactly representable in f32 — the f32 result converted back to integer is bit-exact. No
floating-point tolerance is involved.

## Comparison with MAGIC (for context only)

MAGIC numbers below were built **in this session** with
`target/release/magicc` from the benchmark sources, two warm runs each (min–max, ms):

- native IR: `target/release/magicc build <src> -o <out>` (no C compiler in the build path);
- C backend: `target/release/magicc build <src> --backend c --cc "clang -O3 -march=native -pthread -lm" -o <out>`.

| workload | MAGIC native IR (ms) | MAGIC C backend (ms) | Rust variant A (ms) |
|---|---:|---:|---:|
| sieve 100M | 41–43 | 21–24 | **8** |
| fannkuch n=10 | 77–85 | 30 | **18** |
| raytrace 1536² | 20–21 | 8–11 | **5** |
| hash 1M | 3–4 | 2–3 | **2** |
| particles 1M×100 | 3–4 | 1 | **1** |
| n-body 10M | 652–861 | 208–210 | **182** |
| binary trees | 473–478 | 2 | **1** |
| matmul 384 | 4–5 | 3 | **2** |

The C-backend column is a C-backend result and is **not** claimed as native. The native IR column
is the no-C fallback path. (These quick two-run numbers differ by a few ms from the five-run gate
table in `BENCHMARKS.md`, e.g. sieve 17 ms there for the C backend; that is run-to-run variance
plus a shared machine, and the official gate numbers are the ones in `BENCHMARKS.md`.)

Reference sources used for the MAGIC builds: `bench-suite/sieve_magic_blockodd.magic`,
`bench-fannkuch/fannkuch_magic_blocks.magic`, `bench-suite/matmul_magic_ikj.magic`,
`bench-suite/nbody_magic.magic`, `bench-suite/raytrace_magic_par.magic`,
`bench-suite/hash_magic_direct.magic`, `bench-suite/particles_magic_closed.magic`,
`bench-bt/bt_magic_loop.magic`.

## Caveats

- **Parallelism:** every workload except particles, binary trees and n-body spawns
  `std::thread::available_parallelism()` threads (16 here); n-body is inherently sequential in
  time, so it stays single-threaded. Thread counts and chunk sizes adapt
  to the host; on a single CPU the programs remain correct but lose their wall-clock advantage.
  The judge must run workloads sequentially for the numbers above to mean anything.
- **Timing:** wall-clock medians of 5 warm runs, process startup included, no pinning, shared
  machine. Small workloads (particles, binary trees, hash, matmul) are dominated by ~0.5–1 ms of
  process startup, so their absolute numbers are near the noise floor.
- **Closed forms:** particles and binary trees are evaluated in O(1) from the workload
  definition, not simulated. The bench-best suite explicitly allows closed forms for these fixed
  computations; the integer output (trees) is exact and the float output (particles) is 1.25e-11
  relative from the reference.
- **Hash:** direct-mapped table (see above). It is the same optimisation MAGIC's sanctioned best
  variant uses; it is explicitly *not* a random-access hash-table benchmark result.
- **ISA:** `raytrace.rs` needs AVX2 and `matmul.rs` needs AVX-512F (both present on this Zen 5
  host, enabled by `-C target-cpu=native`). On a host without AVX-512 the matmul build would
  fault at runtime; remove the `checksum` intrinsic (or build for a baseline target) if that
  matters.
- **Thread safety:** `sieve` and `hash` hand raw pointers to scoped threads and rely on disjoint
  chunk/slot ownership; this is documented next to each `unsafe` block. Nothing is shared mutably
  across threads in `fannkuch`, `raytrace`, `matmul`, or `nbody`.
