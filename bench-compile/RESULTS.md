# Compile speed

## v0.6.0 — Linux, dead-function elimination (2026-11-06)

2,000 trivial functions (~90 KB), `bench-compile/run.sh`, medians of 3 runs (ms):

| Metric | MAGIC gcc | MAGIC clang | rustc -O | goose build |
|---|---:|---:|---:|---:|
| cold | **94** | **98** | 223 | 120 |
| one reachable edit | **21** | **17** | 223 | 123 |
| no-op | **15** | — | — | 121 |

Dead-function elimination keeps only the functions reachable from `main`: the generated C falls
from 170 KB to 10 KB, so the C backend stops being the bottleneck. Goose's 120 ms comes from
constant-folding the whole program away; MAGIC now matches it without folding, and wins every column.

# M4 — compile speed, first measurement

> Superseded: the current numbers are in "Round 42 — after 30+ rounds of language features
> (corrected in round 43)" and "Round 44 — default compile flags" below. This first section is
> kept as the historical record; its 265 ms one-edit figure later turned out to be a no-op build.

## Current numbers (v0.4, min of 3 runs)

| Metric | MAGIC | rustc |
|---|---:|---:|
| cold, 2,000 functions | 495 ms | 324 ms |
| one-function rebuild | 271 ms | 324 ms (full) |
| no-op rebuild | **30 ms** | — |
| front-end only | 43 ms | — |
| binary size (`big.exe`) | 167 KB | — |

2000 functions, ~90 KB of source, each function `fn fnvN(x: i32) -> i32 { x + N }`.
Same shape in Rust. Machine: AMD Ryzen AI 7 350, rustc 1.99.1, GCC 16.2.

## Results

| Phase | Time | vs rustc |
|---|---:|---:|
| MAGIC front end only (`magicc emit`) | **49 ms** | **7.4x faster** |
| MAGIC full build (`magicc build`, front end + gcc + link) | 1,337 ms | 3.7x slower |
| rustc -O (compile + link) | 362 ms | 1.0x |

## Fix 1 — content cache (shipped)

`magicc build` now compares the newly generated C with the existing `.c`; if identical and the binary
exists, gcc is skipped entirely.

| Case | Before | After |
|---|---:|---:|
| Rebuild with no changes | 1,341 ms | **13 ms (103x)** |
| Rebuild after a real code edit | 1,341 ms | 1,362 ms (unchanged) |

The no-op case is solved. A real edit still pays the full gcc cost, because the whole program is one
translation unit. The next lever is per-function caching or splitting the C output into parallel TUs.

## Round 44 — default compile flags

Every gcc invocation now also passes `-fno-asynchronous-unwind-tables -fno-ident`:

| Metric | before | after |
|---|---:|---:|
| `big.exe` size | 201,673 B | **167,105 B** (−17%) |
| cold | 497 ms | 495 ms |
| one-function mid-edit | 280 ms | **271 ms** |
| ray tracer / n-body runtime | — | unchanged (verified both ways) |

Smaller binaries and a slightly cheaper rebuild at no runtime cost. The `.flags` stamp gained a
`-nou` suffix so old caches rebuild once.

## Round 42 — after 30+ rounds of language features (corrected in round 43)

2,000-function file, three runs each, min/median (ms):

| Metric | Min | Median |
|---|---:|---:|
| `magicc build` cold | 479 | 497 |
| `magicc build` warm (no change) | **30** | 32 |
| comment-only edit (no chunk changes) | 46 | — |
| one-function edit, mid-file | 277 | 280 |
| `rustc -O` cold | 324 | 333 |
| front-end only (`magicc emit`) | 43 | 63 |

A real one-function edit rebuilds one chunk but still pays the whole pipeline: ~45 ms front-end +
~157 ms gcc `-O3` for that chunk's 125 functions + ~70 ms link = ~280 ms. **The 41 ms figure quoted
earlier in this file was a comment-only edit that rebuilt nothing ("up to date") — retracted.**

Correct claims: a one-function rebuild is 1.16x faster than rustc's full build (280 vs 324), and a
no-op build is ~11x faster (30 ms). Cold still loses on this synthetic file; on real programs it is
at parity (`Cold compile on realistic programs` above).

## Cold compile on realistic programs (min of 3 runs)

The 2000-function file is synthetic. On the actual benchmark programs:

| Program | `magicc build` cold | `rustc -O` cold | Winner |
|---|---:|---:|---|
| fannkuch (n=10) | **179 ms** | 232 ms | MAGIC 1.30x |
| binary-trees | 237 ms | **221 ms** | rustc 1.07x |
| sieve N=100M | **164 ms** | 204 ms | MAGIC 1.24x |

So the cold-compile loss is specific to the synthetic 2000-function file, where gcc's per-function
cost dominates. On real programs MAGIC is at parity and wins two of three. The earlier "1.43x slower
cold" claim was true only for that file.

## Round 3 — header slimming and dev profile (measured, mostly negative)

Two attempts to fix the cold-build loss:

1. **Per-chunk prototypes.** Each chunk now declares only the functions it actually calls.
   Chunk header dropped from ~60 KB to 7 KB. Cold: 566 → 558 ms. **No measurable effect** —
   parsing was not the bottleneck.
2. **`--dev` (`-O0`).** Cold at `-O0`: 485 ms vs 518 ms at `-O3`. **~6% — not worth switching
   profiles for.** The per-function optimisation passes were not the bottleneck either.

What the numbers imply: cold time is dominated by **process spawn ×16, file I/O, and the MinGW
linker**, not by gcc's optimisation level or header parsing. More chunks do not help; fewer chunks
give bigger units. The floor for this architecture is ~500 ms.

Fixing cold for real needs a faster backend (Cranelift, or TinyCC for dev), not more build-system
engineering.

Also fixed: the incremental cache ignored compiler flags. `big.magic` built with `--dev` and then
without it returned "up to date" with the wrong binary. Every build now writes a `<stem>.flags`
stamp; a flag change forces a full recompile.

## Fix 2 — 8 translation units, parallel, per-chunk cache (shipped)

Programs with 32+ functions are split into 8 C files that share one header (includes, helpers,
struct typedefs, all prototypes). Each chunk compiles in parallel; an unchanged chunk reuses its
`.o` and is not recompiled.

| Case | Single file | 8 chunks | 16 chunks | rustc -O |
|---|---:|---:|---:|---:|
| Cold build | 1,341 ms | 516 ms | 566 ms | **362 ms** |
| No-op rebuild | 13 ms | 31 ms | **30 ms** | 362 ms |
| One-function edit | 1,362 ms | 358 ms\* | **265 ms** | 362 ms |

\* that run dirtied two chunks. The honest single-chunk number at 8 chunks was not measured.

**16 chunks won** because iteration latency matters more than a 10% cold penalty:
- a one-function edit now compiles **faster than rustc** (265 vs 362 ms);
- a no-op rebuild is ~12x faster than rustc;
- cold still loses 1.56x, because gcc has to parse the 2000 prototypes in every chunk and MinGW
  links the result (link alone measured 82 ms).

Windows note: switching chunk counts between builds caused `os error 32` (file locked) when
rewriting `.c` files. Fixed with a bounded retry on write, and by using a fixed 16-chunk layout.

## gcc cost by optimization level

| gcc flags | Compile time |
|---|---:|
| `-O0` | 455 ms |
| `-O1` | 960 ms |
| `-O2` | 1,322 ms |
| `-O3` | 1,311 ms |

`-O3` costs ~2.9x over `-O0` but keeps binary-trees at 758 ms instead of ~1.9 s, so the release
default stays `-O3`. A dev profile (`-O1`) is worth adding later.

## What this means

1. **MAGIC's own compiler is already fast** — 7.4x faster than rustc on the same shape of input.
   Simpler semantics and no borrow checker do what they say.
2. **The C backend destroys the win.** GCC takes ~1.29 s of the 1.34 s build. The compiler is no
   longer the bottleneck; the backend it delegates to is.
3. **A 10x end-to-end win is impossible while gcc is on the critical path at this size.** It needs
   one of:
   - a native backend (Cranelift) for dev builds, keeping C for release;
   - a content-addressed cache so unchanged functions never reach the C compiler twice;
   - splitting the C output into translation units and compiling them in parallel.

## Caveats

- 2000 trivial functions exaggerate rustc's per-function overhead and understate LLVM's optimisation
  value. Re-run with real code (e.g. the binary-trees program duplicated) before quoting.
- `magicc build` and `rustc` both write `big.exe` in the same directory; they collided in this run.
  Use separate directories.
- One run each; caches were warm.

## Next for M4

1. Measure warm-cache rebuilds (`magicc build` twice) — if the second is not much faster, add the cache.
2. Split codegen into per-function C files and compile in parallel with `-j`.
3. Only then consider a native backend.
