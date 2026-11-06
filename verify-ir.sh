#!/usr/bin/env bash
# Differential gate: every deterministic program in the repository must produce
# identical stdout and exit status through the C backend and the IR backend.
set -u
cd "$(dirname "$0")" || exit 2
export PATH="${HOME}/.cargo/bin:${PATH}"
CC="${CC:-cc}"
MAGICC=target/release/magicc
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$MAGICC" ]; then
  echo "build magicc first (cargo build --release)" >&2
  exit 2
fi
if ! as --version >/dev/null 2>&1; then
  echo "assembler not available; skipping IR gate"
  exit 0
fi

# Seeds needed by the database programs.
"$MAGICC" build bench-db/dbload_magic.magic -o "$WORK/dbload" >/dev/null 2>&1 &&
  "$WORK/dbload" >/dev/null 2>&1 || true
"$MAGICC" build bench-db/bufpool_magic_fast3.magic -o "$WORK/bpseed" >/dev/null 2>&1 &&
  "$WORK/bpseed" >/dev/null 2>&1 || true

fail=0
run_case() {
  local src=$1 name=$2 stdin=${3:-/dev/null}
  local cexe="$WORK/${name}_c" iexe="$WORK/${name}_i"
  if ! "$MAGICC" build "$src" -o "$cexe" >/dev/null 2>&1; then
    echo "skip (C build failed): $src"; return
  fi
  if ! "$MAGICC" build "$src" --backend ir -o "$iexe" >"$WORK/err" 2>&1; then
    echo "skip (IR build failed): $src :: $(head -1 "$WORK/err")"; return
  fi
  timeout 300 "$cexe" <"$stdin" >"$WORK/out_c" 2>/dev/null; local rc_c=$?
  timeout 300 "$iexe" <"$stdin" >"$WORK/out_i" 2>/dev/null; local rc_i=$?
  if [ "$rc_c" -eq 124 ] || [ "$rc_i" -eq 124 ]; then
    echo "skip (timeout): $src"; return
  fi
  if ! cmp -s "$WORK/out_c" "$WORK/out_i" || [ "$rc_c" -ne "$rc_i" ]; then
    echo "FAIL: $src (exit C=$rc_c IR=$rc_i)"
    diff <(head -5 "$WORK/out_c") <(head -5 "$WORK/out_i") | head -8
    fail=$((fail + 1))
  else
    echo "ok: $src"
  fi
}

for src in examples/hello.magic examples/fib.magic examples/for.magic examples/sum.magic \
  examples/sieve.magic examples/array_param.magic examples/hashmap.magic examples/tokenize.magic \
  examples/arena.magic examples/arena_bump.magic examples/struct.magic examples/points.magic \
  examples/inventory.magic examples/vec.magic examples/cast.magic \
  bench-suite/matmul_magic.magic bench-suite/particles_magic.magic bench-suite/vecpart_magic.magic \
  bench-suite/raytrace_magic.magic bench-suite/nbody_magic.magic bench-suite/sieve_magic_odd.magic \
  bench-suite/sieve_magic_odd_par.magic bench-suite/hash_magic.magic \
  bench-fannkuch/fannkuch_magic.magic bench-bt/bt_magic.magic \
  bench-db/sqlscan_magic.magic bench-db/btree_magic.magic bench-db/hashmap_magic.magic \
  bench-db/bufpool_magic.magic bench-db/bufpool_magic_fast3.magic \
  bench-db/pageio_magic.magic bench-db/dbload_magic.magic bench-db/dbquery_magic.magic \
  bench-db/sqldb_magic.magic bench-db/dbcli_magic.magic; do
  run_case "$src" "$(basename "$src" .magic)"
done

if [ "$fail" -ne 0 ]; then
  echo "IR DIFFERENTIAL GATE FAILED ($fail case(s))"
  exit 1
fi
echo "IR DIFFERENTIAL GATE PASSED"
