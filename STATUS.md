# MAGIC status — what is proven, what is not

## 1.0.0 is withheld

The 1.0.0 tag is withdrawn (currently 0.12.x). The best-of matrix did not establish a
language-speed win: same-algorithm comparisons are at parity or slower, and the five short
workloads are indistinguishable at the process/start-up floor. The remaining 1.0.0 gate is
correctness/safety and a real memory-bound proof, not benchmark theatre.

## Proven, on this machine

- **178 tests, all green** on a clean clone (`cargo test`; two skipped only if a C compiler is absent).
- **Compile speed**: 25 ms cold / 10 ms incremental for the 2,000-function source
  (gcc 47 ms, rustc 109 ms).
- **Static floor**: 5,208-byte `hello` static ELF, no libc or PT_INTERP; native IR path
  needs no C compiler.
- **Normal safety checks**: `a[4]`, `a[-1]`, `a[len]`, huge simple indices, division and
  modulo by zero panic with `file:line` and exit 1.
- **Arena discipline** on the tested cases: escape rejected at compile time, single free
  per array, ASan-clean arena tests.
- **IEEE double** parity with gcc on the tested programs.
- **No GC**.

## Fixed safety blockers (0.12.7)

- **Index overflow**: index arithmetic is evaluated in 64-bit before the bounds check in
  IR, C and legacy native backends; a wrapped i32 product now panics, not reads OOB.
- **`[T+]`/`[T]` alias**: passing the same variable as both a fixed view and a growable
  parameter is rejected at type-check time (push/reserve could otherwise invalidate the view).
- **IR vec-parameter push/pop**: `LoadPtr` destinations are no longer register-promoted;
  push/pop through `[T+]` parameters now preserves pointer/length/capacity.
- **File I/O**: fd table, cap checks, signed -1 returns, missing-fd safety in native/IR.

## Not proven / known losses

- No same-algorithm/same-compiler workload where MAGIC beats optimized C/C++/Rust.
- Native IR n-body 613 ms vs Rust 219 ms; fannkuch 63 ms vs 9 ms; raytrace 7.3 ms vs 3.2 ms.
- **`--report` is not a memory proof**: it prints a syntactic fixed-array sum and now says
  "NOT peak RSS"; recursion/growth/allocator are excluded.
- GitHub Actions history before the 0.12.7 release build fix was red; local clean-clone
  fast gates pass, but CI needs a fresh green run before claiming 7 days.
- Legacy `--backend native` is experimental; index safety is tested, but it is not the
  production backend.
- Strings now support `len`, `s[i]`, `==`/`!=`, `+`, `str_cat`, `[str]` arrays and
  `[str+]` vectors in C and IR; slices, formatting, native backend and
  collections/generics/FFI/AI/web are not ready.

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
