#!/usr/bin/env bash
# Compile-speed comparison: cold build, one reachable-function edit, no-op build.
# Generates the same 2,000-function program in MAGIC, Rust and Goose.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
M="$ROOT/target/release/magicc"
export PATH="$HOME/.cargo/bin:$HOME/tools/goose/build:$HOME/tools/clangroot/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HOME/tools/clangroot/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
WORK=${TMPDIR:-/tmp}/magic-compile-bench
rm -rf "$WORK" && mkdir -p "$WORK" && cd "$WORK" || exit 2
[ -x "$M" ] || { echo "build magicc first"; exit 2; }
command -v rustc >/dev/null || { echo "rustc missing"; exit 2; }
GOOSE=$(command -v goose || true)

python3 - <<'PY'
m=[f"fn fnv{i}(x: i32) -> i32 {{ return x + {i}; }}" for i in range(2000)]
m += ["","fn main() {","    var t = 0;"]
for i in range(0,2000,100):
    m.append(f"    t = t + fnv{i}(t);")
m += ["    print(t);","}"]
open("big.magic","w").write("\n".join(m)+"\n")

r=[f"#[inline(never)] fn fnv{i}(x: i64) -> i64 {{ x + {i} }}" for i in range(2000)]
r += ["fn main() {","    let mut t: i64 = 0;"]
for i in range(0,2000,100):
    r.append(f"    t = t + fnv{i}(t);")
r += ['    println!("{}", t);',"}"]
open("big.rs","w").write("\n".join(r)+"\n")

g=["import std;",""]
for i in range(2000):
    g.append(f"fn fnv{i}(x: i64) -> i64 {{ x + {i} }}")
g += ["fn main() {","    var t: i64 = 0;"]
for i in range(0,2000,100):
    g.append(f"    t = t + fnv{i}(t);")
g += ['    print(t);',"}"]
open("big.goose","w").write("\n".join(g)+"\n")
PY

median() { # label runs cmd...
  local label=$1 runs=$2; shift 2
  local vals=()
  for _ in $(seq "$runs"); do
    local s=$(date +%s%N); "$@" >/dev/null 2>&1; local e=$(date +%s%N)
    vals+=($(( (e - s) / 1000000 )))
  done
  printf '%-26s %6s ms\n' "$label" "$(printf '%s\n' "${vals[@]}" | sort -n | sed -n '2p')"
}

cold_magic_gcc() { rm -f big.c big.bin big.flags; "$M" build big.magic --backend c --cc gcc; }
cold_magic_clang() { rm -f big.c big.bin big.flags; "$M" build big.magic --backend c --cc clang; }
edit_magic_gcc() { sed -i 's/return x + 0;/return x + 5;/' big.magic; "$M" build big.magic --backend c --cc gcc; }
edit_magic_clang() { sed -i 's/return x + 5;/return x + 9;/' big.magic; "$M" build big.magic --backend c --cc clang; }
cold_rust() { rm -f big_r; rustc -O big.rs -o big_r; }
edit_rust() { sed -i 's/x + 0 }/x + 7 }/' big.rs; rustc -O big.rs -o big_r; }

echo "== cold =="
median "MAGIC gcc" 3 cold_magic_gcc
median "MAGIC clang" 3 cold_magic_clang
median "rustc -O" 3 cold_rust
if [ -n "$GOOSE" ]; then median "goose build" 3 "$GOOSE" build big.goose -o big_g; fi
echo "== one reachable edit =="
median "MAGIC gcc" 3 edit_magic_gcc
median "MAGIC clang" 3 edit_magic_clang
median "rustc -O (full)" 3 edit_rust
if [ -n "$GOOSE" ]; then
  edit_goose() { sed -i 's/x + 0 }/x + 4 }/' big.goose; "$GOOSE" build big.goose -o big_g; }
  median "goose build" 3 edit_goose
fi
echo "== no-op =="
median "MAGIC gcc" 3 "$M" build big.magic --backend c --cc gcc
if [ -n "$GOOSE" ]; then median "goose build" 3 "$GOOSE" build big.goose -o big_g; fi
