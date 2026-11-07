# MAGIC status — what is proven, what is not

## 1.0.0 is withheld

The 1.0.0 tag is withdrawn (currently 0.12.x). The best-of matrix did not establish a
language-speed win: same-algorithm comparisons are at parity or slower, and the five short
workloads are indistinguishable at the process/start-up floor. The remaining 1.0.0 gate is
correctness/safety and a real memory-bound proof, not benchmark theatre.

## Proven, on this machine

- **172 tests, all green** on a clean clone (`cargo test`; two skipped only if a C compiler is absent).
- **Compile speed**: 25 ms cold / 10 ms incremental for the 2,000-function source
  (gcc 47 ms, rustc 109 ms).
- **Static floor**: 13,224-byte `hello` static ELF, no libc or PT_INTERP; native IR path
  needs no C compiler.
- **Normal safety checks**: `a[4]`, `a[-1]`, `a[len]`, huge simple indices, division and
  modulo by zero panic with `file:line` and exit 1.
- **Arena discipline** on the tested cases: escape rejected at compile time, single free
  per array, ASan-clean arena tests.
- **IEEE double** parity with gcc on the tested programs.
- **No GC**.

## Not proven / known losses

- No same-algorithm/same-compiler workload where MAGIC beats optimized C/C++/Rust.
- Native IR n-body 613 ms vs Rust 219 ms; fannkuch 63 ms vs 9 ms; raytrace 7.3 ms vs 3.2 ms.
- **i32 index overflow** can bypass the bounds check (safety blocker).
- **`[T+]` → `[T]` view invalidation**: realloc can leave a view dangling; ASan-confirmed
  use-after-free (safety blocker).
- **`--report` is not a memory proof**: it sums syntactic fixed-array sizes and ignores
  recursion/growth/allocator overhead. The measured RSS can be orders of magnitude higher.
- Default IR `file_*` validation was fixed in 0.12.x; clean-clone `verify.sh -Fast` passes,
  but GitHub Actions history before that fix was red. Re-run CI and check the badge.
- Legacy `--backend native` is not production-grade; it is kept for experiments only.

## Benchmark policy

Algorithmic variants (blocked sieve, closed-form particles, level-order trees, `par for`,
SIMD intrinsics) are documented separately and **must not be presented as language-speed
wins**. Any claim must use the same algorithm and compiler on both sides, publish raw runs,
and include compile/memory/diagnostics separately.

## Next

1. Fix the three safety blockers (overflow, view invalidation, proof report).
2. Make `--report` a sound bound or remove the claim.
3. Re-run clean-clone CI and keep it green.
4. Productize fast dev loop (`magic watch`/incremental) and static footprint,
   which are the defensible advantages.
