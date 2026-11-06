# Native backend: feasibility and plan to 0.9.0

## The question

Can MAGIC stop depending on a C compiler, and how close can 1.0.0 get to "no other language"?

Short answer:

- **Yes**, the C compiler can be removed as the code generator. A direct x86-64 backend is a normal
  compiler project: instruction selection, register allocation, the System V ABI, stack frames,
  floating point (SSE2), and a runtime for bounds checks and panics.
- **No**, "no other language at all" is not realistic for 1.0.0. Even a native backend needs an
  assembler and linker (or a hand-written ELF writer) and libc for `printf`, `malloc`, `fopen`,
  sockets and `libm`. Replacing libc means writing our own formatting, allocator, file and socket
  wrappers and a syscall layer per OS, which is a 1.x project, not a 0.x one.
- The realistic 1.0.0 target is: **MAGIC emits machine code itself for Linux x86-64, links against
  libc, and needs no C compiler.** The C backend stays as a portable fallback for Windows and for
  differential testing.

## What the compiler depends on today

| Dependency | Used for | Removable? |
|---|---|---|
| A C11 compiler (gcc/clang) | code generation, optimisation, linking | yes, by a native backend |
| libc (`stdio`, `stdlib`, `string`, `math`) | print, allocation, files, sockets, sqrt | not by 1.0.0 |
| POSIX/Winsock | files and sockets | only with a per-OS syscall layer |
| `as` + `ld` (through the C driver) | assembling and linking | yes with a direct ELF writer, or keep binutils |

The current pipeline (`src/lib.rs`): lexer -> parser -> type checker -> C emitter. The native
backend adds a typed IR between the checker and the emitter; both emitters consume the same IR, and
the existing 132-test suite runs against both.

## Architecture

1. **Typed IR** (`src/ir.rs`): functions, basic blocks, virtual registers, explicit `alloca` slots
   for arrays, typed binary/unary ops, calls, branches, loads/stores, bounds-check calls.
2. **x86-64 emitter** (`src/x86.rs`): a linear-scan register allocator over the IR; SysV ABI for
   calls (integer args in rdi/rsi/rdx/rcx/r8/r9, floats in xmm0-7, stack alignment 16); arrays as
   `(pointer, length)` pairs; bounds checks call `magic_panic`.
3. **Runtime** (`runtime/magic_rt.c` -> later `runtime/magic_rt.S`): the existing builtins as a
   small static library, written in C at first (that is not the compiler backend), then in assembly
   or MAGIC itself.
4. **Driver**: `magicc build --backend native`; default stays `c` until the native path passes the
   whole suite. `--emit asm` and `--emit obj` for debugging.
5. **Assembling/linking** ✅: `as` + `ld` with discovered crt objects and glibc; no C compiler or
   driver. A direct ELF writer is not needed for 0.9.0.

## Milestones

Versioning follows the requested scheme: `0.<feature round>.<fix>`.

| Version | Scope | Gate |
|---|---|---|
| **0.6.1** ✅ | Native emitter: integer functions, control flow, calls, `print`, `--backend native` | 41 ms cold build, identical output on fibonacci/loops/2,000-function test |
| **0.7.0** ✅ | control flow, functions, locals, heap arrays with bounds checks | sieve runs natively (664579); 8/15 examples |
| **0.7.x** | strings, structs, field access, casts, `par for` (pthreads), file I/O through the runtime | all e2e tests pass on both backends |
| **0.8.0** ✅ | array parameters, integer casts, string scanning builtins, stack arguments | `hashmap`, `tokenize`, `array_param`, `sieve` run natively with identical output |
| **0.8.4 (done)** | f64 and `sqrt` (SSE2), f64 arrays, `print_prec` | raytrace and n-body are byte-identical to C |
| **0.8.5 (done)** | struct locals, literals, fields, arrays of structs | struct/points/particles byte-identical to C |
| **0.8.6 (done)** | nested fields, arrays of nested structs, vectors incl. vectors of structs | all 15 examples + vecpart run natively |
| **0.8.7 (done)** | bit-packed `[bool]` on both backends | sieve peak 13.5 MB (7.2x less), same answer |
| **0.8.8 (done)** | calloc zero-fill, div/mod strength reduction, fewer normalisations | sqlscan 63→39 ms, btree 216→170 ms |
| **0.8.9 (done)** | assembly peephole + shift scaling for strides | sqlscan 39→37 ms, bufpool 180→175 ms |
| **0.8.x** | threads, the PostgreSQL demo on native, IR + register allocation | the whole `bench-linux.sh` matrix builds and checksums match |
| **0.8.x (done)** | file I/O builtins mapped to libc | `pageio`, `sqlscan`, `btree`, `bufpool` run natively with identical checksums |
| **0.8.2 (done)** | sockets, args/env, input_read/print_bytes, sequential par-for | native TCP roundtrip and the PostgreSQL wire server pass without a C compiler |
| **0.8.x** | performance: register allocation quality, inlining, direct syscalls for file I/O | native runtime within 10% of the C backend |
| **0.9.0** | self-hosting gate: the native backend compiles the whole test suite and the mini-PostgreSQL programs without invoking a C compiler | `cc` absent from PATH, all gates pass |
| **1.0.0** | Linux x86-64 release; Windows keeps the C backend; honest documentation of the libc dependency | no C compiler for the native path |

## Debate outcome (round 2): what we do next

Four independent reviews (performance, language features, release gates, adversarial) converged on:

- **Toolchain first, and it is done in 0.8.1**: link with `as` + `ld` and discovered crt objects, not
  a C compiler driver. `verify-native.sh` builds a 19-program corpus with a PATH that contains only
  `as` and `ld`, compares stdout to the C backend and checks the ELF header; `verify.sh` runs it.
- **Correctness before features**: the native codegen currently computes in 64-bit and never
  truncates to `i32`/`u8`, so results can diverge from the C backend. Fix width semantics and add a
  differential stage before adding structs/floats.
- **Features in this order**: structs (0.8.2), then f64/SSE2 (0.8.3), then vectors, args/env/
  input_read/print_bytes, and sequential `par for` (0.8.4). Native sockets stay on the C backend for
  0.9.0. Structs always have addresses; use a private pointer/sret convention rather than full SysV
  classification.
- **Performance after parity**: replace the stack-machine expression spine with a two-register
  spine, keep induction variables in registers, constant-fold and strength-reduce division by
  constants. Gate on medians and on push/pop counts in the generated assembly, not on anecdotes.
- **Honesty rules**: do not call this "self-hosted" (the compiler is Rust); say "no-C native
  backend". Native tests must not silently pass when `as`/`ld` are missing: the gate script fails.

## Typed IR prototype (v0.8.10)

`src/ir.rs` lowers integer programs to three-address code (`Const`, `LoadVar`, `StoreVar`, `Bin`,
`Un`, `Trunc`, `Call`, `Print`, `Ret`, `Br`, `Jmp`) with basic blocks and a naive emitter in which
every value owns a stack slot. `--backend ir` builds with `as` + `ld` like the native backend.

Supported: integer scalars, locals, arithmetic/division (guarded), comparisons, casts, `if`,
`while`, `for`, `print` (with C-compatible `true`/`false` for booleans), `return`, and calls with up
to six integer arguments, including recursion. Rejected with diagnostics: arrays, structs, floats,
vectors, `par for`, break/continue, string printing.

Arrays landed in v0.8.11: `Alloc` (calloc or an IR fill loop), `BoundsCheck` (unsigned compare,
`line N: index out of bounds`, exit 1), `IndexLoad`/`IndexStore`, runtime lengths, and array
parameters (pointer + length as two arguments). `examples/array_param` and `matmul` match the C
backend; matmul takes 481 ms against C's 31 ms because every value still has a stack slot.

Correctness is checked by `tests/ir.rs`: ten inline programs, `examples/fib`, `for`, `sum`,
`array_param` and `bench-suite/matmul_magic` must produce byte-identical output through the C and IR
backends.

v0.8.12 adds a per-block linear-scan allocator: six caller-saved registers for temporaries,
live-out/spilled values stored to slots, and a slot-mode fallback for blocks containing calls,
`printf`, `calloc` or division. v0.8.13 promotes the five most-used variables to callee-saved
registers for the whole function (saved in the prologue), which matmul keeps from 297 ms down to
251 ms (481 ms before allocation; C: 31 ms). Bounds checks, index scaling and the lack of
vectorisation are the remaining costs.

v0.8.14 adds loop bounds elimination in the IR: if a natural loop compares `i` against the same
length used by an `a[i]` check and only increments `i` by one, the check is dropped. A 20M-element
direct-index sum then emits zero checks (93 ms vs AST 77 ms, C 36 ms). Affine indexes like
`a[i * n + j]` are not proved yet, so matmul keeps its checks.

## Differential harness## Differential harness

`tests/native_diff.rs` builds a 27-program corpus (all examples, the integer/float/struct/vector
benchmarks and the database components) with both backends, runs them from the repository root and
requires identical stdout and exit status, with per-program timeouts. It is the gate every native
optimisation must pass; a mismatch fails the test with the program names. Current runtime: ~25 s.

## Register-allocation experiments (v0.8.8 → v0.8.9)

Two ad-hoc register experiments were attempted on the AST emitter and **both were reverted**; the
lesson is that register allocation needs a typed IR and differential tests, not string-level patches.

1. **Callee-saved induction counters** (`for i in ...` kept in `%rbx/%r12-%r15`): passed the native
   gate but changed timings by <5% (particles 1010→1011 ms, vecpart 292→274 ms, raytrace 464→469 ms),
   because the hot loops are bound by memory accesses, not by the counter.
2. **Scalar promotion** (params/locals held in callee-saved registers for their lifetime): the
   scanner benchmark miscompiled (one loop iteration, wrong branch) while `struct`/`points` and the
   DB benchmarks stayed correct. A minimal `a[n] + n` case also failed until the prologue was fixed;
   after that, `sqlscan` still diverged, so the whole change was reverted rather than shipped.

The current tree (v0.8.8) is green: 156 tests, native gate 28 programs, no promotion, no induction
registers. The next performance step is a real IR (`src/ir.rs`) with three-address code, explicit
bounds-check pseudo-ops, linear-scan allocation, and a differential harness that runs every e2e
source through both backends before any flag flips.

## Security hardening

The native path passes absolute paths to `as`/`ld`, refuses to write through symlinks, checks every
buffer length at the libc boundary, zeroes owned slots before an early return and wraps `i32`/`u8`
like the C backend. The compiler refuses expression nesting beyond 256, escapes trigraphs, rejects
NUL in string literals, hashes the source and the produced executable in its cache stamp, and runs
the built program by absolute path. `verify-native.sh` audits `execve` with `strace` when available.

## Risks

- **Floats and the ABI** are where hand-written backends usually break; allocate a full milestone
  for SSE2 and calling-convention corner cases.
- **Debugging**: a wrong register allocation produces silent corruption; keep the C backend as the
  reference and compare program output bit for bit.
- **Time**: this is the largest piece of work in the project; 0.6.1 is a spike, 0.7.0 is the first
  usable backend, 0.9.0 is the self-hosting gate.
- **Windows**: not in scope before 1.0; the C backend remains the Windows path.

## What 1.0.0 will honestly claim

- No C compiler in the native build path.
- Generated programs still link libc and start through the normal ELF loader.
- The C backend remains available (`--backend c`) as the portable and differential-testing path.
