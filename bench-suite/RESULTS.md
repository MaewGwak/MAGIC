# MAGIC benchmark suite v1 — sieve + fannkuch

Two workloads, six languages, same machine, same output checks. The point is not to declare a
winner; it is to find out whether a language can be faster than its backend.

## Machine

AMD Ryzen AI 7 350, 31.1 GB RAM, Windows 11 (10.0.26200).
GCC 16.2 (MinGW-W64 UCRT) · rustc 1.99.1 (gnu) · go 1.x · goose 335fddc · magicc 0.1.0.

## Method

- **15 interleaved rounds**: every language runs once per round, so thermal drift and background
  load hit all of them equally.
- **Minimum is the primary number.** Medians on this machine swing ±50% because of desktop
  background processes (Steam, Chrome, Wallpaper Engine). The minimum is the least contaminated
  estimate of the true cost.
- Every implementation must print the same checksum; a wrong answer is a failed run.

## The bug that had to be fixed first

The first version of the C/C++/Rust/Go sieves scanned **all N indices** in the marking loop, while
the MAGIC and Goose versions stopped at `i*i >= N` (the correct bound). MAGIC looked ~25% faster
purely because it did less work. Fixing the four implementations to use the same bound made the
difference disappear. This is the single most common way language benchmarks lie.

## Workload 1 — sieve of Eratosthenes, N = 10,000,000

Identical work: mark composites only for `i*i < N`, then count every prime. Expected `664579`.

| Language | Build (ms) | **Min (ms)** | Median (ms) | Peak RSS (MB) | Correct |
|---|---:|---:|---:|---:|---|
| C `gcc -O2` | 372 | **31** | 62 | 13.7 | yes |
| C++ `g++ -O2` | 438 | **34** | 63 | 14.2 | yes |
| Rust `rustc -O` | 480 | **31** | 64 | 14.2 | yes |
| Go `go build` | 1189 | 47 | 71 | 16.8 | yes |
| Goose `goose build` | 782 | 42 | 62 | 13.9 | yes |
| MAGIC `magicc build` | 678 | **31** | 56 | 13.7 | yes |

### Heavy run — same sieve at N = 100,000,000

100 MB array, expected `5761455`, sequential with 1.5 s pauses between runs, 5 rounds.
Background apps were closed by the owner before this run.

| Language | Min (ms) | Median (ms) | Peak RSS (MB) | Correct |
|---|---:|---:|---:|---|
| C | 524 | 538 | 99.5 | yes |
| C++ | 524 | 554 | 100.0 | yes |
| Rust | 523 | 570 | 100.1 | yes |
| Go | 583 | 724 | 102.8 | yes |
| Goose | 556 | 599 | 100.2 | yes |
| MAGIC | **492** | **538** | **99.5** | yes |

At this size the spread is ~18% and stable (the 10M run swung 2x between sessions). MAGIC and C lead,
Rust is within 5%, Goose within 12%, Go trails by 15-35%. Memory is the array plus a small runtime
delta; Go carries ~3 MB of runtime overhead.

MAGIC being marginally ahead of C here is the 32-bit inner index and the compiler's fill loop versus
C's `memset` plus 64-bit inner loop — a code-shape difference, not a language advantage.

## Workload 2 — fannkuch-redux, n = 10

Expected `73196` / `38`. Five runs each (not interleaved; treat as indicative).

| Language | Build (ms) | Median (ms) | Best (ms) | Peak RSS (MB) | Correct |
|---|---:|---:|---:|---:|---|
| C | 199 | 188 | 171 | 4.1 | yes |
| C++ | 291 | 174 | 172 | 4.6 | yes |
| Rust | 303 | 188 | 172 | 5.3 | yes |
| Goose | 270 | 188 | 186 | 4.4 | yes |
| MAGIC | 221 | 188 | 174 | 4.2 | yes |

## What the numbers say

1. **C, Rust and MAGIC are identical on the sieve: 31 ms.** MAGIC emits C; this is what that means.
   Its bounds checks are not measurable in a memory-bound loop.
2. **Goose is 35% slower than C on this workload** (42 vs 31 ms). Its memory model does not
   automatically win on dense byte-array code; it wins on pointer-shaped data, which this is not.
3. **Go is 50% slower** (47 vs 31 ms) and uses 23% more memory (16.8 vs 13.7 MB) — runtime overhead,
   visible even on a trivial workload.
4. **No 2-3x anywhere.** Not from Goose, not from MAGIC, not from C++.
5. **Medians are unreliable here.** The min is stable across runs; the median tracks background load.

## The honest answer to "why does this syntax beat Rust/Go/Python?"

It does not — not on runtime. Syntax is a front end; it is gone after compilation. The table that
puts MAGIC's "Execution Speed" at "fastest" is comparing developer experience with machine
performance, which is a category error.

| Axis | Can syntax win? | Why |
|---|---|---|
| Runtime on the same algorithm | no | the backend emits the same machine code |
| Peak memory | yes, indirectly | if the language makes the compact representation the default |
| Developer time | yes, strongly | fewer concepts, less ceremony |
| Compile time | yes | simpler semantics, no borrow checker |
| Tail latency | yes, vs GC languages | no collector, deterministic frees |

## Where 2-3x is actually possible

Not "a better compiler". A compiler cannot beat LLVM at LLVM's job. The wins that large come from
**doing less work**, and they are real in exactly three places:

1. **Representation.** Idiomatic Rust `Box<Node>` trees allocate one node at a time and chase
   pointers. A flat array of nodes with 32-bit indices plus a bump arena can be **2-4x faster and
   3-5x smaller** on the same algorithm. That is a language-level default, and it is the single
   most credible path MAGIC has.
2. **Tail latency.** Go's GC creates pauses Rust does not have. Against Go, deterministic memory can
   win P99 by 10x+ even when throughput ties.
3. **Compile time.** Rust's 100k-LOC builds are minutes; a simple language can be seconds — 10-40x
   with zero runtime change.

Note the baseline. Against **idiomatic** Rust, 2-3x is achievable. Against **hand-optimized** Rust
(arena + indices + offsets), the residual shrinks to ~1.1-1.3x — exactly what Goose's 1.24x says.
Any claim of 2-3x must name which baseline it beats.

## The experiment that would settle it

`binary-trees` (Benchmarks Game): allocate millions of small nodes, check the tree.

- Rust idiomatic: `Box<Node>`, pointer recursion.
- Rust optimized: arena + index links.
- MAGIC with structs + flat node array + arena.

Prediction to test, not assert: MAGIC lands 2-3x over idiomatic Rust and 1.0-1.3x over optimized
Rust, with 2-4x lower peak RSS than idiomatic. If it lands there, the claim is real and worth
building. If it only ties optimized Rust, MAGIC's honest product is developer speed + compile speed
+ bounded memory, not raw runtime.

**Blocker:** MAGIC has no structs yet, so binary-trees cannot be written. That is the next feature,
ahead of any syntax work.

## Missing

- n-body / spectral-norm (FP-heavy) and binary-trees (allocation-heavy) are not in v1.
- fannkuch was not interleaved; re-run it with the same 15-round method before quoting.
- Go's build time includes toolchain setup on first use; ignore the build column for Go.
