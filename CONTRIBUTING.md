# Contributing to MAGIC

MAGIC is a small compiler: around 2.5k lines of dependency-free Rust that emits one C11 translation
unit and lets the system C compiler finish the job.

## Build and test

You need Rust and a C compiler (`gcc`, `clang`, or `zig cc`). Then:

```powershell
cargo build --release
cargo test
```

The full gate runs the tests, the doc checks, 42 benchmark cases and the memory bounds:

```powershell
pwsh -File verify.ps1
```

`pwsh -File verify.ps1 -Fast` skips the benchmarks. After changing code generation, run
`pwsh -File bench-all.ps1 -Rebuild` and update the numbers in `BENCHMARKS.md` and `STATUS.md`.

## What a change needs

- **Tests.** Every language feature and every bug fix gets a test. The suite is 123 tests and must
  stay green.
- **Docs.** `check-docs.ps1` reads the test count, example count, version and workload count out of
  the files, so update `README.md`, `STATUS.md` and `TEST-MATRIX.md` together.
- **Benchmarks.** A codegen change runs the benchmark suite; report the new numbers instead of
  leaving stale ones.
- **Commits.** One focused change per commit, and the message says what changed and why.

## Style

- No external Rust crates. The compiler builds with `rustc` and nothing else.
- Keep the emitted C readable; comments in generated C explain bounds checks and arena bookkeeping.
- Prefer the smallest change that passes the gates.

## License

By contributing you agree that your contribution is licensed under MIT OR Apache-2.0.
