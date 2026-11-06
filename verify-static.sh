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
check hash_par bench-suite/hash_magic_par.magic 1500001500000
check fannkuch_blocks bench-fannkuch/fannkuch_magic_blocks.magic 38
check nbody bench-suite/nbody_magic.magic 11211.531383831
check particles bench-suite/particles_magic.magic 500094707846.643
check ray bench-suite/raytrace_magic.magic 336141.941
check ray_par bench-suite/raytrace_magic_par.magic 336141.941
IOBIN="$OUT/io.bin"; IOSRC="$OUT/io.magic"
cat > "$IOSRC" <<EOF
fn main() {
    var fd = file_open("$IOBIN", "wb");
    var b: [u8] = [65; 3];
    file_write(fd, b, 3);
    file_close(fd);
    var rf = file_open("$IOBIN", "rb");
    var c: [u8] = [0; 3];
    var n = file_read(rf, c, 3);
    print(n);
    print(c[0]);
    print(file_size(rf));
    file_close(rf);
}
EOF
if "$M" build "$IOSRC" --static -o "$OUT/io" >/dev/null 2>&1; then
  got=$("$OUT/io" 2>/dev/null | tr '\n' ' ')
  if [ "$got" = "3 65 3 " ]; then echo "file_io: ok static"; else echo "file_io: FAIL got=$got"; FAIL=1; fi
else
  echo "file_io: build FAIL"; FAIL=1
fi

NETSRC="$OUT/net.magic"
cat > "$NETSRC" <<'EOF'
fn main() {
    var s = net_listen(0);
    var p = net_port(s);
    var c = net_connect("127.0.0.1", p);
    var a = net_accept(s);
    var buf: [u8] = [65; 3];
    net_write(c, buf, 3);
    var r: [u8] = [0; 3];
    var n = net_read(a, r, 3);
    print(n);
    print(r[0]);
    net_close(c);
    net_close(a);
    net_close(s);
}
EOF
if "$M" build "$NETSRC" --static -o "$OUT/net" >/dev/null 2>&1; then
  got=$(timeout 20 "$OUT/net" 2>/dev/null | tr '\n' ' ')
  if [ "$got" = "3 65 " ]; then echo "net_loopback: ok static"; else echo "net_loopback: FAIL got=$got"; FAIL=1; fi
else
  echo "net_loopback: build FAIL"; FAIL=1
fi

ENVSRC="$OUT/env.magic"
cat > "$ENVSRC" <<'EOF'
fn main() {
    let v = env("MAGIC_STATIC_TEST");
    if str_eq(v, "yes") { print(1); } else { print(0); }
}
EOF
if "$M" build "$ENVSRC" --static -o "$OUT/envtest" >/dev/null 2>&1; then
  got=$(MAGIC_STATIC_TEST=yes "$OUT/envtest" 2>/dev/null | tail -1)
  if [ "$got" = "1" ]; then echo "env: ok static"; else echo "env: FAIL got=$got"; FAIL=1; fi
else
  echo "env: build FAIL"; FAIL=1
fi

cat > "$OUT/assert.magic" <<'EOF'
fn main() {
    assert(1 == 2);
}
EOF
if "$M" build "$OUT/assert.magic" --static -o "$OUT/assert" >/dev/null 2>&1; then
  "$OUT/assert" >/dev/null 2>"$OUT/ae"; rc=$?
  if [ "$rc" -ne 0 ] && grep -q ':2: assertion failed' "$OUT/ae" && grep -q 'stack trace:' "$OUT/ae"; then
    echo "assertion_trace: ok static"
  else
    echo "assertion_trace: FAIL rc=$rc"; FAIL=1
  fi
else
  echo "assertion_trace: build FAIL"; FAIL=1
fi

cat > "$OUT/pop.magic" <<'EOF'
fn main() {
    var v: [i32+] = [];
    v.push(7);
    var x = pop(v);
    print(x);
}
EOF
if "$M" build "$OUT/pop.magic" --static -o "$OUT/pop" >/dev/null 2>&1; then
  got=$("$OUT/pop" 2>/dev/null | tail -1)
  if [ "$got" = "7" ]; then echo "local_pop: ok static"; else echo "local_pop: FAIL got=$got"; FAIL=1; fi
else
  echo "local_pop: build FAIL"; FAIL=1
fi

cat > "$OUT/pspop.magic" <<'EOF'
struct P { x: i32, y: i32 }
fn take(v: [P+]) -> P { return pop(v); }
fn main() {
    var v: [P+] = [];
    v.push(P { x: 1, y: 2 });
    var p: P = take(v);
    print(p.x);
    print(len(v));
}
EOF
if "$M" build "$OUT/pspop.magic" --static -o "$OUT/pspop" >/dev/null 2>&1; then
  got=$("$OUT/pspop" 2>/dev/null | tr '\n' ' ')
  if [ "$got" = "1 0 " ]; then echo "struct_pop_param: ok static"; else echo "struct_pop_param: FAIL got=$got"; FAIL=1; fi
else
  echo "struct_pop_param: build FAIL"; FAIL=1
fi

cat > "$OUT/spop.magic" <<'EOF'
struct P { x: i32, y: i32 }
fn main() {
    var v: [P+] = [];
    v.push(P { x: 1, y: 2 });
    var p: P = pop(v);
    print(p.x);
    print(len(v));
}
EOF
if "$M" build "$OUT/spop.magic" --static -o "$OUT/spop" >/dev/null 2>&1; then
  got=$("$OUT/spop" 2>/dev/null | tr '\n' ' ')
  if [ "$got" = "1 0 " ]; then echo "struct_pop: ok static"; else echo "struct_pop: FAIL got=$got"; FAIL=1; fi
else
  echo "struct_pop: build FAIL"; FAIL=1
fi

cat > "$OUT/clear.magic" <<'EOF'
fn main() {
    var v: [i32+] = [];
    v.push(1);
    v.push(2);
    clear(v);
    print(len(v));
    v.push(9);
    print(v[0]);
}
EOF
if "$M" build "$OUT/clear.magic" --static -o "$OUT/clear" >/dev/null 2>&1; then
  got=$("$OUT/clear" 2>/dev/null | tr '\n' ' ')
  if [ "$got" = "0 9 " ]; then echo "local_clear: ok static"; else echo "local_clear: FAIL got=$got"; FAIL=1; fi
else
  echo "local_clear: build FAIL"; FAIL=1
fi

cat > "$OUT/popempty.magic" <<'EOF'
fn main() {
    var v: [i32+] = [];
    var x = pop(v);
    print(x);
}
EOF
if "$M" build "$OUT/popempty.magic" --static -o "$OUT/popempty" >/dev/null 2>&1; then
  "$OUT/popempty" >/dev/null 2>"$OUT/pee"; rc=$?
  if [ "$rc" -ne 0 ] && grep -q ':3: pop from empty vec' "$OUT/pee" && grep -q 'stack trace:' "$OUT/pee"; then
    echo "empty_pop: ok static"
  else
    echo "empty_pop: FAIL rc=$rc"; FAIL=1
  fi
else
  echo "empty_pop: build FAIL"; FAIL=1
fi

cat > "$OUT/div.magic" <<'EOF'
fn main() {
    var s = 0;
    for i in 0..4 {
        var z = i - 2;
        s = s + 10 / z;
    }
    print(s);
}
EOF
if "$M" build "$OUT/div.magic" --static -o "$OUT/div" >/dev/null 2>&1; then
  "$OUT/div" >/dev/null 2>"$OUT/de"; rc=$?
  if [ "$rc" -ne 0 ] && grep -q ':5: division by zero' "$OUT/de" && grep -q 'stack trace:' "$OUT/de"; then
    echo "division_zero: ok static"
  else
    echo "division_zero: FAIL rc=$rc"; FAIL=1
  fi
else
  echo "division_zero: build FAIL"; FAIL=1
fi

cat > "$OUT/panic.magic" <<'EOF'
fn main() {
    var a: [i32] = [1; 2];
    var i = 5;
    print(a[i]);
}
EOF
if "$M" build "$OUT/panic.magic" --static -o "$OUT/panic" >/dev/null 2>&1; then
  "$OUT/panic" >"$OUT/po" 2>"$OUT/pe"; rc=$?
  if [ "$rc" -ne 0 ] && grep -q 'index out of bounds' "$OUT/pe" \
     && grep -q 'stack trace:' "$OUT/pe" && grep -Eq '^[0-9a-f]{16}$' "$OUT/pe"; then
    echo "panic_trace: ok static"
  else
    echo "panic_trace: FAIL rc=$rc"; FAIL=1
  fi
else
  echo "panic_trace: build FAIL"; FAIL=1
fi

printf 'fn main() { let x = 1.5; print(x); }\n' > "$OUT/g.magic"
if "$M" build "$OUT/g.magic" --static -o "$OUT/g" >/dev/null 2>&1; then
  echo "%%g program unexpectedly accepted"; FAIL=1
else
  echo "%%g rejection: ok"
fi
[ "$FAIL" -eq 0 ] && echo "STATIC GATE PASSED" || { echo "STATIC GATE FAILED"; exit 1; }
