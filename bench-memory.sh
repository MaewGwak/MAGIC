#!/usr/bin/env bash
# Compares each benchmark's compiler-reported static memory bound with peak RSS.
# Exits 1 if a program's peak RSS is below its reported bound (a broken bound).
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
OUT="$ROOT/target/bench-linux"
export PATH="${HOME}/.cargo/bin:${PATH}"
mkdir -p "$OUT"

CASES=(
  "sieve 100M|bench-suite|sieve_magic.magic"
  "fannkuch|bench-fannkuch|fannkuch_magic.magic"
  "matmul 384|bench-suite|matmul_magic.magic"
  "n-body 10M|bench-suite|nbody_magic.magic"
  "ray tracer|bench-suite|raytrace_magic.magic"
  "hash 1M|bench-suite|hash_magic.magic"
  "binary trees|bench-bt|bt_magic.magic"
)

printf '%-16s %10s %8s %10s %s\n' Benchmark ReportedMB PeakMB Overhead OK
fail=0
for spec in "${CASES[@]}"; do
  IFS='|' read -r name dir src <<< "$spec"
  bin="$OUT/mem-$(printf '%s' "$name" | tr ' /' '__').bin"
  rm -f "$bin"
  ( cd "$ROOT/$dir" && "$M" build "$src" -o "$bin" >/dev/null 2>&1 )
  report=$("$M" check "$ROOT/$dir/$src" --report 2>/dev/null)
  bound=$(printf '%s' "$report" | grep -oE 'main <= [0-9]+ bytes' | head -1 | grep -oE '[0-9]+')
  if [ -z "$bound" ]; then
    printf '%-16s %10s %8s %10s %s\n' "$name" 'n/a' '' '' 'NO BOUND'
    fail=$((fail + 1))
    continue
  fi
  /usr/bin/time -v "$bin" >/dev/null 2>/tmp/memtime.$$
  peakkb=$(grep 'Maximum resident set size' /tmp/memtime.$$ | grep -oE '[0-9]+')
  rm -f /tmp/memtime.$$
  boundmb=$(awk -v b="$bound" 'BEGIN { printf "%.1f", b / 1048576 }')
  peakmb=$(awk -v k="$peakkb" 'BEGIN { printf "%.1f", k / 1024 }')
  overhead=$(awk -v p="$peakmb" -v b="$boundmb" 'BEGIN { printf "%.1f", p - b }')
  ok=$(awk -v p="$peakmb" -v b="$boundmb" 'BEGIN { print (p >= b * 0.95) ? "ok" : "PEAK < BOUND" }')
  [ "$ok" = ok ] || fail=$((fail + 1))
  printf '%-16s %10s %8s %10s %s\n' "$name" "$boundmb" "$peakmb" "$overhead" "$ok"
done

echo
if [ "$fail" -ne 0 ]; then
  echo "$fail case(s) failed"
  exit 1
fi
echo 'every reported bound is <= measured peak RSS'
