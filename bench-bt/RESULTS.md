# binary-trees — the decisive benchmark

One algorithm, three representations, same machine, same checksum.

**Expected total:** `568328208` (17 depths × 2^25 nodes minus the ramp; all three agree).

## Results

| Implementation | Time (ms) | Peak RSS (MB) | vs idiomatic | vs flat | Correct |
|---|---:|---:|---:|---:|---|
| Rust — `Box<Node>` (idiomatic) | 16,517 | 74.6 | 1.0x | 0.09x | yes |
| **MAGIC — flat index pool** | **1,931** | **20.7** | **8.55x faster** | 0.76x | yes |
| Rust — flat index arena | 1,459 | 25.6 | 11.3x faster | 1.0x | yes |

Run: one pass each, n = 4..20, ~33.5M nodes total, AMD Ryzen AI 7 350, GCC 16.2,
rustc 1.99.1, magicc 0.1.0. Peak sampled every 5 ms.

## What this proves

1. **Representation dominates everything.** Flat indices beat `Box<Node>` by **8.5-11.3x** and use
   2.9-3.6x less memory. That is the 2-3x hypothesis, exceeded — and it is not a compiler win, it is
   a data-layout win.
2. **MAGIC beats idiomatic Rust by 8.55x** with 3.6x less memory. That is a real, measured result.
3. **MAGIC loses to hand-optimized Rust by 32%.** Bounds checks and less aggressive inlining. The
   honest claim is therefore: *MAGIC wins against how people actually write Rust, not against a
   Rust expert who already writes flat arenas.*
4. **A language can only win where it changes the default.** Rust can express the flat version —
   it did, in 1,459 ms. MAGIC's bet is that flat is the only easy path, so ordinary code gets the
   fast representation without the expert.

## Why MAGIC is 32% behind Rust flat

- Every index goes through `magic_index` (bounds check), Rust's slice indexing in this loop is
  provably in-bounds and LLVM removes it.
- MAGIC's recursion passes `(ptr, len)` pairs for each array; Rust passes two slices, same shape,
  but rustc inlines more aggressively.
- No arena yet: MAGIC's arrays are `malloc`'d once in `main`, which is already the right shape; the
  gap is check + inlining overhead, not allocation.

## Honest caveats

- One run each. The 8.5x/11.3x gap is far larger than machine noise, so the direction is safe;
  the exact ratio is not.
- `Box<Node>` is the standard way this benchmark is written. Claiming a win over it is legitimate,
  but it must always be labelled "idiomatic".
- The MAGIC program uses parallel `i32` arrays, not `struct` arrays — the flat representation is
  what matters, not the syntax.
- Total fits i32 (568M), so no overflow check was needed; a larger n would need i64.

## Compiler flags — no portable win over -O3

Same generated C, five runs each:

| Flags | Min (ms) | Median (ms) |
|---|---:|---:|
| `-O3` | **707** | **715** |
| `-O3 -funroll-loops` | 713 | 715 |
| `-O3 -march=x86-64-v2` | 714 | 715 |
| `-O2` | 1862 | 1876 |

`-O3` stays the default. Unrolling and a portable SIMD baseline change nothing here; `-O2` costs
2.6x on this recursive workload, which is why `--dev` (`-O0`) is strictly for iteration.

## Bounds-check ceiling — negative result

Stripped every `magic_index` wrapper from the generated binary-trees C (regex, checksum still
`568328208`) to measure the ceiling of an unchecked mode:

| Build | Min (ms) | Median (ms) |
|---|---:|---:|
| with bounds checks | **709** | **716** |
| checks stripped | 773 | 779 |

Removing the checks made it **slower**. The checks are perfectly predicted, and removing them
changes gcc's inlining and code layout for the worse. A `checkfree`/unchecked block would buy
nothing on this evidence, so it was not implemented.

## PGO — no benefit here either

| Build | Min (ms) | Median (ms) |
|---|---:|---:|
| `gcc -O3` | 748 | 748 |
| `gcc -O3 -fprofile-use` | 745 | 747 |

Within noise. The recursion is regular and already well-optimised; profile data does not buy
layout or inlining wins. Not exposed as a compiler flag.

## Round 4 — verified, five runs, matched flags

| Implementation | Runs | Min (ms) | Median (ms) | Peak RSS (MB) |
|---|---:|---:|---:|---:|
| **MAGIC, default `-O3`** | 5 | **713** | **727** | **20.2** |
| MAGIC, `--native` | 5 | 809 | 822 | 20.7 |
| Rust flat, `-C opt-level=3 -C target-cpu=native` | 5 | 1,344 | 1,359 | 20.7 |
| Rust `Box<Node>`, native | 3 | 18,360 | 18,701 | 70.1 |

All four print `568328208`.

- **MAGIC is 1.87x faster than optimized Rust**, with slightly less memory (20.2 vs 20.7 MB).
- **MAGIC is 25.7x faster than idiomatic Rust**, with **3.5x less memory**.
- **`--native` is slower than the default** here (822 vs 727 ms median). `-march=native` enables
  wider vector code that does not help pointer chasing, and can downclock. Keep it opt-in.

## Round 2 — matched optimization flags

The 32% loss was **flags, not the language**. `magicc build` used `-O2` with generic x86-64.

| Build | Time (ms) | Peak RSS (MB) | vs Rust flat |
|---|---:|---:|---:|
| Rust flat, `rustc -O` | 1,459 | 25.6 | 1.00x |
| Rust flat, `rustc -C opt-level=3 -C target-cpu=native` | 1,452 | 25.6 | 1.00x |
| MAGIC, `gcc -O2` (old default) | 1,931 | 20.7 | 0.75x |
| **MAGIC, `gcc -O3 -march=native`** | **810** | **20.2** | **1.79x faster** |
| Rust `Box<Node>`, native | 16,739 | 75.6 | — |

With matched flags MAGIC beats the optimized Rust baseline by **1.79x** with **1.27x less memory**,
and beats idiomatic `Box<Node>` by **20.7x** with **3.7x less memory**.

Caveats: single run each; the Rust version passes `&mut [i32]` fat pointers while MAGIC passes
`(ptr, len)` pairs, which is a real implementation difference; `-march=native` is not portable.
**Action taken:** `magicc build` now defaults to `-O3`.

## Next

- `--native` opt-in flag for `-march=native`
- 5 runs per implementation before publishing ratios
- M2: arena + printed static bound, with tests
- M4: compile-speed benchmark (parallel/incremental is the 10x lever)
