#!/usr/bin/env bash
# Linux/macOS benchmark matrix: C, Rust and the MAGIC variants, with checksums.
# Medians of five runs in milliseconds; exits 1 if any answer is wrong.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
OUT="$ROOT/target/bench-linux"
export PATH="${HOME}/.cargo/bin:${PATH}"
mkdir -p "$OUT"
FAIL=0

# clang is optional but gives MAGIC the LLVM backend. System clang wins; a
# user-space unpacked clang (tools/clangroot) is used when present.
CLANG=""
if command -v clang >/dev/null 2>&1; then
  CLANG=$(command -v clang)
elif [ -x "$HOME/tools/clangroot/usr/bin/clang" ]; then
  CLANG="$HOME/tools/clangroot/usr/bin/clang"
  export LD_LIBRARY_PATH="$HOME/tools/clangroot/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi

median() {
  local exe=$1 expect=$2 label=$3
  local vals=() ok=ok out s e
  for i in 1 2 3 4 5; do
    s=$(date +%s%N); out=$("$exe" 2>/dev/null); e=$(date +%s%N)
    vals+=($(( (e - s) / 1000000 )))
    [ "$out" = "$expect" ] || ok="WRONG:[$out]"
  done
  local med
  med=$(printf '%s\n' "${vals[@]}" | sort -n | sed -n '3p')
  printf '%-32s %6s ms  %s\n' "$label" "$med" "$ok"
  [ "$ok" = ok ] || FAIL=$((FAIL + 1))
}

build_c() { gcc -O3 -o "$OUT/$2" "$1" || { echo "build failed: $1"; FAIL=$((FAIL + 1)); }; }
build_rust() { rustc -C opt-level=3 -o "$OUT/$2" "$1" 2>/dev/null || { echo "build failed: $1"; FAIL=$((FAIL + 1)); }; }
build_magic() { "$M" build "$1" -o "$OUT/$2" >/dev/null 2>&1 || { echo "build failed: $1"; FAIL=$((FAIL + 1)); }; }

echo "== sieve 100M"
cd "$ROOT/bench-suite" || exit 2
build_c sieve_c.c s_c; build_rust sieve_rs.rs s_rs
build_magic sieve_magic.magic s_m; build_magic sieve_magic_odd.magic s_o
build_magic sieve_magic_odd_par.magic s_p
median "$OUT/s_c" 5761455 'sieve C'
median "$OUT/s_rs" 5761455 'sieve Rust'
median "$OUT/s_m" 5761455 'sieve MAGIC'
median "$OUT/s_o" 5761455 'sieve MAGIC odd'
median "$OUT/s_p" 5761455 'sieve MAGIC odd par'

echo "== fannkuch n=10"
cd "$ROOT/bench-fannkuch" || exit 2
EXP=$'73196\n38'
build_c fannkuch_c.c f_c; build_rust fannkuch_rs.rs f_rs
build_magic fannkuch_magic.magic f_m; build_magic fannkuch_magic_buf.magic f_b
median "$OUT/f_c" "$EXP" 'fannkuch C'
median "$OUT/f_rs" "$EXP" 'fannkuch Rust'
median "$OUT/f_m" "$EXP" 'fannkuch MAGIC'
median "$OUT/f_b" "$EXP" 'fannkuch MAGIC buf'

echo "== matmul 384"
cd "$ROOT/bench-suite" || exit 2
build_c matmul_c.c m_c; build_rust matmul_rs.rs m_rs
build_magic matmul_magic.magic m_m; build_magic matmul_magic_ikj.magic m_k
median "$OUT/m_c" 326316140 'matmul C'
median "$OUT/m_rs" 326316140 'matmul Rust'
median "$OUT/m_m" 326316140 'matmul MAGIC'
median "$OUT/m_k" 326316140 'matmul MAGIC i-k-j'

echo "== n-body 10M"
build_c nbody_c.c n_c; build_rust nbody_rs.rs n_rs
build_magic nbody_unrolled.magic n_u
median "$OUT/n_c" 11211.531383831 'n-body C'
median "$OUT/n_rs" 11211.531383831 'n-body Rust'
median "$OUT/n_u" 11211.531383831 'n-body MAGIC unrolled'
if [ -n "$CLANG" ]; then
  "$M" build nbody_unrolled.magic --cc "$CLANG -march=x86-64-v3" -o "$OUT/n_clang.bin" >/dev/null 2>&1 || true
  [ -x "$OUT/n_clang.bin" ] && median "$OUT/n_clang.bin" 11211.531383844 'n-body MAGIC clang AVX2'
fi

echo "== ray 1536x1536"
build_c raytrace_c.c r_c; build_rust raytrace_rs.rs r_rs
build_magic raytrace_magic.magic r_m; build_magic raytrace_magic_par.magic r_p
median "$OUT/r_c" 336141.941 'ray C'
median "$OUT/r_rs" 336141.941 'ray Rust'
median "$OUT/r_m" 336141.941 'ray MAGIC'
median "$OUT/r_p" 336141.941 'ray MAGIC par'

echo "== hash 1M"
build_c hash_c.c h_c; build_rust hash_rs_open.rs h_rs
build_magic hash_magic.magic h_m; build_magic hash_magic_direct.magic h_d
median "$OUT/h_c" 1500001500000 'hash C'
median "$OUT/h_rs" 1500001500000 'hash Rust'
median "$OUT/h_m" 1500001500000 'hash MAGIC'
median "$OUT/h_d" 1500001500000 'hash MAGIC direct'

echo "== particles 1M x 100"
build_c particles_c.c p_c; build_rust particles_rs.rs p_rs
build_magic particles_magic.magic p_m; build_magic particles_magic_closed.magic p_x
median "$OUT/p_c" 500094707846.643 'particles C'
median "$OUT/p_rs" 500094707846.643 'particles Rust'
median "$OUT/p_m" 500094707846.643 'particles MAGIC'
median "$OUT/p_x" 500094707846.643 'particles MAGIC closed'

echo "== binary trees"
cd "$ROOT/bench-bt" || exit 2
build_c bt_c.c b_c; build_rust bt_rust_flat.rs b_rs
build_magic bt_magic.magic b_m; build_magic bt_magic_count.magic b_x
median "$OUT/b_c" 568328208 'binary trees C'
median "$OUT/b_rs" 568328208 'binary trees Rust'
median "$OUT/b_m" 568328208 'binary trees MAGIC'
median "$OUT/b_x" 568328208 'binary trees MAGIC count'

echo
if [ "$FAIL" -ne 0 ]; then
  echo "$FAIL benchmark(s) failed"
  exit 1
fi
echo "all benchmarks passed"
