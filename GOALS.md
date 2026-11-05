# 🔮 MAGIC — win conditions

"ชนะ Rust/Goose/C/C++" ต้องแยกเป็นแกน เพราะแกนหนึ่งชนะได้ อีกแกนชนะไม่ได้ — และการโกหก
ตัวเองตรงนี้คือวิธีที่ภาษาใหม่ตาย

## The four axes

| Axis | Baseline | Target | Winnable? | Why |
|---|---|---|---|---|
| **Compile speed** | rustc on 100k LOC | **≥10x faster**, ≤1.2x `go build` | **yes** | simpler front end, parallel, cache |
| **Memory** | idiomatic Rust on pointer-heavy code | **≥2x lower peak RSS** | **yes** | flat arrays + arena are the default |
| **Memory** | hand-optimized Rust (arena + index) | ≥1.0x (non-loss) | partial | residual ~1.0-1.3x |
| **Safety** | C/C++ | 100% bounds-checked, no UB in safe code | **yes** | already checked indices |
| **Tail latency** | Go under allocation churn | **≥2x lower P99** | **yes** | no GC |
| **Throughput** | C/Rust on dense code | 0.95-1.05x | **not a win axis** | same backend, same machine code |

**The rule:** win on compile speed + memory + safety + tail latency. Never claim throughput
against hand-optimized C; claim non-loss there.

## Hardware: what actually helps compilation

Honest mapping — most "use the GPU" ideas are wrong for a compiler.

| Resource | Real use | Not useful for |
|---|---|---|
| **CPU cores** | parallel parsing/typechecking per function; parallel codegen per module; work stealing | — (this is the main lever) |
| **RAM** | whole program in memory; content-addressed incremental cache; watch mode; PGO data | — |
| **GPU** | batch-compiling many modules at once; ML-guided optimization decisions (inlining, layout, register allocation); offline autotuning of layout/flags | compiling one small function faster |
| **NPU / TPU** | inference for optimizer heuristics (MLGO-style); training layout models offline | critical-path compilation of a single file |
| **Disk** | build cache, object cache | — |

The 10x compile win comes from **doing less work + parallel + cache**, not from accelerators.
GPU/NPU are for the ML decisions layered on top, and those are research-grade, not v0.3.

## Milestones

| # | Milestone | Exit criterion | Status |
|---|---|---|---|
| M0 | structs | 16 tests green | **done** |
| M1 | array parameters (slices) | pool passes into a recursive function; tests | next |
| M2 | arena block + printed bound | `memory <= N bytes` within 1.5x measured | |
| M3 | binary-trees benchmark | 3 implementations: Rust Box / Rust flat / MAGIC flat | |
| M4 | parallel + incremental compiler | 10x rustc on 100k generated LOC, cached rebuild <150 ms | |
| M5 | mini-Minecraft | chunk gen + meshing + world update in MAGIC | months |
| M6 | OS in VM | boots, runs a shell, in QEMU | years |

M5 and M6 are real multi-year targets. They need features MAGIC does not have: dynamic arrays,
generics, threads, file I/O, a stdlib. Naming them as milestones is honest; pretending they are
next is not.

## What "win" is measured on

1. **binary-trees** — memory + speed vs idiomatic and optimized Rust.
2. **compile benchmark** — 100k LOC generated from the test programs, cold and warm.
3. **P99 harness** — a keep-alive service under load, MAGIC vs Go.
4. **mini-Minecraft** — chunk generation throughput and memory, MAGIC vs Rust.

Publish every loss. A negative result is a result.

## The magic clause

MAGIC must feel like Python and be priced like C. That is a **developer-time** win, measured as
lines of code and time-to-working-program, not as FLOPS. Keep it separate from the runtime table
or the whole argument collapses.

## Non-goals

- Beating LLVM at codegen.
- 2-3x throughput over optimized C/Rust.
- GPU/NPU acceleration of ordinary programs.
- A full OS before dynamic arrays, threads, and I/O exist.
