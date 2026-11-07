# Goose as the primary baseline

[Goose](https://github.com/aardappel/goose) is a closer competitor than Rust for
MAGIC's memory-focused direction. It is a real language, not a runtime or an AI
agent: a memory-safe systems language with no heap allocator, no GC and no
lifetime annotations. It compiles to C and ships an in-process TinyCC JIT.

Its published results ([bench/results.md](https://github.com/aardappel/goose/blob/master/bench/results.md))
claim a geometric mean of 1.13x over the best safe Rust, 1.24x over expert C++
and 3.51x over idiomatic C++ on 16 whole-process benchmarks, with 1.25x/1.32x/1.95x
lower peak memory against those tiers. Those are Goose's numbers on its own
hardware, not ours.

## Why Goose matters to MAGIC

Goose already has the things MAGIC's roadmap says it needs:

- data-stack allocation and stable references across growth
- compiler-inferred roots and lifetime checks with no annotations
- bounds-check elimination verified by `checkfree`/`--bce-lines`
- inline variable-sized enums (its 4x-memory row against Rust enums)
- one C file as the portable output, plus a one-command in-process JIT
- formatter, LSP, package fetching, `--diagnostics json`, debug checks
- 16 benchmarks with checksums, whole-process timing and published losses

MAGIC currently has none of the memory model or BCE. It has a small static
footprint and fast compiles, but it loses badly on the features that make Goose
interesting.

## Local sanity checks (this machine, 2026-11-07)

These are sanity checks, not a benchmark verdict:

| check | Goose v2026.4 | MAGIC v0.12.9 |
|---|---|---|
| hello binary | 72,024 B dynamic | 5,208 B static |
| hello peak RSS (best of 5) | 2.1 MB | 0.4 MB |
| hello wall start | < 0.01 s timer floor | < 0.01 s timer floor |
| build hello | works (`goose build`, gcc) | works (`magicc build --static`) |
| run from source / JIT | **fails here**: `libtcc: relocating the generated C failed`, `mprotect failed` | `magic run` works |
| 4M `sum`, small, gcc, 1 rep | 6.6 ms / 23.8 MB | not yet implemented as a matched benchmark |

The JIT failure is environmental (TinyCC `mprotect`, likely SELinux), not a
language verdict. It does show why every claim needs a local reproduction.

## The honest target

Do not try to beat all 16 Goose benchmarks at once. Goose is a 30k-line C++
compiler with an experienced language designer and an AI-built implementation;
its own docs publish losses (calc, lru, scene). A credible path is:

1. **Parity on three workloads**: a flat sum/scan, a growable records workload,
   and a pointer-shaped tree. Same source semantics, same gcc, same checksums.
2. **One axis where MAGIC is genuinely better**: a checkable worst-case heap
   bound (`magic check --bound=...` rejects a program that cannot fit), not just
   data-stack reporting.
3. **One deployment axis**: a 5 KB WASM/static plugin with no runtime and
   machine-readable diagnostics, where Goose's C runtime/OS assumptions are a
   liability.
4. **DX**: DWARF line info, value-bearing panics and a lint suite. Goose already
   has LSP/formatter; MAGIC must at least match before claiming maintainability.

If MAGIC cannot match Goose's data layout, it will not win the memory benchmarks
by adding intrinsics or another backend. The memory model is the product.

## What not to copy

- Do not bake game/graphics/stdlib breadth into MAGIC's compiler; Goose already
  has that ecosystem.
- Do not chase 16 benchmarks before text, Result and DWARF exist.
- Do not call Goose's aggregate numbers "4x less RAM everywhere": the 4x row is
  variable-sized enums vs Rust enums; the aggregate memory ratio is 1.25x vs
  Rust.
