#!/usr/bin/env bash
# Build all eight workloads with both MAGIC backends into ./bin.
#   native IR : magicc build w.magic              (default backend, as+ld)
#   C backend : magicc build w.magic --backend c --cc "clang -O3 -march=native ..."
# The rpath makes the OpenMP binaries run without LD_LIBRARY_PATH.
set -u
cd "$(dirname "$0")" || exit 2
M=${MAGICC:-/home/krittapas/Projects/magic/target/release/magicc}
ROOT=${CLANGROOT:-$HOME/tools/clangroot}
CLANG=${CLANG:-$ROOT/usr/bin/clang}
export LD_LIBRARY_PATH="$ROOT/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
CC="$CLANG -O3 -march=native -L$ROOT/usr/lib64 -Wl,-rpath,$ROOT/usr/lib64 -lm"
mkdir -p bin
for w in sieve fannkuch raytrace hash particles nbody bt matmul; do
    "$M" build "$w.magic" -o "bin/${w}_ir" || exit 1
    "$M" build "$w.magic" --backend c --cc "$CC" -o "bin/${w}_c" || exit 1
done
echo "built into $(pwd)/bin"
