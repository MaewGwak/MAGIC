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
5. **Assembling/linking**: start with `as` + `cc` as the linker driver, then `as` + `ld`, then a
   direct ELF writer if the dependency must go.

## Milestones

Versioning follows the requested scheme: `0.<feature round>.<fix>`.

| Version | Scope | Gate |
|---|---|---|
| **0.6.1** ✅ | Native emitter: integer functions, control flow, calls, `print`, `--backend native` | 41 ms cold build, identical output on fibonacci/loops/2,000-function test |
| **0.7.0** ✅ | control flow, functions, locals, heap arrays with bounds checks | sieve runs natively (664579); 8/15 examples |
| **0.7.x** | strings, structs, field access, casts, `par for` (pthreads), file I/O through the runtime | all e2e tests pass on both backends |
| **0.8.0** | floats and `sqrt` (SSE2), vectors/arenas, sockets, the PostgreSQL demo on native | the whole `bench-linux.sh` matrix builds and checksums match |
| **0.8.x** | performance: register allocation quality, inlining, direct syscalls for file I/O | native runtime within 10% of the C backend |
| **0.9.0** | self-hosting gate: the native backend compiles the whole test suite and the mini-PostgreSQL programs without invoking a C compiler | `cc` absent from PATH, all gates pass |
| **1.0.0** | Linux x86-64 release; Windows keeps the C backend; honest documentation of the libc dependency | no C compiler for the native path |

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
