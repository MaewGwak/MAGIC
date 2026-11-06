#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd); cd "$ROOT" || exit 2
C=clang; CXX=clang++; RS=rustc; M=target/release/magicc; OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$HOME/tools/clangroot/usr/bin/clang" ] && C="$HOME/tools/clangroot/usr/bin/clang" && CXX="$HOME/tools/clangroot/usr/bin/clang++"
export LD_LIBRARY_PATH="$HOME/tools/clangroot/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
timeit(){ local exe=$1; local v=(); for i in 1 2 3; do local s=$(date +%s%N); "$exe" >/dev/null; local t=$(date +%s%N); v+=($(( (t-s)/1000000 ))); done; printf '%s\n' "${v[@]}" | sort -n | sed -n '2p'; }
echo "partial best-of (ms, median3)"
gcc -O3 -march=native bench-best/particles_best.c -o "$OUT/pc" -lm
g++ -O3 -march=native bench-best/particles_best.cpp -o "$OUT/pcpp" -lm
rustc -O -C target-cpu=native bench-best/particles_best.rs -o "$OUT/prs"
"$M" build bench-suite/particles_magic_closed.magic --backend c --cc "$C -O3 -march=native" -o "$OUT/pm" >/dev/null 2>&1
echo "particles: C=$(timeit "$OUT/pc") C++=$(timeit "$OUT/pcpp") Rust=$(timeit "$OUT/prs") MAGIC=$(timeit "$OUT/pm")  (C output $( "$OUT/pc" ); MAGIC output $( "$OUT/pm" ); rel tol 1e-9)"
gcc -O3 -march=native bench-best/bt_best.c -o "$OUT/bc"
g++ -O3 -march=native bench-best/bt_best.cpp -o "$OUT/bcpp"
rustc -O -C target-cpu=native bench-best/bt_best.rs -o "$OUT/brs"
"$M" build bench-bt/bt_magic_loop.magic --backend c --cc "$C -O3 -march=native" -o "$OUT/bm" >/dev/null 2>&1
echo "bt:        C=$(timeit "$OUT/bc") C++=$(timeit "$OUT/bcpp") Rust=$(timeit "$OUT/brs") MAGIC=$(timeit "$OUT/bm")  (exact $( "$OUT/bm" ))"
