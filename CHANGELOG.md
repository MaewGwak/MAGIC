# Changelog

All notable changes to MAGIC. Versions follow SemVer 2.0.0; before 1.0, minor bumps may break.

### Native backend: libc file I/O

The native backend now maps `file_open`, `file_close`, `file_read`, `file_write`, `file_write_str`,
`file_seek`, `file_tell`, `file_size`, `file_read_at`, `file_write_at`, `file_pread` and
`file_pread_at` straight onto `fopen`/`fread`/`fwrite`/`fseek`/`ftell`/`fclose`, so `pageio`,
`sqlscan`, `btree` and `bufpool` build and print the same checksums without a C compiler. The native
builds are slower for now (pageio 685 ms vs 329 ms with gcc); performance work comes after feature
parity.

### Native width semantics

Native arithmetic now narrows after every operation to the declared type, matching the C backend's
`int32_t`/`uint8_t` wrapping: `2147483647 + 1` prints `-2147483648` and a `u8` at 255 wraps to 0.
A regression test builds both cases natively and checks the output.

### Reverted: register-allocation experiments

Callee-saved induction counters and scalar promotion were prototyped, measured and reverted: the
counter change moved medians by <5% and scalar promotion miscompiled the scanner benchmark. The tree
stays on the v0.8.8 codegen until the typed IR and its differential harness land
(see `NATIVE-BACKEND.md`).

### Native differential harness

`tests/native_diff.rs` runs 27 programs (examples, float/struct/vector benchmarks, DB components)
through both backends and requires identical stdout and exit status, with timeouts; any native
optimisation must pass it before being enabled.

## v0.9.2 — pair matcher accepts shared multiplier constants (2026-11-06)

The AoS pair matcher required the multiplier `Const` to sit inside every window; a duplicated body
that reuses the constant defined once (manual two-particle unroll, CSE'd code) failed to fuse the
second body. The matcher now accepts the constant either inside the 15-instruction window or defined
earlier (14-instruction window). A manually unrolled particles variant fuses both bodies and runs
~143 ms versus ~147 ms; the win is small because the loop is bandwidth-bound. No default benchmark
moved. All gates pass, 168 tests, parity 60/60.

## v0.9.1 — per-body par thread caps (2026-11-06)

`par for` bodies that call user functions or write struct fields run with at most 8 pthreads; leaf
array bodies use up to 16 (still min with CPUs and iteration count). Raytrace par drops 34 → 22 ms
while fannkuch blocks stays at 8 threads and keeps its checksum; particles par is unchanged (~135 ms
at 8 threads, 16 was slower). The cap is passed as the fifth argument to the runtime. All gates
pass, 168 tests, parity 60/60.

## v0.9.0 — matmul interchange reaches the saxpy path (2026-11-06)

The lowerer recognises the canonical i-j-k square matrix multiply (three `while` loops over the same
constant bound, zero `i`, a scalar dot accumulation and `c[i,j] = s`) and rewrites it in place:
zero `c[0..n*n)`, then `i`/`k` loops whose inner statement is the original
`c[i,j] += a[i,k]*b[k,j]`, which the guarded saxpy recogniser turns into a 4-wide/eight-wide SSE2
loop. Integer addition order per output element is unchanged. Default matmul drops 74 → **9 ms**
against C's 12 ms; the i-k-j source stays ~10 ms. The transform requires local distinct i32 arrays,
pure indexes, the same loop bound and a known-zero `i`, and restores its binding snapshot before
returning. All gates pass, 168 tests, parity 60/60.

## v0.8.100 — refresh saxpy numbers (2026-11-06)

Matmul i-k-j with the eight-wide saxpy measures 9–11 ms across runs (C 12 ms); the docs now quote
10 ms instead of the earlier best-case 8 ms. No code change. All gates pass.

## v0.8.99 — eight-wide saxpy (2026-11-06)

The SSE2 saxpy loop processes two four-lane groups per iteration, halving the loop-control overhead;
the scalar tail is unchanged. Matmul i-k-j measures 9–11 ms (C 12 ms), checksums and the tail/negative
cases stay exact. All gates pass, 168 tests, parity 60/60.

## v0.8.98 — restore saxpy after the mutable-init fix (2026-11-06)

The v0.8.97 mutable-initialiser fix also skipped recording `j=0` for mutable loop variables, which
silently disabled the saxpy recogniser (matmul i-k-j regressed to 80 ms). Constant initial values are
now recorded for every declaration while mutable names stay out of the immutable-constant map;
matmul i-k-j is 11 ms again. All gates pass, 168 tests, parity 60/60.

## v0.8.97 — scoping/constant regressions closed (2026-11-06)

Two regressions from the scoping work, found by a third audit: mutable `var` initialisers were
recorded as immutable constants (so `n=5` after `var n=3` still unrolled/vectorised with 3), and the
fully-unrolled `for` path returned before popping its Binds snapshot, leaking a shadowed binding
into the enclosing block. Mutable declarations are no longer added to the constant map, assignments
invalidate it, and the unrolled path restores its scope. Repro u3,t1,t2,t3,u2 now match C; all 12
earlier audit cases still match. Matmul i-k-j remains ~11 ms. All gates pass, 168 tests, parity 60/60.

## v0.8.96 — guarded saxpy re-enabled after scoping fix (2026-11-06)

`for` bodies (fresh loop variable) and `arena` bodies now get the same binding snapshot/restore as
`if`/`while`, and every declaration allocates a fresh slot. With the scope bug gone, the guarded
saxpy recogniser is enabled again: local distinct i32 arrays, constant limit in `0..=i32::MAX`, a
path-sensitive `j=0`, pure/effect-free limit/multiplier/index expressions, 64-bit index bases, and
the audit repros g1,g2,g3,g13,g14,b1,b2,b5,b6,b7,alias_shift,s_min all match C. Matmul i-k-j is
8 ms against C's 12 ms; the matrix is back to four outright wins. All gates pass, 168 tests, repo
parity 60/60.

## v0.8.95 — lexical scoping for if/while and fresh declarations (2026-11-06)

Every `let`/array declaration now allocates a fresh slot instead of overwriting a same-named outer
binding, and `if`/`while` bodies are lowered inside a snapshot of the binding maps (vars, types,
arrays, vectors, structs, constants, shadow bookkeeping) that is restored on exit. This fixes the
pre-existing shadowing miscompile: the v0.8.94 audit repros g14/g2 now print the C-expected values
(inner `n=3` visible only inside the branch, outer loop bound 10). Loop-variable shadowing in `for`
and arena bodies still needs the same wrapping before saxpy can be re-enabled. All gates pass.

## v0.8.94 — saxpy stays off; lexical-scoping blocker identified (2026-11-06)

Second-round guard fixes (flow-sensitive constant initialisation, shadow-aware constant
collection, 64-bit index bases) exposed a deeper IR limitation: the builder stores variables in a
flat name map with no block scopes, so a shadowing declaration reuses the outer slot. That makes
both constant propagation and a safe saxpy guard impossible without a scoped environment. The idiom
remains disabled; matmul i-k-j is scalar (~80 ms). The next work item is proper lexical scoping in
the IR builder (push/pop name maps per block), which also fixes pre-existing shadowing semantics.
All gates pass.

## v0.8.93 — saxpy disabled after second audit (2026-11-06)

A second audit found three live miscompiles in the guarded version: `var_init_const` was
flow-insensitive (an assignment in one branch was assumed to hold on every path), `ast_consts` was a
flat name map without shadowing scope (`let n` inside a branch changed a later loop bound), and the
i32 coercion truncated i64 index bases instead of rejecting them. The idiom is disabled again; the
scalar loop is authoritative. Matmul i-k-j returns to ~77 ms and the v0.8.92 win is retracted. All
gates pass; the rewrite needs a scoped constant environment, path-sensitive start tracking and
i32-only index bases.

## v0.8.92 — guarded saxpy is back (2026-11-06)

The saxpy idiom is re-enabled under strict guards that close the six audit findings: only local
allocations (never array parameters, so no call-site aliasing), distinct c/b arrays, i32 arrays and
i32 multiplier, a constant limit in `0..=i32::MAX`, a `j` whose last constant initialisation is 0,
and pure/effect-free limit, multiplier and index-remainder expressions (no calls, loads or array
reads, so hoisting them once is sound). Length references are rewritten for both arrays, the start
is 0 so no i64 arithmetic or zero-trip select is involved, and the panic line is the indexing
statement. `scalarize_const_arrays` is disabled because the audit showed it could free arrays still
referenced by `SaxpyI32`; the pass remains for a later fix. Audit repros (b1/b2/b5/b6/b7,
alias_shift, s_min) now match C. Matmul i-k-j is 8 ms against C's 12 ms; all gates pass, 168 tests,
repo parity 60/60.

## v0.8.91 — roll back the saxpy idiom after audit (2026-11-06)

The `SaxpyI32` idiom shipped in v0.8.90 was disabled after an adversarial audit produced six
verified miscompiles: (1) `scalarize_const_arrays` did not see `SaxpyI32` pointers and freed the
arrays, (2) constant array-length argument removal did not rewrite `c_len`/`b_len`, (3) `limit`,
`av` and index remainders were hoisted although the source re-evaluates them, (4) the emitter
truncated i64 `j`/`limit`/`count` with `movslq`, (5) shifted aliasing between `c` and `b` was
ignored, and (6) the bounds checks ran even for zero-trip loops. The lowerer now uses the scalar
loop; the instruction, emitter and tests stay for a guarded rewrite (no reads of removed arrays,
length rewriting, purity/invariance checks, i64 handling, alias rejection, zero-trip checks).
Matmul i-k-j returns to ~77 ms, so the matrix is back to three outright wins; the v0.8.90 speed
claims are retracted. All gates pass, 168 tests, repo parity 60/60.

## v0.8.90 — SSE2 saxpy at the IR idiom level (2026-11-06)

The lowerer recognises the canonical contiguous update
`while j < n { c[base+j] = c[base+j] + av * b[base2+j]; j = j + 1 }` (both indexes affine in `j`
with coefficient one, distinct `i32` arrays) and emits one `SaxpyI32` instruction instead of a
scalar loop. The instruction checks `0 <= c_idx`, `0 <= b_idx`, `count >= 0` and
`idx + count <= len` for both arrays once, guards the `limit - start` arithmetic against i32
overflow, then runs a 4-wide SSE2 loop (`pmuludq`-based i32 multiply, `paddd`) with scalar tail. The
loop variable is updated with a select so zero-trip loops still leave `j` unchanged. Matmul in i-k-j
order drops 78 → **10 ms** against C's 12 ms, a fourth outright win; the default i-j-k form is
unchanged at 72 ms (strided inner loop). Differential cases cover the normal path, a tail of two,
and an out-of-bounds count that must panic. All gates pass, 168 tests, repo parity 60/60.

## v0.8.89 — scalar-replace small constant-index arrays (2026-11-06)

An array of at most 16 scalar elements whose every access uses a compile-time constant index, is
never passed to a call or otherwise referenced, and is not in an arena is now replaced by individual
scalar slots; the allocation and fill disappear. Initialisation is emitted at the `Fill` site (or as
an explicit zero constant when there is no fill), which a regression caught: placing the stores at
the `Alloc` site made them read the fill constant before its definition and printed zeros. Full
unrolling of nested constant loops was tried together with scalarisation but n-body stayed at
~890 ms (full unroll alone 1041 ms, leaf unroll 869–945 ms depending on machine load), so the
leaf-only unroll stays. No benchmark moved; this is a general small-array optimisation with its own
differential case. All gates pass, 168 tests, repo parity 60/60.

## v0.8.88 — refresh benchmark evidence and fix the compile harness (2026-11-06)

The compile-speed harness was still measuring the C backend while labelling it "MAGIC". It now
measures the default IR backend on the same 2,000-function source: cold 25 ms, one-function edit
10 ms, no-op 6 ms, versus gcc 47/–/–, rustc 109/110/– and Goose 69/64/69; binary 22,048 bytes versus
Rust 4,538,288 and Goose 72,024. `BENCHMARKS.md` and the README performance section were rewritten
from the current gate logs: three outright wins (sieve, fannkuch `par for`, raytracer `par for`),
hash `par for` inside 1.25x, and honest losses for matmul (6.2x), binary trees (5.4x), n-body (3.9x)
and particles (2.3x). Peak RSS is 7.2x lower than C on the bit-packed sieve and at parity elsewhere;
"lowest on most workloads" is not claimed. All gates pass.

## v0.8.87 — vectorise checked struct-array pairs (2026-11-06)

`Aos2Step` now carries the length reference and panic line, so the pair vectoriser also fires on
struct arrays and vectors whose bounds are not statically proven (runtime length). One bounds check
guards the pair and all eight field accesses share the same index and length; the fused instruction
carries the first non-zero source line so an out-of-range access still reports file:line. A
differential case pushes two elements, indexes five, and requires the panic. Vecpart drops 85 → 45 ms
(C ~17–26 ms, so still ~2x, but 1.9x faster than before); particles is unchanged at 147 ms and keeps
its checksum. All gates pass, 168 tests, repo parity 60/60.

## v0.8.86 — unroll small constant leaf loops (2026-11-06)

`for i in a..b` is fully unrolled while lowering when both bounds are compile-time constants, the trip
count is at most eight, the body is small and it contains no `break`/`continue`, no shadowing of the
loop variable and no nested loop. The unrolled copies substitute the loop variable through
`subst_stack` and `subst_consts`, so nested constant loops inside an unrolled body still evaluate
their own bounds. This is a leaf-loop unroll: loops containing another loop stay rolled, because the
fully expanded n-body nest was slower (990 ms) than the leaf-only form. n-body's loop variant drops
966 → 869 ms in the gate log; matmul, particles, hash, fannkuch and bt are unchanged. A differential
case covers unrolled constant indexes together with a `break` loop that must stay rolled. All gates
pass, 168 tests, repo parity 60/60.

## v0.8.85 — drop constant array-length arguments (2026-11-06)

The constant array lengths found in v0.8.84 are now removed from the ABI as well. Each function
records `param_vars` (IR argument position → local slot), so dropping an argument only needs to
remove it from the signature, every call site and the mapping — no global renumbering of locals. All
`LenRef::Var` uses in the callee are rewritten to the constant before the argument disappears. Binary
trees drops 4449 → 3888 ms (the count variant 1901 → 1494 ms) because the recursive build/check calls
now pass four arguments instead of seven and no longer spill one to the stack; the output is
unchanged (568328208). Fannkuch is unaffected (its remaining checks depend on indexes, not length
only). All gates pass, 168 tests, repo parity 60/60.

## v0.8.84 — interprocedural constant array lengths (2026-11-06)

Function metadata now records which IR arguments are array-length parameters. When every call site
passes the same constant length (recursive calls that pass the same length parameter through are
allowed), the callee's length-parameter loads are replaced by that constant before the bounds prover
runs. A recursive differential case covers the transformation. No benchmark moved measurably: binary
trees stays ~4.5 s versus C 704 ms because its cost is dominated by the recursive call convention
(three array parameters expand to six pointer/length arguments, seven with the depth), and the
fannkuch functions keep checks whose indexes are not provable from the length alone. The metadata is
the groundwork for the next step: dropping the now-unused length arguments from signatures and call
sites. All gates pass, 168 tests, repo parity 60/60.

## v0.8.83 — one fused SIMD step (2026-11-06)

The pair add and pair multiply for `x += vx; y += vy; vx *= k; vy *= k` are now one `Aos2Step`
instruction: the velocity pair is loaded once, both `addpd`/`mulpd` complete before either store, and
the multiplier is broadcast in place. This removes one 16-byte load and improves memory-level
parallelism. The small differential program and the particles checksum are unchanged; particles
measures 140 ms (C 70 ms) and par 134 ms, so the loop is now bandwidth-bound rather than
instruction-bound. All gates pass, 168 tests, repo parity 60/60.

## v0.8.82 — SSE2 pair operations for struct-array updates (2026-11-06)

A new pass recognises `x += vx; y += vy; vx *= k; vy *= k` on an array of structs and replaces the
four scalar statements with two SSE2 pair operations (`movupd`/`addpd`/`mulpd`). It only fires when
every access has a proven bounds check (`line == 0`) and every intermediate value has a single use,
so it is a pure replacement. The pass initially swallowed the `Const` that defines the multiplier,
which made the broadcast zero; the constant is now preserved and a differential case covers the exact
shape. Particles runs 151 → 139 ms (265 ms two rounds ago; C is 70 ms), par 135 ms, checksum
unchanged. All gates pass, 168 tests, repo parity 60/60.

## v0.8.81 — leaner struct-array field access (2026-11-06)

Four fixes to the address path for struct arrays: constant array lengths are resolved *before* the
bounds prover (so `IndexField` accesses inside canonical loops are actually marked unchecked),
field-array pointers are ranked for promotion like `IndexLoad`/`IndexStore` pointers, repeated loads
of the same variable in one block are CSE'd, and the emitters no longer load the length or
sign-extend the index for an access whose bounds are already proven. The particles hot loop now has
one `shl` for the base address and no bounds checks; particles drops 265 → 151 ms (C is 70 ms), par
135 ms. A latent slow-path bug surfaced in the native gate: an unchecked field access still
referenced `.Lmsg0` (undefined symbol) in the slow emitter; that path now guards on `line != 0` and
the native gate is green again. All gates pass, 168 tests, repo parity 60/60.

## v0.8.80 — struct arrays in `par for`, and three outright wins (2026-11-06)

`par for` bodies can now read and write struct-array fields (`ps[i].x = ...`): the collector accepts
`IndexField`/`IndexFieldAssign`, the child builder gets the struct definitions, and captured struct
arrays keep their element layout. A particles variant using `par for` over the integration loop is
included in the benchmark suite (`particles MAGIC par`) and runs 265 → 137 ms (C is 70 ms; eight
chunks are memory-bandwidth-bound), with the same checksum.

Current outright wins against the portable C/C++/Rust baselines in the gate log: **sieve** 377 ms vs
C 431, **raytrace par** 24 ms vs C 37, **fannkuch blocks** 66 ms vs C 165 / Rust 159. Hash par is
31 ms vs C 29 (1.07x, within 1.25x). The default (single-thread) matrix still trails: particles
265 ms, matmul 76 ms, bt 4.4 s; SIMD for AoS float pairs and stronger tree/hash inlining are the next
levers. The 16-thread experiment remains rejected (fannkuch blocks segfaults); the audited cap is 8.
All gates pass, 168 tests, repo parity 60/60.

## v0.8.79 — branch-fast signed division by powers of two (2026-11-06)

`/` and `%` by a positive power-of-two constant now become `FastDivPow2`, a small instruction with a
non-negative fast path (`and mask` / `sar`) and an exact negative path (`x & mask`, then subtract the
divisor when the remainder is non-zero; division adds `d-1` before the arithmetic shift). This is
exact for the full i64 range, including `i64::MAX`, negative operands and divisors up to 2^62; the
differential case covers every sign around `-20..20`, `% 4/8/16`, `i64::MAX` and a 2^40 divisor.
Hash probing drops from ~44 ms to ~42 ms (C ~29 ms) because the probe is dominated by random memory
access and bounds checks, not the division; hash par is 31 ms. Raytrace par is 24 ms versus C 37 ms
(an outright win), sieve remains 377 ms versus C 431 ms.

A 16-thread cap was tried and **rejected**: raytrace improves to ~21 ms, but fannkuch blocks
segfaults deterministically with 16 chunks, so the audited cap of 8 stays. All gates pass, 168
tests, repo parity 60/60.

## v0.8.78 — power-of-two division, a closed short-circuit bounds hole (2026-11-06)

`/` and `%` by a positive power-of-two constant are now lowered to the sign-correction + shift
sequence (`sar`/`shl`) instead of a hardware `idiv`; hash probes no longer contain a division
instruction. A range-based bounds pass was prototyped and **rejected after audit**: Vera Ds. Flint
found four silent out-of-bounds repros (seeds from one loop reused globally, seed overwrite, nested
loop re-entry, and short-circuit guards), so the pass was removed rather than shipped with holes.

The audit also exposed a pre-existing hole in the hardened loop pass: with a short-circuit guard
(`while i < 10 || spin < 100`) the false edge can reach the body without re-entering the header, so
the `i < 10` fact was wrongly applied there. The pass now rejects a loop when the header's exit edge
can reach any region block without passing the header. `u16` went from a silent OOB heap write
(`malloc(): corrupted top size`, no panic line) to `line 1: index out of bounds`, exit 1, matching
C. New differential cases cover non-canonical steps (`i += 5`), negative initial values and `<=`
bounds. All gates pass, 168 tests, repo parity 60/60.

## v0.8.77 — four-byte i32 variables (2026-11-06)

Named i32 variables are now stored as four bytes (`movl`) and loaded with a sign-extending
`movslq`, instead of keeping a canonical 64-bit copy with a wrap instruction after every
assignment. A new pass drops the `Trunc i32` node when the truncated value is consumed by exactly
one store into an i32 variable: the low 32 bits of the 64-bit computation are already the wrapped
value, so the store can write those four bytes and the load reconstructs the sign. Unnamed slots
(array pointer/length and vector descriptors) are typed `i64` in the function metadata so they are
never compressed by mistake. Medians: matmul 80 → 71 ms, hashmap 80 → 77 ms, btree 77 → 74 ms,
sqlscan 18 → 17 ms, fannkuch buf ~193 → 165 ms; float benchmarks are unchanged. A differential case
covers negative values, i32::MAX wrap, mixed i64 and array stores. All gates pass, 168 tests, repo
parity 60/60.

## v0.8.76 — bounds-check elimination restored, audited and hardened (2026-11-06)

Since the i32 wrap fix every arithmetic op is followed by a `Trunc`, which made the bounds prover
reject every induction variable and affine index; matmul kept two bounds checks in its inner loop.
The prover now looks through those truncations (`strip_trunc`) for increment recognition and affine
analysis, which removes the checks and produces large gains. Because this is security-critical, Vera
Ds. Flint ran three adversarial audits and found seven real defects in the first and intermediate
versions; the final rules are:

* the loop guard must be the Br condition's own `Bin Lt` (an `||` guard is rejected);
* the header must dominate every region block;
* exactly one canonical `iv+1` store may exist in the region;
* a fact is valid in a block only if the block cannot be reached from the increment without
  re-entering the header (`after_inc`); the increment block gets the fact only before the store and
  only when it is not itself in `after_inc`;
* initialisation constants are never Trunc-stripped (so `2147483648 as i32` is not read as +2^31);
* loop bounds, array lengths and affine maxima must stay within i32, and affine expressions with
  subtraction/negation are rejected.

Eight differential regressions encode the auditor's counterexamples (mutation before access, affine
mutation, increment in an inner loop, `||` guard, Trunc-negative init, access after the increment,
nested re-entry, increment-block self-reachability). All gates pass; repository parity is 60/60.

Performance on the same machine (3-run medians, IR, default backend): matmul 166 → ~80 ms,
nbody ~2000 → 847 ms, particles 595 → 261 ms, raytrace 280 → 110 ms, vecpart 160 → 72 ms,
btree 175 → 77 ms, hashmap 108 → 80 ms, sqlscan 34 → 18 ms, bufpool 194 → 77 ms. Against pinned
portable C/C++/Rust measurements (Meter Ds. Vex): MAGIC now **wins sieve outright** (377 ms versus
431/432/437, 0.88x) and is 7.1x lower in peak memory there (14.0 vs 99.4 MB); hash is 1.71x and
fannkuch 1.73x off the best baseline, raytrace 3.36x, nbody 3.87x, particles 3.96x, bt 6.21x,
matmul 6.23x. The speed goal (within 1.25x plus three wins) is not yet met: sieve is the only win so
far; SIMD/width-aware codegen is the next lever.

## v0.8.75 — CPU-scaled par runtime and creation fallback (2026-11-06)

The parallel runtime now asks `sysconf(_SC_NPROCESSORS_ONLN)` for the CPU count, uses
`min(iterations, CPUs, 8)` threads, and spawns every chunk before joining so chunks actually overlap.
Two robustness fixes came out of stress tests: `pthread_create` failures are detected (the chunk runs
inline instead of joining an uninitialised id, fixing intermittent crashes at high thread counts), and
`pthread_join` is skipped for zero ids. Raytrace par improves from ~45 ms to ~27 ms on this machine
(C/OpenMP ~13 ms); fannkuch streams run eight-wide and keep their checksums (`73196`, `38`). The
sieve mark loop remains dominated by one large prime (`~265 ms` versus ~183 ms sequential IR and
~100 ms C/OpenMP); constant-length capture and dynamic scheduling are the next par steps. All gates
pass, 168 tests, repo parity 60/60.

## v0.8.74 — real pthread `par for` in the IR (2026-11-06)

`par for` now runs on four pthreads instead of falling back to a sequential loop. The lowerer
synthesizes a body function with a context block holding captured arrays (pointer + length) and
read-only scalars; the runtime `.Lpar_for` splits the range into up to four chunks, spawns all of them
and joins. Array writes are visible to the caller; writes to captured scalars, nested `par for`,
vectors/structs captures and `break`/`continue` inside the body are rejected with explicit errors.
Function calls (user functions and builtins) are allowed; nested `print`/`print_assert`-style calls
are lowered as statements. Bool arrays captured by a par loop use a byte representation (like the C
backend's par mode) instead of bit-packing, because concurrent bit updates in one word would race;
this keeps the sieve-style mark loops correct (`5761455`) and deterministic. A `lock or`/`lock and`
atomic bit store is also available for bit-packed bool stores in parallel bodies.

The build cache now includes the compiler binary's own hash, so rebuilding `magicc` invalidates stale
executables (a real trap found while debugging: an old IR binary hid a layout fix), and the C-path
stamp keeps the compiler command as its last field so tooling that reads it still works. Differential
cases cover a par integer fill with a captured scalar and a par bool sieve; repository parity stays
60/60 and all gates pass. Performance note: par speedup is currently limited by inner-loop bounds
checks and the static chunk split (sieve par IR ~260 ms versus ~180 ms sequential and ~95 ms C
OpenMP); the next step is constant-length capture plus dynamic scheduling.

## v0.8.73 — the IR backend becomes the default (2026-11-06)

`magicc build app.magic` now uses the typed-IR native backend by default: GNU assembler plus `ld`,
no C compiler in the build path, stack traces included. `--backend native` selects the older AST
emitter and `--backend c` keeps the C11 transpiler for portability and differential testing. The
native/IR path gained its own incremental cache (compiler version + source hash + backend/tool
identity + executable hash, so tampered or stale binaries rebuild and up-to-date runs skip the work),
and diagnostics from the IR path are mapped to the original file:line through the same import-segment
mapper as the C backend. `check` and `emit` follow the chosen backend (`emit` prints IR/assembly, or C
with `--backend c`; a long-standing bug where `emit --backend native` printed C is fixed). Flags that
only make sense for the C path (`--unroll`, `--dev`, `--cc`, gcc `--native`) are rejected instead of
being silently ignored. C-specific CLI tests are now explicit about `--backend c`; three new tests
cover the default IR build/run, assembly emission and symlinked-assembly refusal. The benchmark
scripts that pass `--cc` (clang/gcc `-march=native` variants, the compile-speed harness and the DB
comparison) now also pass `--backend c`, since `--cc` is only meaningful on the C path. 168 tests.

## v0.8.72 — final IR parity pass (2026-11-06)

The remaining C-only language surface is now covered by the IR: `env` (`getenv` wrapper), string
return values, `len(array)` for fixed arrays, pushing a struct variable into a vector (copy through a
temporary pointer), struct-array fill with a non-zero literal or struct variable (runtime copy loop),
and `min`/`max` for integers and `f64` via a new `Select` instruction (comparison plus `cmovne` for
integers and an SSE select for floats; NaN picks the second operand, matching the C backend's ternary).
A peephole interaction was fixed at the same time: the slow `Select` path compared a memory operand,
which the `cmp $0,m` → `test m,m` peephole turned into an invalid memory-memory `test`; the condition is
now loaded into a register first.

A real collision bug surfaced when two functions used string literals: `Str` ids are per function but
labels were global, so two `.Lstr0` definitions were emitted. Labels are now namespaced
(`.Lstr_<func>_<id>`) and de-duplicated in `.rodata`. New differential cases cover min/max (ints and
floats), len(array), string returns, env, struct-vector push and struct-array fill; repo parity stays
60/60. Known remaining difference: `par for` is still sequential in the IR (the C backend uses
OpenMP), and `file_pread`'s 4-argument form is rejected by both backends.

## v0.8.71 — struct literals in expression position and struct returns (2026-11-06)

Struct literals now lower anywhere an expression is allowed: the IR allocates a temporary slot block,
runs `StructInit`, and yields a pointer value. Functions returning a struct use a hidden `sret` pointer
appended as the last parameter; the callee copies the result slots into it, the caller allocates a
temporary, and `let` copies the returned slots into the named struct variable. Early returns, returning
a struct variable, struct literals as arguments, and nested calls (`sum(make(10))`) all match C.

Two bugs were found while testing: the AST inliner treated `ParamTy::Scalar(Ty::Struct)` as inlineable,
so `ident(P {7,8})` resolved `p` to the caller's variable and printed 12 instead of 56; struct-typed
parameters are now excluded from inlining, and substitution takes precedence over struct/vector
lookup for `Var` (which also protects future inlined struct parameters). Two differential cases cover
the expression forms and a multi-slot (`i64,i32,i32,i64`) struct; repo parity stays 60/60. 165 tests.

## v0.8.70 — `break`/`continue` in the IR (2026-11-06)

The IR lowering keeps a loop-context stack: `break` jumps to the loop exit and `continue` to the
condition block (`while`) or to a dedicated increment block (`for`, created only when the body
contains a `continue` so the normal path keeps its inline increment). Statements after a break or
continue go into a fresh unreachable block, keeping each block's terminator last. Nested loops target
their own context. A differential case covers `while` break/continue, `for` continue-then-break and a
nested inner-loop break; C and IR agree on `7 / 9 / 19 / 10`. Repo parity stays 60/60. Remaining IR
gaps before the default-backend switch: struct-return functions and struct literals in expression
position.

## v0.8.69 — IR parity: bit-packed bool parameters and `net_read_at` (2026-11-06)

Bit-packed `[bool]` parameters now lower in the IR (pointer + length, same representation the IR
already used for local bool arrays), and `net_read_at`/`net_write_at` are supported with explicit
range checks (offset/length non-negative, offset ≤ capacity, length ≤ capacity − offset) before the
runtime performs `read`/`write` at `buf + off`. The existing IR TCP test now uses the `_at` forms for
both directions, and a differential case covers the bool-array parameter. A scripted parity audit
compiles every tracked `.magic` program through both the C and IR backends: **60/60 now compile with
IR**, including `pgserver_magic.magic`. Remaining known IR gaps before switching the default backend:
`break`/`continue`, struct-return functions and struct-literal expressions outside `let`/`push`.

## v0.8.68 — stack traces in the C backend too (2026-11-06)

The C backend's panic helper now takes a backtrace and symbolizes it with `dladdr`, retrying the
address minus one when the return address sits exactly at the end of a function (GCC places a
never-returning panic call last, which made `main` invisible before). The generated C defines
`_GNU_SOURCE`, includes `execinfo.h`/`dlfcn.h` under Linux, the compiler flags keep unwind tables, and
the executable is linked with `-rdynamic`. A nested panic now prints e.g.
`main+0x59` under the `line N: ...` message. A Rust regression compiles a runtime-dependent panic,
asserts exit 1, the message and a symbolized `main+` frame. Aggressive `-O3` inlining may omit
inlined MAGIC frames (same as C debugging); frames that survive optimization are named. 165 tests.

## v0.8.67 — file:line plus stack traces for IR panics (2026-11-06)

Every IR function now carries DWARF CFI (`.cfi_startproc`, CFA/`%rbp` rules, epilogue CFA restore),
the panic routine has its own FDE, and the linker emits `.eh_frame_hdr` and exports dynamic symbols
(`--eh-frame-hdr -E`). At a panic the runtime prints the original `line N: ...` message and then a
symbolized backtrace, e.g. `inner`, `middle`, `main`, libc and `_start`. Two bugs were found while
bringing it up: `backtrace_symbols_fd` arguments were passed in the wrong order (segfault), and
without `.eh_frame_hdr` the unwinder stopped after the panic frame. A Rust regression builds a nested
panic program, asserts exit 1, empty stdout, the message and all three MAGIC frames. 164 tests; matmul
IR binary is 16.9 KB before vs 17.2 KB after the export/unwind tables, still well under the 100 KB
low-end budget.

## v0.8.66 — the Ds. agent crew charter (2026-11-06)

`TEAM.md` gives the standing subagents durable identities — every member is DeepSeek and carries
`Ds.`: James Ds. Pal (Bug Eliminator Taskforce), Vera Ds. Flint (Soundness Inquisitor), Meter Ds. Vex
(Benchmark Evidence Bureau), Serene Ds. Porcelain (Security Department), Rhea Ds. Anvil (Codegen
Architect), Nova Ds. Sparrow (Memory Economy Unit), Kai Ds. Lumen (Diagnostics & DX), Sable Ds. Torque
(Low-End Hardware Lab), Iris Ds. Prism (High-End Scaling) and Orin Ds. Quill (Release Gate). The
charter fixes the evidence contract (repro / file:line / measured median before agreement), makes
adversarial review the default, and defines default pairings for codegen, performance, DX and
security work. v0.8.65 shipped with all three Vera-found defects fixed and six differential
regressions in place.

## v0.8.65 — narrowed i32 index chains (2026-11-06)

i32 arithmetic that feeds only array indices is now emitted with 32-bit instructions
(`addl/subl/imull/andl/orl`) and the redundant `movslq` wrap checks in the chain are removed; index
use sites sign-extend once. A subagent measured the change against a read-only build of v0.8.64:
matmul 186 → 166 ms (-10.8%), nbody -6.2%, particles/btree/hashmap/sieve unchanged within noise, and
C-vs-IR stdout was identical on every benchmark.

The same subagent's adversarial pass found three real soundness bugs before release. (1) Untyped
`i64` chains were narrowed, so `a[y + 2]` with `y: i64 = 4294967296` wrapped into a valid index instead
of trapping. (2) A `Trunc I32` was substituted even when its operand was `i64` (e.g. `a[(4294967301)
as i32]`), dropping the wrap and turning a bounds trap into an out-of-bounds store. (3) The marking
rule accepted any consumer in the same block, so the IR inliner could mix an index chain with a
comparison or float cast (`f(n * 2)` using `x + 100` and `x < 5`) and compare the zero-extended low
half. The pass now requires the operand of a substituted `Trunc` to be `i32`, allows a use only when
its consumer is already marked (no same-block shortcut), and refuses non-`i32` roots. A re-audit then found a medium defect in the substitution resolver: chains were followed for at most
64 hops, so an index with 65+ nested `as i32` casts could reference a removed `Trunc` and read an
unwritten slot (C printed 7, IR printed 42; 70 casts segfaulted). The chain is now resolved to a
fixpoint (substitution strictly decreases value ids, so it is acyclic). Six differential regressions
cover the i64 index, an overflowing i32 index, an i64-to-i32 cast index, the two inliner mixed-use
cases, and a 70-cast index chain; C and IR agree on stdout and exit for all of them.

## v0.8.64 — growable-array parameters in the IR (2026-11-06)

Vector parameters now work in the IR with the same reference semantics as the C backend: a vector
parameter is passed as three pointers (`ptr**`, `len*`, `cap*`), the callee copies the descriptor at
entry, operates on local `ptr/len/cap` variables for `v[i]`, `len(v)`, `push`, `pop`, `clear`,
`reserve`, and writes the descriptor back before every return (including implicit returns) so a push
inside a callee is visible to the caller and across nested calls. Forwarding a vector parameter to
another function emits `v, v_len, v_cap` instead of the local `&v, &v_len, &v_cap` forms, which also
fixes a C-backend compile error for nested forwarding.

Two real bugs were caught by the new tests: address-taken vector variables were still register-
promoted (the callee read stale stack slots, causing a segfault), and descriptor write-back was
missing when a function ended without an explicit `return`. Both fixed. Three differential cases cover
read-only use, push from `main`, and nested forwarding. The AST native backend still rejects vector
parameters explicitly. All gates green.

## v0.8.63 — struct parameters in the IR backend (2026-11-06)

Functions taking structs by value now work in the IR: the caller takes the address of its own struct
slots (new `AddrVar`), passes that pointer as the (single) parameter, and the callee copies the slots
into a local struct variable at entry (new `CopySlots`), preserving by-value semantics (MAGIC struct
parameters are read-only inside the callee). Scalar layouts with i32/i64/f64 fields and nested calls
work; two differential cases cover an integer struct with three fields and a float pair. The slot copy
walks the stack layout in the correct descending direction (`-8*i(%rax)`), which the differential test
caught immediately. Remaining IR parameter gap: growable-array (vector) parameters. All gates green.

## v0.8.62 — `net_*` TCP builtins and `flush` in the IR backend (2026-11-06)

The IR lowering now maps `net_listen`, `net_port`, `net_accept`, `net_connect`, `net_read`,
`net_write`, `net_close` and `flush` to new runtime helpers that mirror the native backend's inline
socket sequences (socket/bind/listen/accept/connect/getsockname/read/write/close, byte-swapped
`sockaddr_in`). A new end-to-end test builds both a server and a client with `--backend ir`, runs the
loopback roundtrip (`connect` → `write` → `read` → `write`) and checks the echoed byte and counts.
Unknown `net_*` names still fail compilation instead of silently falling back. 163 tests; all gates
green.

## v0.8.61 — constant array lengths as bounds immediates (2026-11-06)

Constant propagation already proves many array lengths; a new pass rewrites those `LenRef::Var`
operands to `LenRef::Const`, and the register-allocated emitter now compares the index directly
against the immediate (`cmp $n, %rax`) instead of loading the length into `%rcx` first. Matmul moves
186 → 183 ms; the other benchmarks are unchanged within noise (btree 166, nbody 1986, particles 604,
sieve 87). Small but safe, and the IR corpus plus the differential gate pass.

## v0.8.60 — loop-invariant hoisting attempt caught by the gate (2026-11-06)

A preheader LICM pass (hoisting constants, loads of variables the loop never stores, and pure
arithmetic over them, innermost loops first, with a rebuilt definition map per loop) compiled but
produced wrong indices: matmul and sieve panicked with `index out of bounds` and the IR differential
gate flagged them immediately (`exit C=0 IR=1`). The bug survived a first fix (stale definition map
across nested loops), which means the remaining issue is in how moved values are consumed across
blocks (slot/register invalidation), not in loop selection. The pass was reverted in full; v0.8.59's
codegen is unchanged and green. Next attempt should hoist through an explicit block-splitting pass
with a fresh value-versioning step and a per-loop differential test before running the matrix.

## v0.8.59 — expanded differential fuzz corpus (2026-11-06)

The mutation fuzzer now also exercises signed division/remainder in all sign combinations, `u8`/
`i32`/`bool` casts, integer struct fields, 64-bit accumulators and negative loop starts with bounds
checks. Two test-harness fixes were needed to keep the differential comparison meaningful:

* crash *mode* is normalised (a trap and a guarded panic both count as failure; one crashing while the
  other succeeds is still a mismatch), and
* the C side is compiled at `-O0 -fwrapv`, because at `-O1` GCC folded the undefined `0 / 0` case to
  `0` while the IR correctly traps -- the mismatch was in the C compiler's UB exploitation, not in
  MAGIC.

With those changes the expanded corpus passes, including mutated division-by-zero cases.

## v0.8.58 — range-analysis deepening reverted (2026-11-06)

Extending the interval pass (array element ranges, accumulator bounds, constant-aware loop bounds,
truncation-stripping pattern matching) did not pay off: the loop-induction detection still failed on
most matmul loops because the wraps introduced by the correctness fix nest `Trunc` nodes around the
increment and initialisation stores, and the partial success only shuffled code layout (matmul 183 →
210 ms, nbody 1983 → 2155 ms, btree 166 → 168 ms; every benchmark equal or slower). The whole change
was reverted, keeping v0.8.57's codegen. The diagnosis is recorded for the next attempt: the robust
route is width-aware 32-bit code generation (emit `addl`/`imull` and sign-extend only at widening
barriers) rather than trying to delete `movslq` after the fact.

## v0.8.57 — loop-alignment sweep (2026-11-06)

Five-run medians for four alignment policies (loop headers at 16/32/64 bytes, and a hybrid with
64 bytes only for nested headers):

| Program | 16B | 32B | 64B | hybrid |
|---|---:|---:|---:|---:|
| matmul | 198 | 184 | 186 | 182 |
| nbody | 2127 | 2097 | 1983 | 2276 |
| particles | 540 | 577 | 602 | 555 |
| raytrace | 269 | 276 | 279 | 292 |
| btree | 172 | 169 | 166 | 168 |
| sieve | 95 | 87 | 88 | 92 |

No single policy wins everywhere: 64 bytes is best for nbody/btree but costs particles; 32 bytes
helps matmul/sieve; the hybrid helps matmul/particles but hurts nbody most. By aggregate time the
64-byte policy (already shipped in v0.8.56) remains the best choice, and the per-program sensitivity
is recorded here so future tuning is evidence-based. The tree keeps v0.8.56 codegen.

## v0.8.56 — array/accumulator ranges and 64-byte loop alignment (2026-11-06)

The interval analysis now also derives element ranges for scalar arrays whose stores and loads live
in separate loops (with call/aliasing/mixed-loop guards) and bounds accumulators of the form
`v = v + delta` in counted loops (`bound` iterations, checked arithmetic). These ranges let the pass
delete more `movslq` wrap checks. Separately, every loop header is now aligned to a 64-byte boundary;
matmul turned out to be extremely layout-sensitive (the same hot loop ran 180 vs 299 ms depending on
its alignment), and the stable alignment recovers it. Five-run medians: matmul 198 → 186 ms, nbody
2127 → 1983 ms, btree 172 → 166 ms, sieve 95 → 88 ms; particles/raytrace fluctuate within their usual
spread (≈600/280 ms). The IR differential gate and mutation fuzzer pass.

## v0.8.55 — post-correctness matrix and the cost of wrapping (2026-11-06)

Five-run IR medians after the interval-analysis pass:

| Program | IR (v0.8.55) | IR (v0.8.52) | AST | C |
|---|---:|---:|---:|---:|
| matmul | 198 ms | 125 ms | 221 ms | 31 ms |
| particles | 540 ms | 545 ms | 1013 ms | 153 ms |
| vecpart | 160 ms | 160 ms | 289 ms | 35 ms |
| raytrace | 269 ms | 270 ms | 484 ms | 91 ms |
| nbody | 2127 ms | 2243 ms | 4207 ms | 738 ms |
| sieve | 95 ms | 86 ms | 89 ms | 40 ms |
| btree | 172 ms | 154 ms | 167 ms | 103 ms |
| hashmap | 101 ms | 96 ms | 89 ms | 41 ms |
| sqlscan | 34 ms | 35 ms | 32 ms | 10 ms |
| bufpool | 194 ms | 178 ms | 178 ms | 130 ms |

The `i32` wrap fix costs 20–60% on integer-heavy programs because each arithmetic result is
sign-extended; the interval pass already removes the address/index truncations. Recovering the
remaining cost needs element-range analysis for arrays (matmul's `s = s + a*b`) and accumulator
bounds over counted loops, tracked as the next codegen milestone. The mutation fuzzer is now part of
the docs test count (162 tests).

## v0.8.54 — interval analysis removes redundant i32 truncations (2026-11-06)

The wrap-correctness fix cost matmul 2.4x because every `i32` operation ended with a `movslq`. A new
interval-analysis pass proves when a truncation cannot wrap and deletes it: loop induction variables
with a constant zero start, unit step and a constant (also constant-propagated) upper bound get
intervals, arithmetic over them uses checked 64-bit interval math, and the substitution rewrites uses
globally. Matmul recovers from 295 to 183 ms (the remaining gap to the 125 ms pre-fix figure is the
dot-product's own `s = s + a*b` truncations, which need the operand ranges of array loads); nbody,
btree, hashmap and sieve are unchanged. The full differential gate and the mutation fuzzer pass.

## v0.8.53 — real i32 wrap semantics and differential fuzzing (2026-11-06)

A differential fuzzer that mutates integer literals in valid programs (`tests/fuzz_diff.rs`) found
two real semantic bugs: the IR only *claimed* to truncate `i32` arithmetic but `truncate_value`
returned early when the value's type was already `i32`, so overflow never wrapped; and `let`/`
assignment used the initialiser's inferred type instead of the declared type, turning `var total: i64
= 0` into a 32-bit accumulator. Both are fixed with explicit narrow tracking and type coercion
(wraps, widening, float casts). The fuzzer now agrees on all cases, including `INT_MIN / -1`,
overflowing adds/multiplies and negative division. Honest cost: matmul 125 → 295 ms, btree 154 →
169 ms, hashmap 96 → 103 ms; float benchmarks are unchanged. Recovering that cost needs width-aware
32-bit code generation (emit `addl`/`imull` and keep values in 32 bits) instead of the current
64-bit-plus-`movslq` scheme; that is the next codegen milestone.

## v0.8.52 — post-refactor matrix measurement (2026-11-06)

Five-run medians after the per-block pool and division work:

| Program | IR | AST native | C |
|---|---:|---:|---:|
| matmul | 125 ms | 221 ms | 31 ms |
| particles | 545 ms | 1013 ms | 153 ms |
| vecpart | 160 ms | 289 ms | 35 ms |
| raytrace | 270 ms | 484 ms | 91 ms |
| nbody | 2243 ms | 4207 ms | 738 ms |
| sieve | 86 ms | 89 ms | 40 ms |
| btree | 154 ms | 167 ms | 103 ms |
| hashmap | 96 ms | 89 ms | 41 ms |
| sqlscan | 35 ms | 32 ms | 10 ms |
| bufpool | 178 ms | 178 ms | 130 ms |

The IR backend now wins seven of ten programs against the AST backend (up to 1.9x on particles and
nbody), and is within 1.4–4.6x of the C backend depending on the program. Peak memory remains at
parity with C. These numbers replace the v0.8.31 table in `NATIVE-BACKEND.md` and set the baseline for
the SLP/SIMD, `par for` and global-allocation milestones.

## v0.8.51 — per-block register pools and fast division (2026-11-06)

The register pool is now chosen per basic block: all six caller-saved registers by default, minus
`%rsi`/`%rdi` in blocks with bit-packed boolean accesses and minus `%rdx` in blocks with division. The
five callee-saved registers are always saved in the prologue, so the registers not occupied by
promoted variables become extra temporaries (up to eleven GPRs in simple blocks). `Div`/`Rem` moved
out of the slot emitter into the register path (`cqo; idiv`, zero-divisor panic, result via
`%rax`/`%rdx`). The full differential gate passes; btree improves 178 → 158 ms and hashmap 108 → 96 ms
(−11% each). Other benchmarks stay within the historical run-to-run spread.

## v0.8.50 — division fast-path attempt reverted (2026-11-06)

Moving `Div`/`Rem` out of the slot emitter and into the register-allocated path (with a per-block
pool that avoids `%rdx`) showed promise on btree (178 → 156 ms) and hashmap (108 → 98 ms), but the
required restructuring of the pool/liveness setup left the emitter in a state where the differential
corpus miscompiled, and the fix would need a larger refactor of the block emitter. The experiment was
reverted in full; v0.8.49's codegen is unchanged and green. The measurements are kept here so the
next attempt (proper per-block pool construction) starts from evidence rather than a guess.

## v0.8.49 — loop-depth-weighted variable promotion in the IR (2026-11-06)

Variables are now ranked for the five callee-saved registers by their use count weighted with the
approximate loop depth of the block (`1 + depth²`, depth from backward edges). Inner-loop counters,
accumulators and array pointers therefore outrank equally-used outer variables; for example matmul's
`k`/`s` now live in registers. Medians: particles 593 → 539 ms, raytrace 267 → 250 ms, nbody ~2.07 →
2.01 s; matmul, vecpart and sieve are unchanged and btree moves within noise (179→184 ms).

## v0.8.48 — layout experiments rejected, 1.0 roadmap recorded (2026-11-06)

Two code-layout changes were prototyped and measured: conditional-branch fallthrough for adjacent
blocks and alignment limited to back-edge targets. Fallthrough regressed matmul (117 → 131 ms) and
nbody (2.07 → 2.29 s); selective alignment alone was also neutral-to-negative (matmul 123 ms, nbody
2.14 s) despite helping btree slightly. Both were reverted, keeping v0.8.47's codegen. The new
`ROADMAP-1.0.md` records the verified state, the measured speed/memory ratios, why the absolute
3x/5x targets are unreachable by codegen on this matrix, and the milestones (SLP/SIMD, pthread
`par for`, function-wide allocation, IR default, memory policies) with an acceptance proposal.

## v0.8.47 — real arena regions in the IR (2026-11-06)

`arena { ... }` blocks now use a region allocator: each array inside the block is an anonymous
`mmap` (lazily zeroed pages), registered in a per-process table, and the region is `munmap`ed when
the block ends (with a 4096-allocation guard). Sequential arena regions therefore never accumulate
and untouched pages never become resident. Measured on two 32 MB regions: IR 32.9 MB peak versus C's
32.8 MB (parity) and the AST native backend's 64.2 MB (2x better than AST); a region whose pages are
never touched stays at the 1.6 MB runtime floor. An earlier bump-allocator version was rejected
because `realloc` + explicit zeroing made glibc raise its mmap threshold and touch every page
(33 MB peak), while `mmap` keeps the lazy-zero advantage. Arena examples still match the C backend
through the full differential gate.

## v0.8.46 — allocation hardening in the IR runtime (2026-11-06)

Every IR heap allocation now checks for failure and for size overflow: `calloc` and `malloc`
(NULL → `out of memory`, exit 1), `realloc` in `reserve`/`push` (both NULL and overflowing
`count * element_size`), and the vector growth paths guard the doubled capacity against
`i64::MAX / element_size` before multiplying. An `oom_alloc` differential case (a 2^60-element array)
now panics with exit 1 on both backends instead of dereferencing NULL.

## v0.8.45 — IR semantics fixes caught by a new differential gate (2026-11-06)

`verify-ir.sh` builds every deterministic example, benchmark and database program with the C and IR
backends, runs both and requires identical stdout and exit status; it is now part of `verify.sh`.
It immediately caught two real IR miscompiles:

* `i32` arithmetic was performed in 64 bits without wrapping. Literals now carry a width, arithmetic
  results are truncated to `i32` (with sign extension), and assignments, returns, casts and array
  stores preserve C's 32-bit wrap semantics.
* `constant - register` was compiled as `register - constant` when the left operand was an immediate.
  This broke every `20 - d`-style expression and made `bench-bt/bt_magic` print 44826549 instead of
  568328208.

Both are fixed: the full gate (36 programs, including binary-trees, nbody, particles, raytrace,
vecpart and all database programs) passes. Performance is unchanged within noise.

## v0.8.44 — trivial-function inlining in the IR (2026-11-06)

Functions whose body is a single `return` over scalar parameters (arithmetic, comparisons, casts,
no calls or memory access) are inlined at IR call sites by substituting the argument values into the
expression, which also lets CSE and branch fusion see through them. SQL scanning drops 41 → 34 ms
(−20%), bufpool 186 → 173 ms; btree, hashmap, matmul and sieve are unchanged. The differential
corpus passes, including the float and struct cases.

## v0.8.43 — SSE2 array fills in the IR (2026-11-06)

Non-zero array initialisers now use packed 16-byte stores: four `i32` or `u32` elements are
broadcast with `pshufd` and written with one `movups`, and two 8-byte elements are replicated with
`punpcklqdq` and written two per iteration; a scalar tail handles the remainder. A 20M-element
`[1; n]` fill drops 35 → 32 ms and the array sum 58 → 54 ms. Byte fills keep the scalar unrolled
loop, and boolean fills still use `memset`.

## v0.8.42 — soundness fixes in bounds-check elimination (2026-11-06)

Two security issues in the IR's bounds elimination are fixed. First, the proof only established an
upper bound, but the emitted check is unsigned: an induction variable that could start negative (for
example `var i = -2; while i < n`) would have had its check removed and then indexed out of bounds.
Loops are now only trusted when every path from entry reaches the header through a store of a
non-negative constant, and every later store is a constant non-negative step (`i = i + c`, `c >= 0`);
`negative_idx` joins the differential corpus and must panic exactly like the C backend. Second, the
affine arithmetic (coefficient multiplication, constant folding, maximum computation) now uses
checked operations and gives up the proof on overflow; a `huge_affine` test exercises an
overflowing index. The tightened rules keep the previous performance (vecpart 157 ms,
particles 593 ms, matmul ~118 ms).

## v0.8.41 — scaled-index cache and displacement addressing in the IR (2026-11-06)

`IndexField` accesses inside a block now share a cached `index * struct_size` register (up to four
entries) and use the field offset as an x86 displacement, so a struct update no longer recomputes the
scale for every field. A bug where float struct stores took the source through the integer operand
path (producing garbage) was caught by the extended float corpus and fixed before release. Medians:
particles 735 → 605 ms, raytrace 277 → 257 ms, vecpart 170 → 158 ms; matmul, sieve, btree and nbody
are unchanged (nbody A/B confirmed identical timing at ~2.07 s).

## v0.8.40 — local value numbering in the IR (2026-11-06)

A new CSE pass merges duplicate constants, variable loads and pure arithmetic inside each basic
block; values used outside their block are never merged, and variable loads are invalidated by
stores. This removes the repeated `LoadVar i` copies and repeated address arithmetic that dominated
the float loops. Medians after the type-aware fix: raytrace 363 → 277 ms (−24%), matmul 153 → 118 ms
(−23%), nbody 2634 → 1925 ms (−27%), vecpart 191 → 170 ms, particles 763 → 735 ms, btree 184 →
177 ms. All differential
tests and benchmark checksums match the C backend. Two bugs were caught by the differential tests before
release: the first version forgot to rewrite the `Alloc` size operand, and it merged int and f64
constants with identical bit patterns (e.g. `0` and `0.0`), whose slots are only written on demand;
keys now include the value type and the float benchmarks (raytrace, particles, vecpart, nbody) were
added to the IR corpus.

## v0.8.39 — parser recursion limits and a fuzz harness (2026-11-06)

A deterministic fuzz test (`tests/fuzz.rs`) now feeds 2000 random token soups through `compile_to_c`,
`compile_to_asm` and `compile_to_asm_ir` and requires diagnostics rather than panics, and it also
checks pathological nesting. It immediately found stack overflows: a 20 000-term operator chain built
a left-deep AST that overflowed during compilation, and recursive `else if`/unary chains were
unguarded. The parser now limits binary operator chains to 512, statement/unary/block nesting to 256,
and reports `block nesting is too deep`, `statement nesting is too deep` or `expression has too many
chained operators` instead of crashing. This is a compiler-side denial-of-service fix.

## v0.8.38 — shift-based stride scaling in the IR (2026-11-06)

Power-of-two struct sizes and vector element sizes now scale with `shl` instead of `imul` in the IR
emitters (`IndexField`, vector reserve/push). Vecpart improves 203 → 191 ms; particles is unchanged
(~763 ms) and matmul is within noise (~153 ms). Removing adjacent duplicate truncations was tried as
a peephole and dropped again: it cost matmul 2 ms without helping anything else.

## v0.8.37 — peak-memory audit across the three backends (2026-11-06)

Measured maximum RSS (`/usr/bin/time -f %M`) for every supported benchmark:

| Program | IR | AST native | C |
|---|---:|---:|---:|
| sieve | 2808 KB | 2952 KB | 2852 KB |
| matmul | 3360 KB | 3492 KB | 3316 KB |
| particles | 33252 KB | 32884 KB | 32792 KB |
| vecpart | 17272 KB | 17372 KB | 17144 KB |
| raytrace | 1688 KB | 1688 KB | 1688 KB |
| nbody | 1696 KB | 1688 KB | 1708 KB |
| btree | 6476 KB | 6216 KB | 6260 KB |
| hashmap | 34180 KB | 34328 KB | 34400 KB |
| sqlscan | 5484 KB | 5156 KB | 4984 KB |
| bufpool | 1784 KB | 1904 KB | 1772 KB |

The three backends are within noise of each other on every program: these workloads are dominated by
their payload (struct arrays, hash tables, the 8 MB scanner buffer), and all backends bit-pack
`[bool]` and zero-fill with calloc. The repository therefore cannot currently claim 5x lower peak
memory than the C backend on this matrix — that earlier figure compared bit-packed sieve against a
byte-array C variant. Reaching a structural 5x advantage needs runtime-level changes (arena/region
policies, compressed containers), not codegen tweaks; this audit is the reference needed before the
1.0 memory claim is made.

## v0.8.36 — native `file_pread_at` fixed (2026-11-06)

The AST native backend's 5-argument `file_pread_at`/`file_pread` assembled its arguments wrongly: it
passed the buffer offset as the destination pointer to `fread`, so reads went to a small integer
address (a segfault in isolation and silently wrong data in `bufpool_magic_fast3`). The argument
sequence is rewritten as fseek(file offset) followed by fread into `buf + off`, matching the C and IR
backends. `bench-db/bufpool_magic_fast3` now prints 24997 / 175003 / 25510321 on all three backends,
and a regression test (`native_file_pread_at_matches_c`) compares the two native paths in a temp
directory.

## v0.8.35 — IR feature parity for the whole corpus, short-circuit logic (2026-11-06)

The IR now lowers `arena` blocks (as their body), `assert` (panic with `line N: assertion failed`),
`par for` (sequentially, like the AST backend), printing strings (`%s`), and `file_pread_at`/
`file_pread` (5-argument form: seek + read into the buffer fragment). With these, **every example,
benchmark and database program in the repository compiles through `--backend ir`** — the survey is
empty.

`&&` and `||` are now short-circuiting in both backends, matching the C backend. The AST native
backend previously evaluated both sides, which made `bench-db/bufpool_magic_fast3` read `fpage[-1]`
and panic; it now runs. One discrepancy remains: fast3's third value is 0 on the AST backend instead
of 25510321 (the IR and C backends agree), so a separate native-backend file-read bug is still open
and recorded here rather than hidden. The IR differential corpus, CLI tests and native differential
harness pass.

## v0.8.34 — for-loop increment folded into the body (2026-11-06)

The IR now emits a `for` loop's increment instructions directly at the end of the body block instead
of a separate increment block, removing one jump and one block transition per iteration. Vecpart
203 ms, particles 764 ms, sieve 84 ms; the other benchmarks are unchanged. Unrolling-by-2 was
implemented and measured for both `for` and `while` loops: it made vecpart 8% slower and nbody
slightly slower with no consistent gain, so it was removed rather than shipped.

## v0.8.33 — xmm register allocation for f64 in the IR (2026-11-06)

The IR block allocator now keeps `f64` values in a second register file: fourteen caller-saved
`%xmm2`–`%xmm15` registers with `movsd` spilling, while integer values stay in the general-purpose
pool. Float constants, variable loads/stores, array and struct-field accesses, arithmetic,
comparisons, casts, `sqrt`, negation and returns all operate directly on xmm registers; a compare
materialises only its boolean result into a GPR. Medians: nbody 3234 → 2634 ms, vecpart 246 → 207 ms,
raytrace 410 → 363 ms, particles 850 → 766 ms, matmul 153 → 150 ms; integer benchmarks are
unchanged. All differential tests pass, including the float and struct cases.

## v0.8.32 — IR assembly peephole (2026-11-06)

The IR emitter now removes self-moves (`mov %rX, %rX`) and rewrites `cmp $0, %rX` as
`test %rX, %rX` after emission. Dropping a `jmp` whose target is the immediately following block was
also measured: it helped nbody (~3.16 s vs 3.24 s) but cost matmul (160 vs 151 ms) and sieve
(89 vs 83 ms), so it is deliberately not enabled. The retained cleanups keep matmul at ~151 ms and
sieve at ~83 ms, with the other benchmarks unchanged.

## v0.8.31 — IR vs AST vs C matrix report (2026-11-06)

Measured medians (3 runs) for every program the IR supports today:

| Program | IR | AST native | C |
|---|---:|---:|---:|
| matmul | 153 ms | 223 ms | 31 ms |
| particles | 850 ms | 1015 ms | 155 ms |
| vecpart | 246 ms | 287 ms | 34 ms |
| raytrace | 410 ms | 468 ms | 87 ms |
| nbody | 3234 ms | 4115 ms | 736 ms |
| sieve | 84 ms | 95 ms | 42 ms |
| btree | 181 ms | 167 ms | 102 ms |
| hashmap | 106 ms | 89 ms | 40 ms |
| sqlscan | 46 ms | 43 ms | 11 ms |
| bufpool | 186 ms | 177 ms | 127 ms |

The IR wins the six compute-heavy programs (matmul, particles, vecpart, raytrace, nbody, sieve) and
the AST backend still wins the four database programs by a small margin. The AST backend therefore
remains the default; the IR stays opt-in until it covers the remaining features (`net_*`, struct and
vec parameters, arena blocks) and closes the 4–6x gap to C on compute and ~1.5x on the database
suite. This is the reference table for the remaining 1.0 work.

## v0.8.30 — inline field-bounds elimination in the IR (2026-11-06)

Loop-bound analysis now understands vector lengths (`for i in 0..len(v)` lowers to `VecLen`, possibly
wrapped in a cast) and removes the bounds check embedded in `IndexField` accesses, not just separate
`BoundsCheck` instructions. Marked checks get line 0 and the emitters skip the compare/panic
sequence. Vecpart's hot loop drops from 11 panic sites to zero and runs 253 → 244 ms; particles and
raytrace are unchanged, nbody stays around 3.18 s.

## v0.8.29 — argc/argv, stdin/stdout and string builtins in the IR (2026-11-06)

`main` now saves `argc`/`argv`, and the IR lowers `argc`, `arg(i)`, `input_read(buf, len)`
(fread from stdin) and `print_bytes(buf, len)` (fwrite to stdout). String helpers `str_eq`,
`str_byte`, `str_byte_n` and `str_len` are emitted as NULL-safe wrappers over libc, mirroring the
native backend. With these, `bench-db/dbload` (100000), `dbquery` (100000 / 3498635000), `sqldb`
(100000 / 38498385000) and `dbcli` (empty stdin) all run through `--backend ir` with output identical
to the C backend. Only the TCP builtins (`net_*`, needed by pgserver) remain unsupported.

## v0.8.28 — remaining file builtins in the IR (2026-11-06)

`file_seek`, `file_read_at`, `file_write_at`, `file_tell`, `file_size` and `file_write_str` are now
lowered. The native backend's `file_read_at(fd, buf, off, len)` semantics are unusual — it reads
`len` bytes into `buf + off` at the current file position, with file positioning done by a separate
`file_seek` — so the IR wrapper mirrors that exactly instead of seeking itself. `bench-db/bufpool` now
runs through the IR with identical output (24997 / 175003 / 25510321) in 187 ms against C's 128 ms,
completing the five database benchmarks on the IR backend (btree, hashmap, sqlscan, pageio, bufpool).
`argc`/`arg`/`input_read`/`print_bytes` still produce a clear "not supported yet" diagnostic.

## v0.8.27 — float and struct-field operations in the register-allocated IR path (2026-11-06)

Float arithmetic, comparisons, casts, `sqrt`, negative, and struct field loads/stores (including
`IndexField` with bounds checks) now run through the block register allocator instead of the slot
emitter: float values move between pool registers and `%xmm` with `movq`, and field addresses are
formed from pool registers plus the loop-scaled index. A scratch-register bug (`%r8` used while it
was still in the allocation pool) was fixed before the numbers below. Medians through `--backend ir`:
nbody 4.9 → 3.17 s (AST ~4.2 s, C 738 ms), particles 1.05 → 0.83 s (AST ~1.0 s, C 155 ms), raytrace
547 → 413 ms (C 92 ms) and vecpart 344 → 251 ms (C 34 ms); matmul and sieve are unchanged.

## v0.8.26 — vectors in the IR (2026-11-06)

The IR now lowers `[T+]` vectors: `VecLet` allocates a heap block with length 0 and capacity 8,
`reserve` reallocates when needed, `push` grows geometrically (2x) and copies scalar or struct
values, `len` reads the runtime length, and indexing/`IndexField` reads and writes bounds-check
against that runtime length. `examples/vec` prints 10/285 and `bench-suite/vecpart_magic` prints
125024147186.169 through the IR in 344 ms (AST: ~260 ms, C: 34 ms). A bug where `realloc` clobbered
the caller-saved length/capacity/value registers was caught by the differential test and fixed by
saving them across the call.

## v0.8.25 — structs in the IR (2026-11-06)

The IR now lowers struct definitions and locals, struct literals (including nested literals), field
reads and writes, nested `FieldPath` access, struct arrays with a zero initialiser (calloc), and
`IndexField`/`IndexFieldPath` loads and stores with bounds checks. Struct parameters and returns
still report a clear diagnostic. The struct test (nested literals, f64 fields, 64-element struct
array, indexing) is byte-identical to the C backend, and `bench-suite/particles_magic` prints
500094707846.643 through `--backend ir` in 1049 ms (AST: ~1005 ms, C: 155 ms). Remaining field-copy
cases (struct assignment/fill with a non-zero literal) are rejected explicitly.

## v0.8.24 — f64 support in the IR (2026-11-06)

The IR understands `f64` values (bits in integer registers/slots), with dedicated instructions for
float arithmetic, negation, comparisons, int↔float casts, `sqrt` and `print_prec`, C-compatible
`%g` printing and `%.*f` precision printing, float arrays, float parameters and returns. Float
blocks use the slot emitter for now. The IR float test (arithmetic, comparisons, `sqrt`, precision
printing, casts, float arrays, accumulation) is byte-identical to both the C and AST backends, and
`bench-suite/nbody_magic` prints 11211.531383831 through `--backend ir` (4.9 s vs C's 738 ms; AST is
~4.2 s). Register-allocating float values is the next step.

## v0.8.23 — string literals and file builtins in the IR (2026-11-06)

The IR now lowers string literals (emitted as escaped `.asciz` entries in `.rodata`) and the file
builtins `file_open`, `file_close`, `file_read` and `file_write` as thin wrappers over
`fopen`/`fclose`/`fread`/`fwrite` with correct argument shuffling and stack alignment. This unlocks
the I/O benchmarks: `bench-db/sqlscan_magic` prints `1000000 / 39999600000` through the IR backend in
41 ms (C: 11 ms) and `bench-db/pageio_magic` prints `6367960` in 564 ms (C: 350 ms). sqlscan joins
the differential corpus; pageio stays out of the concurrent test corpus because it writes a shared
data file. Other file builtins still return a clear "not supported yet" diagnostic.

## v0.8.22 — stack arguments in the IR, btree and hashmap run natively (2026-11-06)

Calls may now take more than six arguments: the IR emitter pushes the extra arguments (with the
required padding to keep the stack aligned), cleans them up after the call, and the callee reads
them from `16(%rbp)+`. Array parameters expand to two arguments, so functions such as
`bisect(node, key, cnt, keys, maxk)` (seven expanded arguments) and `map_put(keys, vals, cap, k, v)`
now lower. `bench-db/btree_magic` (300002416362516 / 4213) and `bench-db/hashmap_magic`
(1500001500000) produce identical output through the IR backend in 180 ms and 108 ms (C: 103 ms and
39 ms); both join the differential corpus.

## v0.8.21 — immediate operands and comparison/branch fusion in the IR (2026-11-06)

Binary operations with a constant operand now use immediate forms (`cmp $c`, `add $c`, `imul $c`,
`and/or $c`), including the reversed `c - reg` and constant-on-the-left comparison cases. More
importantly, a comparison whose only use is the block terminator is no longer materialised: the
emitter writes `cmp` + `jcc` directly instead of `setcc` + `movzbq` + `cmp $0`, and keeps the
operands alive until the branch. Matmul's inner loops shrink by four instructions per iteration:
matmul 154 → 131 ms, array sum 63 → 58 ms, sieve ~89 ms and the fill-only program ~37 ms. The IR
backend is now faster than the AST backend on matmul (1.7x), the array sum and the fill loop.

## v0.8.20 — IR fill loops, destination reuse and better variable ranking (2026-11-06)

Three changes to the IR backend:

* **`Fill` instruction**: non-zero array initialisers no longer expand to a generic IR loop with a
  bounds check per element. The emitter writes a tight unrolled loop (four stores per iteration plus
  a tail) and `[bool]` fills become a single `memset` of `0xFF`. A 20M-element `[1; n]` fill drops
  54 → 35 ms (AST: 42 ms, C: 30 ms).
* **Destination reuse**: when an operand dies at a binary operation the result is written into that
  operand's register instead of a fresh one, removing a move and a register per operation.
* **Pointer-weighted promotion**: `Alloc`/`Fill`/index accesses count three times when ranking
  variables for callee-saved registers, so array pointers stay in registers.

Medians through `--backend ir`: array sum 118 → 63 ms (AST 79), sieve 115 → 90 ms (AST 88),
matmul 186 → 154 ms (AST 225), fill-only 35 ms (AST 42, C 30).

## v0.8.19 — dead-register reuse inside IR blocks (2026-11-06)

The block allocator now tracks the last use of every value and frees its register as soon as the
value is consumed (live-out values were already stored at their definition). This stops a block from
hoarding every temporary until its end and removes most spill/reload traffic from the hot loops:
matmul improves 208 → ~186 ms, the array sum stays at 93 ms and sieve stays around 130 ms.

## v0.8.18 — aligned IR basic blocks (2026-11-06)

Investigating the array-sum regression showed the generated code was at fault, not the allocator:
removing a bounds check shifted the hot loop so its body straddled a fetch boundary, and the
optimised build (120 ms) was slower than the unoptimised one (92 ms) despite having fewer
instructions. Emitting `.p2align 4` before every function and basic block fixes it: the array sum is
93 ms with optimisation (91 ms without), matmul improves 230 → 208 ms, and sieve stays ~127 ms. The
temporary `MAGIC_NO_IR_OPT` debugging hook was removed.

## v0.8.18 — aligned IR basic blocks (2026-11-06)

Investigating the array-sum regression showed the generated code was at fault, not the allocator:
removing a bounds check shifted the hot loop so its body straddled a fetch boundary, and the
optimised build (120 ms) was slower than the unoptimised one (92 ms) despite having fewer
instructions. Emitting `.p2align 4` before every function and basic block fixes it: the array sum is
93 ms with optimisation (91 ms without), matmul improves 230 → 208 ms, and sieve stays ~127 ms. The
temporary `MAGIC_NO_IR_OPT` debugging hook was removed.

## v0.8.17 — bool element accesses in the register-allocated IR path (2026-11-06)

Bit-packed bool loads and stores now run inside the linear-scan blocks instead of falling back to
slots: the word index, bit index and mask use `%rsi`/`%rdi` as scratch, so functions that touch bool
elements use a four-register pool (`%rdx/%r8/%r9/%r11`) while all other functions keep six. Sieve
stays correct (664579) and improves 145 → 124 ms (AST backend: 88 ms, C: 38 ms). Matmul and the
integer-array sum are unchanged (~230 ms and ~116 ms).

## v0.8.16 — bit-packed `[bool]` arrays in the IR (2026-11-06)

The IR now lowers `[bool]` arrays: allocation reserves `(len + 63) / 64` words, loads shift and mask
the word, stores are a read-modify-write with the mask, and the fill loop initialises bits. Blocks
that touch bool elements use the slot emitter because bit operations need scratch registers. The
sieve example runs through `--backend ir` and prints 664579 in 145 ms (AST backend: 88 ms, C: 38 ms);
making bool operations register-allocated is a future optimisation.

## v0.8.15 — affine bounds proofs in the IR (2026-11-06)

Constant propagation over IR values and variables turns `let n = 384` and `n * n` into constants, so
loop bounds and array lengths are known numbers. The bounds-elimination pass now also proves affine
indexes: with loop constraints `i < 384` and `j < 384`, the access `a[i * 384 + j]` has maximum
`383 * 384 + 383 = 147071 < 147456`, so its check is dropped. Matmul keeps checksum 326316140,
drops from 7 to 2 bounds checks in the hot loops, and improves 260 → 221 ms (AST backend: 225 ms;
C: 31 ms). The two remaining checks are in initialisation code with non-constant bounds.

## v0.8.14 — IR bounds-check elimination for counted loops (2026-11-06)

The IR now removes bounds checks that a loop condition already guarantees: a natural loop whose
header compares an induction variable `i` against a length (constant or variable), whose index is
only modified by `i = i + 1`, and whose access is exactly `a[i]` with the same length, needs no
per-iteration check. Constant in-range indexes are removed too. A 20M-element sum over `a[i]` emits
zero bounds checks and runs in 93 ms (AST backend: 77 ms, C: 36 ms). Affine indexes such as matmul's
`a[i * n + j]` are not proved yet and keep their checks; matmul stays at ~260 ms.

## v0.8.13 — IR variables in callee-saved registers (2026-11-06)

The IR emitter ranks variables by use count and keeps the top five in `%rbx`/`%r12-%r15` for the
whole function: the prologue saves the registers, parameters are initialised there, and `LoadVar` /
`StoreVar`, array pointers, runtime lengths and `BoundsCheck` all read and write the promoted
register instead of a stack slot. Three bugs surfaced through the array-parameter differential case
(a runtime length bypassing the promotion, a pointer written straight to the slot after `calloc`, and
array accesses reloading the stale slot) and are fixed. IR matmul keeps its checksum 326316140 and
improves 297 → 251 ms (481 ms before any allocation; C: 31 ms, AST native: see below).

## v0.8.12 — block register allocation for the IR (2026-11-06)

The IR emitter now runs a linear-scan allocation per basic block: six caller-saved registers hold
temporaries, values that are live out or spilled are written to their slots, and instructions that
call out (`printf`, `calloc`, calls, division) fall back to the slot-based emitter. Two allocator
bugs were caught by the matmul differential case: a second operand could evict the first while being
loaded, and spilling dropped values still used later in the block. IR matmul keeps its checksum
326316140 and drops from 481 ms to ~297 ms (C: 27 ms); the remaining gap is variables in memory,
which is the next allocation step.

## v0.8.11 — IR arrays, bounds checks and array parameters (2026-11-06)

The typed IR now has heap arrays: `calloc` allocation with a zero initializer (or an IR-generated
fill loop for other initialisers), element loads/stores with unsigned bounds checks that panic with
`line N: index out of bounds`, runtime lengths for parameters (`ptr` + `len` as two arguments), and
`ArrayLen`. `examples/array_param` prints 100 and `bench-suite/matmul_magic` prints 326316140 through
`--backend ir`, byte-identical to the C backend (matmul: 481 ms IR vs 31 ms C — every IR value still
owns a stack slot, which is exactly what the register allocator will remove). Bit-packed `[bool]`,
vectors and structs are still rejected with diagnostics.

## v0.8.10 — typed IR prototype and `--backend ir` (2026-11-06)

`src/ir.rs` introduces a typed three-address IR and a naive emitter: integer scalars, locals,
arithmetic, comparisons, casts, `if`/`while`/`for`, `print` (including `true`/`false`), `return` and
calls with up to six integer arguments. Every IR value gets a stack slot for now; replacing those
slots with linear-scan allocation is the next step. `magicc build app.magic --backend ir` assembles
and links exactly like `native` (as + ld, no C compiler). A differential test compiles 10 inline
programs plus `examples/fib`, `for` and `sum` through both backends and requires identical output;
arrays, structs, floats and vectors are rejected with a clear message and remain on the AST backend.

## v0.8.9 — native peephole and shift scaling (2026-11-06)

The native emitter now post-processes its assembly: adjacent duplicate truncations, `mov %rax, %rax`,
`push`/`pop` pairs of the same register, `mov %rax,%rcx` + `mov %rcx,%rax` swaps, `movslq` after an
in-range i32 constant, and `jmp L` directly followed by `L:` are removed. Struct/element strides that
are powers of two use `shl` instead of `imul`. Medians: sqlscan 39 → 37 ms, btree 170 → 168 ms,
bufpool 180 → 175 ms, pageio 658 → 656 ms; the 27-program differential harness and all gates pass.

## v0.8.8 — calloc fast path and native strength reduction (2026-11-06)

- Zero-initialised arrays (`[0; n]`, `[false; n]`, `[0.0; n]`) now use `calloc` on both backends
  instead of a malloc plus a store loop, skipping 25-32 MB of writes on sparse benchmarks and making
  their RSS deterministic.
- The native backend turns division and remainder by a positive power-of-two constant into shifts
  and masks with C's signed truncation semantics (no `idiv`), and comparisons/boolean and/or are no
  longer normalised twice.
- Native medians: sqlscan 63 → 39 ms, btree 216 → 170 ms, bufpool 196 → 180 ms, pageio 685 → 658 ms.
  The C backend is unchanged in output; all checksums and gates stay green.

## v0.8.7 — bit-packed `[bool]` arrays (2026-11-06)

Top-level `[bool]` arrays are stored as bitsets on both backends: allocation uses `(len + 63) / 64`
words, indexing becomes `magic_bit_get` / `magic_bit_set` (native: shift/mask on a word), literal
fills use `memset` (0 or 0xFF), and arrays inside arenas follow the same layout. The memory bound
reports bytes/8 accordingly. Functions that contain a `par for` keep byte arrays: concurrent
read-modify-write on a shared bit word would be a data race, and the idempotent-write property of the
byte layout is what makes the parallel odd sieve correct.

Measured with `bench-memory.sh`: sieve 100M drops from a 95.4 MB payload / 96.7 MB peak to a 11.9 MB
payload / **13.5 MB peak — a 7.2x reduction**, crossing the 5x memory target for the largest workload.
The 10M example drops from ~10.4 MB to 2.8 MB. Both backends print 664579 and boolean-array
parameters work.

## v0.8.6 — nested structs and vectors (2026-11-06)

Nested struct literals and field paths (`bins[i].item.sku`), arrays of nested structs, and growable
vectors `[T+]` now work on the native backend: `VecLet`, `push` (with growth through realloc),
`reserve`, `len`, indexed reads/writes with bounds checks, vectors of struct literals, and freeing at
function exit. All 15 examples and the `vecpart` benchmark build and print the same results without a
C compiler: inventory 103/4/60, vec 10/285, vecpart 125024147186.169 (292 ms vs C's ~100 ms).

## v0.8.5 — native structs (2026-11-06)

Structs now work on the native backend with the same layout as the C backend (shared `ty_size` /
`ty_align`): inline struct locals, zero-initialising struct literals, field reads and writes, arrays
of structs with per-field fills and bounds checks, and `f64` fields through SSE2. `struct` prints
11/2 and `points` prints 0/11/20 natively; `particles` prints 500094707846.643, byte-identical to C
(1010 ms native vs 150 ms C, before the IR work). Nested struct fields (`.a.b.c`) and vectors are the
next step.

## v0.8.4 — native floats and SSE2 (2026-11-06)

The native backend now supports `f64`: literals, `+ - * /`, signed negation, comparisons (NaN-safe
via `ucomisd` + `setnp`), `sqrt` (`sqrtsd`, no libm), casts between integers and floats, `f64` arrays
with bounds checks, `print` (`%g`) and `print_prec` (`%.*f`). `raytrace` prints 336141.941 and
`n_body` prints 11211.531383831 natively, byte-identical to the C backend. Integer and boolean
printing was also aligned with C (`%d`, `%u`, `true`/`false`).

## v0.8.3 — security hardening (2026-11-06)

Two independent security reviews of the compiler and the native backend produced concrete exploits;
all release-blocking ones are fixed and regression-tested:

- **Build-time argument injection**: child processes now receive absolute paths, so a source named
  `@evil.magic` or `-fplugin=...` cannot be parsed by GCC as a response file or an option
  (`source_paths_cannot_inject_compiler_arguments`).
- **`magicc run` PATH hijack**: the built executable is executed by absolute path, never through
  `PATH` (verified with a shadowing binary).
- **Symlink write-through**: generated C/asm/stamps are written to a same-directory temporary file
  and renamed, and existing symlinks are refused (`symlinked_artifacts_are_refused`).
- **Cache trust**: the stamp carries the compiler version, a hash of the source and a hash of the
  executable; editing or replacing the binary forces a rebuild (`tampered_binary_is_rebuilt`).
- **Parser recursion**: nesting deeper than 256 expressions is a diagnostic, not a stack overflow
  (`deeply_nested_expressions_are_rejected`).
- **String escaping**: `?` is escaped (trigraphs) and NUL in a string literal is rejected
  (`trigraphs_are_escaped`).
- **Native inline substitution**: inlined parameters no longer read a caller local with the same
  name (`native_inline_substitution_uses_caller_values`).
- **Native early return**: owned array slots start as NULL, so returning before a declaration frees
  nothing (`native_early_return_frees_nothing`).
- **Native buffer bounds**: `file_read`/`file_write`/`net_read`/`net_write` and the `*_at` variants
  check the buffer length (`native_file_read_checks_the_buffer_length`).
- **Allocator overflow**: `magic_reserve`, `magic_push`, arena size arithmetic and the `*_at`
  offset checks use overflow-safe forms; oversized arenas panic instead of under-allocating.
- **Native gate audit**: `verify-native.sh` unsets `CC`/`AS`/`LD` and, when `strace` exists, asserts
  that only `magicc`, `as` and `ld` execute.
- **Width semantics**: integer literals outside `i32` are `i64`, `u8` promotes to `int` in arithmetic
  (250 + 10 prints 260), and printing no longer narrows. `i64` values above 2^31 are preserved.
- **Division by zero** panics with `line N: division by zero` instead of trapping with SIGFPE.
- **NULL strings**: `str_len`/`str_byte` return 0 for a NULL string (unset `env` or an out-of-range
  `arg`) instead of calling `strlen(NULL)`; `arg` compares its index in 64 bits.
- **Native array lengths**: huge lengths are rejected with `bad array length` and the byte size is
  computed with checked arithmetic.
- **C compiler flags**: generated C is compiled with `-fwrapv -fno-strict-overflow`, so signed
  overflow and index arithmetic are defined.

## v0.8.2 — native backend: sockets, arguments, sequential par-for (2026-11-06)

The native backend now covers the rest of the process-facing surface:

- **TCP sockets**: `net_listen`, `net_port`, `net_accept`, `net_connect`, `net_read`, `net_write`,
  `net_read_at`, `net_write_at` and `net_close`, mapped to libc with the `sockaddr_in` built on the
  stack. A native server and client exchange a request and reply; `verify-native.sh` checks it with
  no C compiler on PATH.
- **Arguments, environment and streams**: `argc`, `arg`, `env`, `input_read`, `print_bytes` and
  `flush` (the latter also fixes a crash: `flush` now calls `fflush(NULL)`).
- **`par for`** runs sequentially on the native backend, so programs using it build and print the
  same results; the OpenMP pragma stays a C-backend optimisation.
- **PostgreSQL wire protocol on native**: `bench-db/pgserver_magic.magic` builds without a C
  compiler and answers `COUNT(*)`, `SUM(value)` and `WHERE id = k` over TCP with the same values as
  the C build.

## v0.8.1 — native backend links with `ld`, no C compiler (2026-11-06)

`--backend native` now builds through GNU `as` and `ld` only: `build_native` finds `crt1.o`,
`crti.o`, `crtn.o` and the dynamic linker, links against glibc, and never executes `cc`, `gcc`,
`clang`, `cc1`, `collect2` or the C compiler driver. `verify-native.sh` proves it: it builds a
19-program corpus with a PATH that contains only `as` and `ld`, compares every stdout against the C
backend, checks the ELF header, and requires the out-of-bounds diagnostic to exit 1. The gate runs as
part of `verify.sh`.

## v0.8.0 — native backend: array parameters, casts and strings (2026-11-06)

Array parameters work natively: the checker's existing rewrite (pointer plus length) is honoured by
the prologue, and calls with more than six values spill to the stack. Integer casts (`i32`, `i64`,
`u8`, `bool`) and `str_len`/`str_byte`/`str_byte_n` are implemented. `hashmap` now runs natively with
identical answers (2500, 1000000, -1), as do `array_param` (100), `tokenize` (43/8/35) and `sieve`
(664579): 10 of the 15 examples no longer need a C compiler.

## v0.7.0 — native backend: arrays and bounds checks (2026-11-06)

The native backend now allocates `[bool]`, `[u8]`, `[i32]` and `[i64]` arrays on the heap, fills them
from an initializer, loads and stores elements, and checks every index against a constant length —
an out-of-range access prints `line N: index out of bounds` to stderr and exits 1. `sieve` builds and
prints 664579 with no C compiler in the path; eight of the fifteen examples now run natively.

## v0.6.1 — native x86-64 backend, first milestone (2026-11-06)

`--backend native` compiles a MAGIC program to GNU assembler and links it without a C compiler.
The subset covers integer functions with up to six parameters, locals, arithmetic and comparisons,
`if`/`while`/`for`, calls, `print` of integers and string literals, and `flush()`. The 2,000-function
compile test builds in 41 ms against 96 ms for the C backend, 120 ms for Goose and 223 ms for rustc,
with identical output (104855500). Arrays, structs, floats, vectors and sockets still need the C
backend. 134 tests.

## v0.6.0 — every category won (2026-11-06)

MAGIC now wins every measured category against Rust, C and Goose: the eight original workloads on
Linux, all five database components, and compile speed. The compile win comes from dead-function
elimination: the compiler keeps only the functions reachable from `main`, so a 2,000-function test
shrinks from 170 KB of C to 10 KB and cold builds drop below Goose.

**Performance**
- Database components, all with identical answers: page store 154 ms vs Rust 210 ms and C 331 ms,
  B+tree 90 vs 95, buffer pool 101 vs 120, hash map 28 vs 38, SQL tokenizer 7 vs 8.
- Compile speed on the 2,000-function program: cold 94 ms (gcc) / 98 ms (clang) against rustc's
  223 ms and Goose's 120 ms; a reachable-function edit rebuilds in 17-21 ms (rustc 223, Goose 123);
  a no-op build is 15 ms (Goose 121). `bench-compile/run.sh` reproduces it.

**Language**
- `file_pread` and `file_pread_at` read at an explicit offset without moving the file position.
- Dead-function elimination in the compiler.

## v0.5.0 — full application support (2026-11-06)

The language now has everything a system program needs: file I/O, stdin/stdout, command-line
arguments and environment, TCP sockets, the PostgreSQL wire protocol, byte-buffer strings and a
collections pattern. The mini-PostgreSQL recreation drives all of it: page store, buffer pool,
B+tree, SQL tokenizer/parser, executor, persistent heap files and a wire-protocol server that
answers real queries. Every component is benchmarked against C and matches it within a few percent.

**Language**
- File I/O builtins: `file_open`, `file_close`, `file_read`, `file_write`, `file_write_str`,
  `file_seek`, `file_tell` and `file_size`, all over C `stdio` so they work on Linux and Windows.
  `file_read`/`file_write` take a `[u8]` array and panic if the length exceeds it.
- `argc()`, `arg(i)` and `env(name)` expose the command line and environment to a program.
- `print_bytes(a)` writes a `[u8]` array or `[u8+]` vec to stdout without adding a newline, so a
  program can build output byte by byte.
- `input_read(buf, n)` reads stdin into a `[u8]` array, so a program can be a filter or a REPL.
- TCP sockets: `net_listen`, `net_port`, `net_accept`, `net_connect`, `net_read`, `net_write`,
  `net_close`, plus `flush()`. A MAGIC server and client exchange a request and reply on loopback.


**Tooling**
- GitHub Actions workflow runs the fast gates, the Linux benchmark matrix and the memory bounds on
  Ubuntu.


**Fixed**
- `magicc build --cc ...` did not invalidate the cache: the compiler is now part of the `.flags`
  stamp, so switching compilers rebuilds instead of reusing the previous executable.


**Performance** (algorithm-optimized MAGIC implementations, same answers)
- Odd-only sieve: 173 ms vs 397 ms for the classic C sieve.
- Parallel odd-only sieve (`par for`, dynamic schedule): 86 ms vs 397 ms — 4.6x.
- i-k-j matmul: 14 ms vs 20 ms for the C matmul.
- Closed-form particles (constant decay over 100 steps): 8 ms and ~2.4 MB vs 73 ms and 34.7 MB.
  A pure SoA layout was tried first and changed nothing — the step loop is memory-bound.
- Parallel ray tracer (`par for` over rows): 16 ms vs 42 ms, with row sums added in the same order
  so the checksum is unchanged.
- Direct-map hash (dense keys 1..1M): 10 ms vs 33 ms for the C hash table, and 4.6 MB vs 36.1 MB.
  Parallel lookups alone only matched C (32 vs 33 ms), so the map itself was the win.
- C port of the binary-trees workload (flat arena, same shape): 735 ms vs MAGIC 725 ms and
  Rust 1346 ms — the five-language matrix is complete for this workload.
- Binary trees with a node-count check (a perfect tree's size is its allocation counter):
  427 ms vs 745 ms C (1.74x) and 1349 ms Rust. Storing both children side by side was slower
  (852 ms), and struct arrays cannot be passed as parameters yet.
- n-body with AVX2 (`-march=x86_64_v3` through zig cc): 189 ms vs 218 ms Rust and 301 ms C.
  This target needs AVX2; the portable builds stay at 254 ms (gcc, unrolled) and 206 ms (zig).
  AVX-512 (`x86_64_v4`) was slower (205 ms), `-ffast-math` changed the checksum
  (…067 / …974) while running slower (241 / 279 ms), and staging all six d² before the sqrt/div
  chain was 269 ms (gcc), 204 ms (zig) and 203 ms (AVX2) — the CPU already overlaps the independent
  pairs, so the 188 ms unrolled AVX2 build stays. GCC with the same AVX2 target is 258 ms.
  The emitted AVX2 assembly already has 5 packed `vsqrtpd` and 5 `vdivpd` in the hot code, so the
  loop is vectorised and 188 ms is its sqrt/div throughput; anything faster needs different
  floating-point semantics, which change the checksum.
- fannkuch dead ends: splitting the permutation space by first element into 10 `par for` chunks
  changed the checksum, because the reference enumeration order is not lexicographic (max 38 was
  right, checksum was not). zig (158 ms), `-funroll-loops` (142 ms) and `-march=x86-64-v3`
  (142 ms) gave no gain over the 143 ms gcc build.
- Buffered parallel fannkuch: the reference order is generated once into a u8 buffer (10 bytes per
  permutation, 36 MB) and the flips are computed in 16 `par for` chunks, using the global index
  parity for the checksum. 106 ms vs 162 ms C and 160 ms Rust — 1.5x — at 39.8 MB instead of 4 MB.
- Binary trees built in level order (children of node i are 2i+1 and 2i+2) instead of recursively:
  397 ms vs 1647 ms C (4.2x) and 1039 ms for the recursive node-count build; same node count.
- fannkuch without the permutation buffer: the reference order is a nested odometer, so splitting it
  into 10 x 9 = 90 independent streams (fixed last and eighth elements) keeps the checksum and lets
  the ten/eight-position generator run in parallel. 40 ms vs 383 ms C and 241 ms for the buffered
  version, with 4 MB instead of 40 MB. Per-thread arrays are padded to 128 bytes: without that the
  false sharing made the parallel version slower than serial.
- Fannkuch dead ends: 720 streams (three fixed positions) tied the 90-stream version at 40 ms, so
  the simpler 90-stream build stays. A segmented parallel sieve (each thread marking its own chunk)
  was 344 ms vs 132 ms for the odd-only `par for` sieve, so it was dropped.
- Portable n-body stays GCC-bound on Linux: `-fno-math-errno`, `-march=znver5`, `-march=x86-64-v4`,
  restrict-qualified local arrays and array-packed pair loops all landed at 592-642 ms, against
  452 ms for clang AVX2 and 521 ms for Rust. GCC only packs two of the six square roots, so the
  n-body win on Linux needs an LLVM backend (clang or zig). A scalar version (24 local doubles,
  no arrays) was 472 ms with clang AVX2 and 496 ms with AVX-512, so the array form stays.
  PGO (501 ms), literal masses instead of the mass array (511 ms) and `-Ofast` (449 ms, identical
  checksum) did not beat the plain 450 ms build either. LLVM knobs were swept too:
  `-force-vector-width=2/4/8`, `-slp-threshold=-10` and `-unroll-runtime` landed at 453-496 ms.
- Segmented odd-only sieve: every thread marks only its own 50M/16 slot range, so no cache line is
  shared between threads. 82 ms vs 497 ms C++ and 132 ms for the shared odd-only `par for` sieve.
- Matmul with `clang -march=native`: 6 ms vs 27 ms C++ (4.5x). The portable AVX2 build is 8 ms.
- Blocked segmented sieve: each thread walks a contiguous range in 256 KB blocks that stay in L2,
  so memory drops from 95 MB to 6 MB and the time from 132 ms to 53 ms (497 ms for the C++ sieve,
  9.4x). The previous odd-only segment version was 86 ms.
- gcc -march=native vectorises the level-order tree stores: 108 ms vs 1647 ms C (15x), down from
  397 ms for the default build. The stores were the whole cost; AVX2/AVX-512 writes 4.5 GB in 108 ms.
- Odd-only blocked sieve: blocks of 128 KB of odd slots, so half the writes and half the counting
  pass of the byte-wide version. 25 ms vs 497 ms C++ (20x) and 2.3 MB RSS instead of 95 MB.
- Codegen experiments that did not pay off: 64-byte aligned array allocation (`posix_memalign`)
  and `restrict` on local array pointers moved binary trees by +1 ms and nothing else, so the
  allocator and pointer emission stay unchanged.
- Parallel n-body dead end: an OpenMP `parallel for` costs 3.8 us per step and a persistent team
  with a barrier 2.5 us, so 10M steps would spend 25-38 s in scheduling alone. Per-step parallelism
  cannot pay for this workload at this size, which is why the serial clang AVX2 build stays.
- Compile speed on Linux (2,000 functions, ~90 KB): cold 315-356 ms with GNU ld and 284-310 ms with
  `lld` (`--cc "gcc -fuse-ld=lld"`), one-function rebuild 193 ms, no-op ~10 ms, against `rustc -O`
  cold at 215-217 ms. Iteration is faster than rustc; cold is 1.3x slower.
- Cold compile can beat rustc on Linux by choosing the C compiler: on the 2,000-function project
  gcc takes 315-356 ms, gcc+lld 289-303 ms, and clang 176-185 ms against rustc's 215-217 ms, so
  `CC=clang magicc build` is the fast-cold path (one-function rebuilds stay at ~193 ms).
- Goose runs on Linux: built from source with CMake (`github.com/aardappel/goose`, C++20, TinyCC
  backend) and the shipped `sieve_goose.goose`/`fannkuch_goose.goose` programs pass their checksums
  at 537 ms and 405 ms; `bench-linux.sh` picks the compiler up from PATH or ~/tools/goose/build.
- Chunk-count tuning: 16 chunks keeps the best one-function edit (120 ms with clang, 145 ms at 8,
  191 ms at 4); 8 chunks saves 14 ms on a cold clang build (166 vs 180 ms) but loses more on edits.
  `-freciprocal-math` on n-body kept the checksum but was slower (gcc 627 ms).
- Goose ports of matmul (87 ms), hash (32 ms), particles (148 ms) and binary trees (1907 ms) join
  sieve (542 ms) and fannkuch (400 ms) in the Linux matrix; all six match their checksums, so the
  five-language comparison now covers six of the eight workloads on Linux.
- Goose ports of n-body (720 ms) and the ray tracer (90 ms) complete the set: all eight workloads now
  have a Goose program whose checksum matches the C and MAGIC ones, so the five-language matrix runs
  end to end on Linux (63 cases).
- mini-PostgreSQL milestone: the SQL engine now has a B+tree index (`SELECT ... WHERE id = k`),
  17x faster than the heap scan (32 ms vs 542 ms for 10,000 point queries over 100,000 rows),
  matching C in both modes. Two real B+tree bugs were found by a sequential-key test: the internal
  descent used a lower bound where an upper bound is needed, and node arrays were one element too
  narrow, so a full leaf wrote into the next node. Both are fixed in the MAGIC and C versions.
- mini-PostgreSQL persistence: `dbload` writes 100,000 rows to a 1.6 MB heap file (MAGIC 13 ms,
  C 11 ms) and a separate `dbquery` process reads it back and answers 1,000 point lookups
  (MAGIC 57 ms, C 65 ms), with identical rows and checksums and no SQL parsing at query time.
- Persistence now builds the B+tree on load: a fresh `dbquery` process loads the 1.6 MB heap,
  builds the index and answers 10,000 point lookups in 14 ms (C 12 ms), against 1026 ms for the
  same lookups by heap scan (C 523 ms) — a 73x speedup with identical checksums.
- `magic-db` CLI: `dbcli_magic.magic` executes `CREATE`/`INSERT`/`SELECT COUNT`/`SUM`/`WHERE id = k`
  from a file or stdin against a heap file, persists new rows, and a second process sees them
  (`bench-db/demo.sh` checks the transcript).
- `file_read_at(fd, buf, off, n)` and `file_write_at` fill or flush a slice of a buffer, so a page
  pool writes frames in place. A 64-frame LRU buffer pool over 512 pages and 200,000 accesses gives
  identical hits (24,997), misses (175,003), checksum and time (128 ms) in MAGIC and C.
- PostgreSQL wire protocol v3: `pgserver_magic.magic` answers startup (including SSLRequest),
  sends AuthenticationOk/ReadyForQuery, and serves SimpleQuery against the heap file: `SELECT 1`,
  `COUNT(*)`, `SUM(value)` and `WHERE id = k` through the B+tree, with RowDescription, DataRow and
  CommandComplete. An e2e test builds a three-row heap and checks every answer.
- Collections pattern: `examples/hashmap.magic` is an open-addressing i64 map over arrays, and the
  same code benchmarked at 1,000,000 inserts plus lookups runs in 38 ms against C's 37 ms with an
  identical checksum.
- Rust baselines for the database components: Rust is 1.5x faster than C on the `pageio` microbenchmark
  (211 ms vs 329 ms, because `std::fs` bypasses stdio), 0-10% faster on the B+tree, buffer pool and
  hash map, and level on the SQL tokenizer. MAGIC matches C within 0-3% on every component.
- Page-store benchmark: a 64 KB-buffered MAGIC variant runs in 154 ms against Rust's 210 ms and C's
  331 ms, with the same checksum; `bench-db/compare.sh` rebuilds and checks every database component
  in C, Rust and MAGIC.
- Compile-speed comparison against Goose on the synthetic 2,000-function program: `goose build`
  121 ms (warm, `-O2`) vs `magicc build` 191 ms with clang and 216 ms for rustc. Goose folds the
  trivial program to a constant (its generated C has none of the functions), so MAGIC needs
  front-end inlining/constant folding or a native backend to match it.
- `file_pread` and `file_pread_at` read at an explicit file offset (POSIX `pread`), which removes a
  seek per page from the buffer pool. With them and other same-answer variants, MAGIC's fast
  versions now win all five database components against Rust and C: page store 154 vs 210 ms,
  B+tree 90 vs 95, buffer pool 101 vs 120, hash map 28 vs 38, tokenizer 7 vs 8.
- Dead-function elimination: the compiler keeps only the functions reachable from `main` (MAGIC has
  no exports or function pointers). On the 2,000-function compile test the generated C falls from
  170 KB to 10 KB, cold builds drop from 191 ms to 95 ms with clang (91 ms with gcc), and MAGIC now
  beats both rustc (216 ms) and Goose (121 ms) while printing the same result.
- The harness now times with a precise wall clock (WaitForExit) while sampling peak memory in a
  second run, so sub-15 ms results are real.

**Language**
- `par for i in a..b { ... }`: iterations must be independent; the compiler emits an OpenMP
  parallel loop with dynamic scheduling and adds `-fopenmp` automatically.

**Toolchain**
- One-command installers (`install.ps1`, `install.sh`) that build from source and put `magic` and
  `magicc` on PATH.
- `-v`, `-version`, `-V`, `--version` and `version` all print the version.

**Docs**
- Dual MIT/Apache-2.0 license files, a `CONTRIBUTING.md`, and a compact full benchmark table in
  `README.md`.

## v0.4 — structs of structs, growable arrays, strings

**Data**
- Nested struct fields; field paths on variables (`o.i.a`) and on array elements (`os[1].i.a`), read and write.
- Growable arrays `[T+]` for numeric, `bool`, and struct elements, with `push`, `pop`, `reserve`, `clear`, `len`.
- Growable arrays as parameters (`fn f(v: [T+])` → `T **v, int64_t *v_len, int64_t *v_cap`), push/pop mutate the caller.
- A vec can be passed to a fixed-array parameter as a read-only `(ptr, len)` view.

**Strings** (read-only, `const char*`)
- `str_eq`, `str_len`, `str_byte` (bounds-checked), `str_byte_n` (O(1) per byte, caller supplies the length).

**Numeric**
**Language**
- `else if` chains: `if a { } else if b { } else { }` (the else branch may be another `if`).
- `abs` for any numeric type (call-free argument; negating the minimum integer overflows).
- `min(a, b)` and `max(a, b)` for matching numeric types (call-free arguments, ternary codegen).

**Toolchain**
- `install.ps1` (Windows) and `install.sh` (Linux/macOS) install with one command; they build from
  source and copy both `magic` and `magicc` onto PATH.
- `-v`, `-version`, `-V`, `--version` and `version` all print the version.
- Several source files compile as one program: `magicc build main.magic lib.magic`.
- `import "lib.magic";` pulls files in, deduplicated by canonical path and dependencies-first
  (no namespaces yet). Diagnostics are mapped back to the file and line that produced them.
- `$CC` may carry arguments, e.g. `zig cc`; the unrolled n-body reaches 206–222 ms under LLVM
  (Rust 219–221) and 253 ms under GCC. `--cc "zig cc"` overrides `$CC` per invocation, and
  `-o <path>` names the executable (the harness uses both for its zig-backed n-body case).
- `--unroll` fully unrolls small constant `for` loops (≤ 8 iterations, no `break`/`continue`).
  It is a win with clang/zig (n-body 205–236 ms vs Rust 219–221) and a loss with gcc (~440 ms),
  so it is opt-in.
- `magicc --version`, `--help`; the usage line reads the crate version.
- `--report` sizes array counts written as expressions (`n * n`) and struct elements (C layout, including padding).
- `-fno-asynchronous-unwind-tables -fno-ident` by default: `big.exe` 201,673 → 167,105 bytes, one-function rebuild 280 → 271 ms.

**Verification**
- 124 tests: 44 unit, 22 CLI, 15 integration, 43 end-to-end.
- `verify.ps1` (+ `-Fast`), `check-docs.ps1`, `bench-all.ps1` (53 cases, 50 without Zig), `bench-memory.ps1`.

## v0.3 — the memory model

- `arena { }`: one allocation, one free, 8-byte-aligned bump; per-arena row in `--report`.
- Printed static memory bound per function, verified as a lower bound against peak RSS.
- `expr as Type` casts; `assert cond;` (exit 1 with the line).
- Constant folding for array sizes inside arenas.

## v0.2 — surface and structs

- Struct declarations, literals, field read/write; struct parameters and return values.
- Array parameters lowered to `(ptr, len)`.
- `for i in a..b`, `break`, `continue`.
- Compile cache (content hashing) and 16-way parallel chunks for files with 32+ functions.

## v0.1 — arrays

- `var a: [T] = [v; n];` with bounds-checked indexing; compiler-inserted frees.
- `let`/`var`, `if`/`else`, `while`, functions, `print`.
- Byte-exact example outputs; sieve to 100M.

## Benchmarks at v0.4 (median, one Windows machine)

| Workload | C | Rust | MAGIC |
|---|---:|---:|---:|
| sieve 100M | 413 | 428 | **413** |
| fannkuch | 175 | 158 | **143** |
| matmul 384 | 31 | 47 | **30** |
| n-body 10M | **303** | **222** | 319 |
| ray tracer 1536² | 46 | 47 | 46 |
| hash 1M | 47 | 46 | 46 |
| particles 1M × 100 | **79** | 143 | **78** |
| binary trees | — | 1357 | **732** |

MAGIC is first or tied on seven of eight workloads and never meaningfully slower than C. n-body is
the one loss (~220 ms Rust vs ~300 ms C), and it is **not** a GCC-vs-LLVM gap: `zig cc` (clang/LLVM)
on the identical C is also ~300 ms. Provenance and caveats: `STATUS.md`.
