#!/bin/sh
# MAGIC installer (Linux / macOS).
#
#   curl -fsSL https://raw.githubusercontent.com/MaewGwak/MAGIC/main/install.sh | sh
#
# Installs magicc + magic into $MAGIC_PREFIX/bin (default: ~/.local/bin).
set -eu

repo="https://github.com/MaewGwak/MAGIC.git"
prefix="${MAGIC_PREFIX:-$HOME/.local}"

command -v cargo >/dev/null 2>&1 || {
    echo "Rust not found. Install it from https://rustup.rs" >&2
    exit 1
}

if ! command -v cc >/dev/null 2>&1 && ! command -v gcc >/dev/null 2>&1 && ! command -v clang >/dev/null 2>&1; then
    echo "No C compiler found (gcc/clang)." >&2
    exit 1
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd || true)
if [ -n "$script_dir" ] && [ -f "$script_dir/Cargo.toml" ]; then
    src="$script_dir"
else
    command -v git >/dev/null 2>&1 || {
        echo "git not found." >&2
        exit 1
    }
    tmp=$(mktemp -d)
    echo "cloning $repo ..."
    git clone --depth 1 "$repo" "$tmp/magic"
    src="$tmp/magic"
fi

echo "building ..."
(cd "$src" && cargo build --release)

mkdir -p "$prefix/bin"
cp "$src/target/release/magicc" "$prefix/bin/magicc"
cp "$src/target/release/magicc" "$prefix/bin/magic"
chmod +x "$prefix/bin/magicc" "$prefix/bin/magic"

"$prefix/bin/magic" --version
echo "installed: $prefix/bin"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "add this to your shell profile: export PATH=\"$prefix/bin:\$PATH\"" ;;
esac
