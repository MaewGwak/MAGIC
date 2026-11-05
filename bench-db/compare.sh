#!/usr/bin/env bash
# Rebuilds the database-component benchmarks in C, Rust and MAGIC, checks every
# answer and prints medians of three runs. Exits non-zero on a wrong answer.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
M="$ROOT/target/release/magicc"
cd "$ROOT" || exit 2
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
command -v rustc >/dev/null || { echo "rustc not found"; exit 2; }

build() { # name source compiler...
  "$@" >/dev/null 2>&1 || { echo "build failed: $1"; exit 1; }
}
gcc -O3 -o bench-db/pageio_c bench-db/pageio_c.c
rustc -C opt-level=3 -o bench-db/pageio_rs bench-db/pageio_rs.rs
"$M" build bench-db/pageio_magic.magic -o bench-db/pageio_magic >/dev/null
"$M" build bench-db/pageio_magic_fast.magic -o bench-db/pageio_magic_fast >/dev/null
gcc -O3 -o bench-db/btree_c bench-db/btree_c.c
rustc -C opt-level=3 -o bench-db/btree_rs bench-db/btree_rs.rs
"$M" build bench-db/btree_magic.magic -o bench-db/btree_magic >/dev/null
gcc -O3 -o bench-db/sqlscan_c bench-db/sqlscan_c.c
rustc -C opt-level=3 -o bench-db/sqlscan_rs bench-db/sqlscan_rs.rs
"$M" build bench-db/sqlscan_magic.magic -o bench-db/sqlscan_magic >/dev/null
gcc -O3 -o bench-db/bufpool_c bench-db/bufpool_c.c
rustc -C opt-level=3 -o bench-db/bufpool_rs bench-db/bufpool_rs.rs
"$M" build bench-db/bufpool_magic.magic -o bench-db/bufpool_magic >/dev/null
gcc -O3 -o bench-db/hashmap_c bench-db/hashmap_c.c
rustc -C opt-level=3 -o bench-db/hashmap_rs bench-db/hashmap_rs.rs
"$M" build bench-db/hashmap_magic.magic -o bench-db/hashmap_magic >/dev/null

median() { # label expected binary
  local label=$1 expect=$2 bin=$3 vals=() out
  for i in 1 2 3; do
    local s=$(date +%s%N); out=$("$bin"); local t=$(date +%s%N)
    vals+=($(( (t - s) / 1000000 )))
    [ "$out" = "$expect" ] || { echo "$label WRONG: [$out]"; exit 1; }
  done
  printf '%-22s %6s ms\n' "$label" "$(printf '%s\n' "${vals[@]}" | sort -n | sed -n '2p')"
}

median "pageio C"        "6367960"                 ./bench-db/pageio_c
median "pageio Rust"     "6367960"                 ./bench-db/pageio_rs
median "pageio MAGIC"    "6367960"                 ./bench-db/pageio_magic
median "pageio MAGIC fast" "6367960"               ./bench-db/pageio_magic_fast
median "btree C"         $'300002416362516\n4213'  ./bench-db/btree_c
median "btree Rust"      $'300002416362516\n4213'  ./bench-db/btree_rs
median "btree MAGIC"     $'300002416362516\n4213'  ./bench-db/btree_magic
median "sqlscan C"       $'1000000\n39999600000'   ./bench-db/sqlscan_c
median "sqlscan Rust"    $'1000000\n39999600000'   ./bench-db/sqlscan_rs
median "sqlscan MAGIC"   $'1000000\n39999600000'   ./bench-db/sqlscan_magic
median "bufpool C"       $'24997\n175003\n25510321' ./bench-db/bufpool_c
median "bufpool Rust"    $'24997\n175003\n25510321' ./bench-db/bufpool_rs
median "bufpool MAGIC"   $'24997\n175003\n25510321' ./bench-db/bufpool_magic
median "hashmap C"       "1500001500000"           ./bench-db/hashmap_c
median "hashmap Rust"    "1500001500000"           ./bench-db/hashmap_rs
median "hashmap MAGIC"   "1500001500000"           ./bench-db/hashmap_magic
