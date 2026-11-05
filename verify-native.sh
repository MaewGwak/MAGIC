#!/usr/bin/env bash
# Native-backend gate. Proves the native path needs no C compiler:
#  - builds every corpus program with a PATH that contains only `as` and `ld`
#  - compares stdout with the C backend's output
#  - checks the executable is x86-64 ELF and that an out-of-bounds index exits 1
# Fails (never skips) on a missing tool or a wrong answer. Linux x86-64 only.
set -u
ROOT=$(cd "$(dirname "$0")" && pwd)
M="$ROOT/target/release/magicc"
cd "$ROOT" || exit 2
[ -x "$M" ] || { echo "build magicc first (cargo build --release)"; exit 2; }
[ "$(uname -s)" = "Linux" ] && [ "$(uname -m)" = "x86_64" ] || {
  echo "native gate needs Linux x86-64"; exit 2;
}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/bin"
for tool in as ld; do
  path=$(command -v "$tool") || { echo "missing $tool"; exit 2; }
  ln -s "$path" "$WORK/bin/$tool"
done
# The native build PATH contains only the assembler and linker.
SAFE="$WORK/bin"
for forbidden in cc gcc clang cc1 collect2; do
  if env -i PATH="$SAFE" "$forbidden" --version >/dev/null 2>&1; then
    echo "FAIL: $forbidden is reachable from the native PATH"
    exit 1
  fi
done

fail=0
run_case() {
  local src=$1
  local name
  name=$(echo "$src" | tr '/' '_')
  local c_exe="$WORK/${name}_c"
  local n_src="$WORK/${name}.magic"
  local n_exe="$WORK/${name}_n"
  cp "$src" "$n_src"
  if ! "$M" build "$src" -o "$c_exe" >/dev/null 2>&1; then
    echo "FAIL(build C) $src"; fail=1; return
  fi
  local golden
  golden=$("$c_exe" 2>/dev/null)
  local cstatus=$?
  if ! env -i PATH="$SAFE" "$M" build "$n_src" --backend native -o "$n_exe" >/dev/null 2>"$WORK/err"; then
    echo "FAIL(build native, no cc) $src: $(tail -1 "$WORK/err")"; fail=1; return
  fi
  local got
  got=$("$n_exe" 2>/dev/null)
  local nstatus=$?
  if [ "$got" != "$golden" ]; then
    echo "FAIL(output) $src"; fail=1; return
  fi
  if [ "$cstatus" != "$nstatus" ]; then
    echo "FAIL(exit) $src: C=$cstatus native=$nstatus"; fail=1; return
  fi
  if command -v readelf >/dev/null 2>&1; then
    if ! readelf -h "$n_exe" | grep -q 'X86-64'; then
      echo "FAIL(elf) $src"; fail=1; return
    fi
  fi
  echo "ok $src"
}
while IFS= read -r src; do
  [ -n "$src" ] && run_case "$src"
done < tests/native/corpus.txt

# Out-of-bounds access must print the diagnostic and exit 1.
cat > "$WORK/oob.magic" <<'MAGIC'
fn main() {
    var a: [i32] = [0; 4];
    print(a[7]);
}
MAGIC
if env -i PATH="$SAFE" "$M" build "$WORK/oob.magic" --backend native -o "$WORK/oob" >/dev/null 2>"$WORK/oob.err"; then
  "$WORK/oob" >"$WORK/oob.out" 2>"$WORK/oob.run.err"
  status=$?
  if [ "$status" -ne 1 ] || ! grep -q 'index out of bounds' "$WORK/oob.run.err"; then
    echo "FAIL(bounds check): exit=$status"; fail=1
  else
    echo "ok out-of-bounds diagnostic"
  fi
else
  echo "FAIL(build oob): $(tail -1 "$WORK/oob.err")"; fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "NATIVE GATE FAILED"
  exit 1
fi
echo "NATIVE GATE PASSED"
