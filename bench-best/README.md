# Best-of-language benchmarks

Independent implementations written to be the fastest reasonable version in each language, not
ports of the original naive sources. Floating workloads are checked with a relative tolerance
(1e-9) because algebraic reorderings/closed forms legitimately differ in the last bits; integer
workloads are exact. This suite answers: "what is the best each language can do?", separate from the
historical portable matrix in BENCHMARKS.md.
