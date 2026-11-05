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

# TCP: a native server and a native client, still with no C compiler.
cat > "$WORK/srv.magic" <<'MAGIC'
fn main() {
    var lfd = net_listen(0);
    print(net_port(lfd));
    flush();
    var c = net_accept(lfd);
    var b: [u8] = [0; 8];
    var n = net_read(c, b, 4);
    print(n);
    var o: [u8] = [0; 4];
    o[0] = 112; o[1] = 111; o[2] = 110; o[3] = 103;
    net_write(c, o, 4);
    net_close(c);
    net_close(lfd);
}
MAGIC
cat > "$WORK/cli.magic" <<'MAGIC'
fn atoi(s: str) -> i32 {
    var v = 0;
    var i = 0;
    while i < str_len(s) {
        v = v * 10 + ((str_byte(s, i) as i32) - 48);
        i = i + 1;
    }
    return v;
}
fn main() {
    var fd = net_connect("127.0.0.1", atoi(arg(1)));
    var o: [u8] = [0; 4];
    o[0] = 112; o[1] = 105; o[2] = 110; o[3] = 103;
    net_write(fd, o, 4);
    var b: [u8] = [0; 8];
    var n = net_read(fd, b, 4);
    print(n);
    print(b[0]);
    net_close(fd);
}
MAGIC
if env -i PATH="$SAFE" "$M" build "$WORK/srv.magic" --backend native -o "$WORK/srv" >/dev/null 2>"$WORK/srv.err" &&
   env -i PATH="$SAFE" "$M" build "$WORK/cli.magic" --backend native -o "$WORK/cli" >/dev/null 2>"$WORK/cli.err"; then
  "$WORK/srv" >"$WORK/srv.out" 2>&1 &
  srv_pid=$!
  for _ in $(seq 1 50); do [ -s "$WORK/srv.out" ] && break; sleep 0.1; done
  port=$(head -1 "$WORK/srv.out")
  client_out=$("$WORK/cli" "$port" 2>&1)
  wait "$srv_pid" 2>/dev/null
  if [ "$client_out" != $'4\n112' ]; then
    echo "FAIL(socket client): [$client_out]"; fail=1
  elif ! grep -q '^4$' "$WORK/srv.out"; then
    echo "FAIL(socket server): [$(cat "$WORK/srv.out")]"; fail=1
  else
    echo "ok native TCP server/client"
  fi
else
  echo "FAIL(build sockets): $(tail -1 "$WORK/srv.err" "$WORK/cli.err")"; fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "NATIVE GATE FAILED"
  exit 1
fi
echo "NATIVE GATE PASSED"
