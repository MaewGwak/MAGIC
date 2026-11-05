# KILL.md — the numbers that end this project

A language without a kill number becomes a decade of enthusiasm. These are the gates.

## v0 (now)

**Pass:** `cargo test` green; `magicc run examples/hello.magic`, `fib.magic`, and `sum.magic` all produce correct output through the C backend.

**Fail:** if v0 cannot be built or run at all, stop and fix that before adding any feature.

## v0.1 — arrays (needed for any real benchmark)

**Pass:** the sieve of Eratosthenes (N = 10,000,000) compiles and matches C's answer.

**Fail:** if arrays cannot be expressed without adding a hidden allocator, the design is wrong — fix the design, not the test.

## The real gate — day 90

On three frozen programs (sieve, a JSON-ish parser, a pointer-shaped workload), against **hand-optimized Rust** (arena + typed indices + offsets, not idiomatic):

- **Memory:** peak RSS at least **1.3x lower**, and
- **Latency:** P99 at least **1.5x lower** under the same load, and
- **CPU:** never worse than **0.90x** of Rust's throughput on any program.

All three must hold. Two out of three is a fail.

## What this gate is not

- Not "faster than idiomatic C++". That baseline is a straw man; Goose's own 3.51x collapses to 1.24x against hand-tuned C++.
- Not "most powerful language". There is no such measurement.
- Not a throughput record. The $12-server challenge showed seven languages inside 1.17x on throughput; throughput is the wrong axis.

## If the gate fails

Publish the negative result and ship the surviving piece as a Rust attribute or lint. A 1.13x memory-model win is still worth having; a 1.13x win wrapped in a new language is not.
