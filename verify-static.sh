#!/usr/bin/env bash
# Freestanding static gate: integer/bool programs must build into tiny static
# ELFs with no interpreter and no libc, using only as+ld.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
OUT=$(mktemp -d); trap 'rm -rf "$OUT"' EXIT
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
FAIL=0
check() {
  name=$1 src=$2 want=$3
  exe="$OUT/$name"
  if ! "$M" build "$src" --static -o "$exe" >/dev/null 2>&1; then
    echo "$name: build FAIL"; FAIL=1; return
  fi
  got=$("$exe" 2>/dev/null | tail -1)
  size=$(stat -c %s "$exe")
  interp=$(readelf -l "$exe" 2>/dev/null | grep -c 'INTERP' || true)
  if [ "$got" != "$want" ] || [ "$interp" -ne 0 ] || [ "$size" -gt 65536 ]; then
    echo "$name: FAIL got=$got size=$size interp=$interp"; FAIL=1
  else
    echo "$name: ok ${size}B static"
  fi
}
check sieve bench-suite/sieve_magic.magic 5761455
check matmul bench-suite/matmul_magic.magic 326316140
check hash bench-suite/hash_magic.magic 1500001500000
check fannkuch bench-fannkuch/fannkuch_magic.magic 38
check bt bench-bt/bt_magic.magic 568328208
if "$M" build bench-suite/nbody_magic.magic --static -o "$OUT/nb" >/dev/null 2>&1; then
  echo "float program unexpectedly accepted"; FAIL=1
else
  echo "float rejection: ok"
fi
[ "$FAIL" -eq 0 ] && echo "STATIC GATE PASSED" || { echo "STATIC GATE FAILED"; exit 1; }
