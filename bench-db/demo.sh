#!/usr/bin/env bash
# Functional check for the magic-db CLI: run SQL, then query the same file from
# a second process. Exits non-zero on a wrong answer.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
M="$ROOT/target/release/magicc"
cd "$ROOT" || exit 2
[ -x "$M" ] || { echo "build magicc first (cargo build --release)"; exit 2; }
"$M" build bench-db/dbcli_magic.magic -o bench-db/dbcli_magic >/dev/null || exit 1
rm -f bench-db/dbcli.dat

got1=$(./bench-db/dbcli_magic bench-db/demo.sql)
want1=$'2\n210\n140'
got2=$(printf 'SELECT COUNT(*) FROM t;\nSELECT value FROM t WHERE id = 1;\n' | ./bench-db/dbcli_magic)
want2=$'2\n70'

[ "$got1" = "$want1" ] || { echo "run 1 mismatch: [$got1]"; exit 1; }
[ "$got2" = "$want2" ] || { echo "run 2 mismatch: [$got2]"; exit 1; }
echo "cli demo ok"
