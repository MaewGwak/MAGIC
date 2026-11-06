#!/usr/bin/env bash
# Build all eight best-of C workloads with the benchmark's flags.
set -eu
cd "$(dirname "$0")"
CC=${CC:-gcc}
FLAGS="-O3 -march=native -fopenmp"
mkdir -p bin
for w in sieve fannkuch raytrace hash particles nbody bt matmul; do
    $CC $FLAGS -Wall -Wextra $w.c -o bin/$w -lm
done
echo "built: $(ls bin | tr '\n' ' ')"
