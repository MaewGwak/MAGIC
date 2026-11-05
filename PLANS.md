# 🔮 MAGIC — Plan

## Where we are

v0.4: working compiler with structs, array parameters, arena (bump allocator), casts, assert,
`sqrt`, and `print_prec`. 112 tests green. Eight benchmark workloads — see `BENCHMARKS.md`.
Compiler: content-cached builds, 16-way parallel chunks, incremental rebuild 265 ms vs rustc 362.

## The one thesis

MAGIC does not win on codegen. It wins if its **memory model** produces measurably lower
peak RSS and P99 latency than hand-optimized Rust on the same workload. Everything below
serves that measurement.

## Roadmap

### v0.1 — arrays (DONE)
- `var a: [T] = [value; count];`, checked indexing, compiler-inserted frees.
- Exit: sieve correct; tests green.

### v0.2 — surface, not semantics
- Newline terminators and optional semicolons (`a[i]` must never swallow the next line).
- `for i in a..b` as sugar for the three-clause while.
- Structs with field access (needed by the parser benchmark).
- Exit: every doc example compiles without semicolons; v0.1 sources unchanged.

### v0.3 — the actual differentiator
- `arena` block: bump allocation, one free at block exit, printed peak bound.
- Compiler prints `memory: <= N bytes` per function.
- Exit: sieve and JSON parser run inside an arena with a printed bound within 1.5x of measured peak.

### v0.4 — indentation edition (optional)
- `MAGIC edition 2027`: colon + indentation, no braces.
- Old editions keep compiling. This is a front-end change only; the memory model is untouched.
- Exit: the same program compiles under both editions with identical output.

### 1.0 — the promise
- Source written for 0.x still compiles (with warnings) on 1.0.
- No hidden allocation, no GC, no runtime.

## Kill gates

See `KILL.md`. The day-90 gate: on sieve + parser + pointer workload vs hand-optimized Rust,
**≥1.3x lower peak RSS**, **≥1.5x lower P99**, and **never below 0.90x CPU** on any program.
All three, or the language premise is dead.

## Non-goals (for now)

- Being faster than C on raw codegen.
- Replacing C++ for existing codebases.
- A package manager, debugger, LSP, or async runtime before the memory model is proven.
- Python-style indentation before the semantics are measured (it is an edition, not a rewrite).

## Working method

- Every design change goes through the 5 orchestrator / 5 coder / 5 QA vote; 4/5 approval required.
- No claim without a number. No rejection without a test or a stated test that would settle it.
- Ship the smallest verifiable increment; keep `cargo test` green between increments.
