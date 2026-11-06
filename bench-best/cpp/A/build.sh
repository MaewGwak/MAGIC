#!/usr/bin/env bash
# Build every best-of C++ workload (variant A).
# Suite toolchain command:  g++ -O3 -march=native -fopenmp -lm
set -euo pipefail
cd "$(dirname "$0")"

CXX="${CXX:-g++}"
FLAGS="${FLAGS:--O3 -march=native -fopenmp}"

mkdir -p bin
for w in sieve fannkuch raytrace hash particles nbody binary_trees matmul; do
    printf '  %-14s -> bin/%s\n' "$w.cpp" "$w"
    $CXX $FLAGS "$w.cpp" -o "bin/$w" -lm
done
echo "built 8 binaries in $(pwd)/bin"
