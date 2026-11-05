# 🔮 MAGIC — Where it can actually win

The thesis: **attack costs the language forces, not costs a programmer can avoid.**
If Rust can express the compact representation with enough discipline, MAGIC cannot win by
having a better compiler. It can win by making that representation the default, and by
proving things the other language cannot state.

## C / C++

| Weakness | Forced? | MAGIC response | Status |
|---|---|---|---|
| No bounds safety; ~70% of serious CVEs are memory-safety | yes | every index checked, no unsafe escape | done (v0.1) |
| Manual free; leaks, double free | yes | compiler inserts one free per array | done (v0.1) |
| UB on overflow, uninitialised data | yes | not trapped yet | **gap** |
| Header/module model, build systems | mostly | one file in, one C file out, `magicc build` | done |
| Template error walls | yes | no templates | by omission |

Honest limit: C/C++ codegen is the same backend; **MAGIC cannot beat C on the same layout.**
It can only beat it by changing the layout and making that safe.

## Rust

| Weakness | Forced? | MAGIC response | Status |
|---|---|---|---|
| Borrow checker / lifetime friction | yes | no borrow checker; arrays are owned locals | done |
| Compile times 10-40x Go | yes | simpler semantics, no trait solving, no borrowck | **needs measurement** |
| Defaults bad for data-oriented code (`Vec<Box>`, pointer chase) | yes, by default | flat arrays + indices are the only array model | done |
| No stable ABI | yes | emits C, C ABI | done |
| Async colouring | yes | no async yet | **gap** |

Honest limit: against **hand-optimized** Rust (arena + indices + offsets) the residual is
~1.0-1.3x. Goose measured 1.24x. Anyone claiming 2-3x must say "versus idiomatic Rust".

## Go

| Weakness | Forced? | MAGIC response | Status |
|---|---|---|---|
| GC pauses / tail latency | yes | no GC, deterministic frees | done (no runtime) |
| Runtime overhead per process (~3 MB, slower) | yes | no runtime | observed in benchmarks |
| Less layout control | yes | arrays are the only container; indices are explicit | done |
| Slowest of the six on the 100M sieve | — | measured 15-35% behind | measured |

## Goose

| Weakness | Forced? | MAGIC response | Status |
|---|---|---|---|
| Whole-program only | yes | MAGIC is also single-file today | **same gap** |
| No function pointers / dynamic dispatch | yes | not needed for v0.1 | by omission |
| No growable data across recursion | yes | arrays + params (planned) | **planned** |
| Strict explicit casts (i64 → i32 rejected) | yes | same strictness is planned | **planned** |
| Tiny ecosystem / docs | yes | docs-first, tests in-repo | in progress |

## What MAGIC cannot win

- Dense numeric loops (same LLVM/GCC backend, same machine code).
- Memory bandwidth (the array is the array).
- Any workload where the idiomatic version already uses flat arrays.

Measured evidence: sieve 10M and 100M, fannkuch n=10 — every language within 8-18%, MAGIC tied
with C. The language is not the variable; the representation is.

## Where the 2-3x actually lives

1. **Pointer-shaped data.** `Box<Node>` per node vs a flat array of nodes: 2-4x speed,
   3-5x memory. This is the binary-trees experiment.
2. **Tail latency vs GC.** P99 under allocation churn; not throughput.
3. **Compile time.** 10-40x, with zero runtime change.

## Algorithms and ideas worth stealing

| Idea | Source | Use in MAGIC |
|---|---|---|
| Region inference | Tofte–Talpin, MLKit, Cyclone | arena lifetimes without annotations |
| Destination passing | MLIR bufferization | build values in place, no copies |
| Perceus RC + reuse | Koka, Lean 4 | reference counting with in-place reuse |
| Generational references | Vale | escape hatch for long-lived values |
| Relative references | Goose | 1-4 byte links instead of 8-byte pointers |
| Static resource bounds | AARA / RAML | print `memory <= N bytes` per function |
| PGO + BOLT | LLVM | whole-program layout without user flags |
| SoA / index compression | data-oriented design | the default container shape |

## The product sentence

> MAGIC is a language where the compact, allocation-free representation is the **default**, the
> compiler **proves** the memory bound, and the source stays short enough for a human and a model
> to rewrite safely.

That is a real product. "Faster than Rust at everything" is not.

## Order of work

1. `struct` — done (v0.2).
2. **array parameters (slices)** — the missing piece: pools must be passable so recursion can
   build a tree. Without this, binary-trees cannot be written at all.
3. `arena` block + printed bound.
4. binary-trees: Rust `Box<Node>` vs Rust flat arena vs MAGIC flat pool.
5. Verdict against both baselines; publish the negative result if the residual is ~1.0x.
