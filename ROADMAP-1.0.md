# Roadmap to MAGIC 1.0.0

Status: 2026-11-06, after v0.8.48. This document records the verified state, the remaining gap to the
1.0 objectives, and the milestones that can actually close it.

## Verified state

* Language surface: every program under `examples/`, `bench-suite/`, `bench-db/`, `bench-fannkuch/`
  and `bench-bt/` compiles through both the C backend and the IR backend, and the differential gate
  (`verify-ir.sh`, 36 programs) requires identical stdout and exit status.
* Native x86-64 backend: `as` + `ld` only, no C compiler in the path (`verify-native.sh`).
* Security: parser/recursion/chain limits with a deterministic fuzzer (`tests/fuzz.rs`), absolute
  build tool paths, symlink refusal, cache stamping, checked affine bounds proofs with non-negative
  induction variables, `i32` wrap semantics, allocation NULL/overflow guards, and a
  `negative_idx`/`huge_affine`/`oom_alloc` differential corpus.
* Gates: 161 Rust tests, native gate, IR differential gate, benchmark matrix, memory matrix, docs
  check — all green.
* Peak memory: parity with the C backend across the matrix (payload-dominated workloads); arena
  blocks are real `mmap` regions freed at block end (32.9 MB vs C 32.8 MB, AST 64.2 MB for two
  32 MB regions).
* Speed through `--backend ir` versus the C backend (medians): sieve 1.9x, btree 1.7x, sqlscan 3.1x,
  hashmap 2.6x, bufpool 1.4x, matmul 3.8x, raytrace 3.0x, vecpart 4.6x, particles 3.8x, nbody 2.8x.

## Gap to the stated 1.0 targets

The objectives "at least 3x faster and 5x+ lower memory than the best C/Rust/Go/Goose baselines on
the benchmark matrix" are **not currently met**, and the v0.8.37 memory audit shows they cannot be met
by codegen changes alone: the workloads are dominated by their payload and every backend already
bit-packs booleans and zero-fills allocations. Closing the speed gap requires new machinery; closing
the memory gap requires algorithmic/data-structure advantages, not a better emitter.

## Milestones

1. **M1 — SLP/SIMD for float struct loops.** Vectorize straight-line blocks of independent `f64`
   field updates on adjacent offsets (particles/vecpart shape). Target: 2–3x on those two programs.
   Acceptance: IR differential gate plus new per-program checksums, no regression >2% elsewhere.
2. **M2 — parallel `par for`.** Extract the body as a generated worker, capture array pointers and
   scalar parameters, run ranges on pthreads, join before continuing. Target: match the C backend's
   OpenMP results on the `*_par` variants.
3. **M3 — function-wide register allocation.** Replace the block-local allocator with linear scan
   over the CFG (live intervals, spilling around calls). Target: 10–20% across the matrix.
4. **M4 — IR as the default backend.** Flip `native` to the IR once M1–M3 are in and every feature
   gate passes; keep the AST backend as `--backend native-legacy` for one release.
5. **M5 — memory policies.** Extend arenas to vectors, add compressed containers in the runtime, and
   re-run the memory audit. Only a structural advantage (fewer bytes per element) can move the 5x
   claim.

## Acceptance proposal for 1.0.0

Ship 1.0.0 when: all features work on both backends, every security item above is covered by tests,
all gates are green, and the IR is the default with documented performance/memory ratios against the
C baselines. Absolute "3x faster and 5x less memory on every workload" should be restated per workload
(the n-body/I/O/hash payload bounds make it physically unreachable on the current matrix); this
document is the evidence for that restatement.
