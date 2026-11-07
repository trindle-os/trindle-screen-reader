#!/bin/sh
# Builds the screen reader as a Trindle plugin (interface 1) from a scratch copy
set -eu

top=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$top/dist}
toolkit=${TRINDLE_TOOLKIT_COMMON:-$top/../trindle-toolkit-public/trindle-common}
id=$(sed -n 's/^Id=//p' "$top/packaging/plugin.ini")
version=$(sed -n 's/^Version=//p' "$top/packaging/plugin.ini")
cargo_version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$top/Cargo.toml" | head -n 1)
if [ "$version" != "$cargo_version" ]; then
    echo "plugin.ini says $version but Cargo.toml says $cargo_version" >&2
    exit 1
fi
[ -f "$toolkit/Cargo.toml" ] || { echo "no toolkit at $toolkit (set TRINDLE_TOOLKIT_COMMON)" >&2; exit 1; }

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
(cd "$top" && git ls-files -z -co --exclude-standard | tar --null -T - -cf -) | tar -xf - -C "$scratch"
printf '\n[patch."https://github.com/trindle-os/trindle-toolkit"]\ntrindle-common = { path = "%s" }\n' "$toolkit" >> "$scratch/Cargo.toml"
sed -i '/^source = "git+https:\/\/github.com\/trindle-os\/trindle-toolkit/d' "$scratch/Cargo.lock"

cargo_home=${CARGO_HOME:-$HOME/.cargo}
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home/user --remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$cargo_home/registry/src=/crates --remap-path-prefix=$cargo_home/git/checkouts=/crates --remap-path-prefix=$scratch=/src --remap-path-prefix=$top=/src"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$top/target/shipped}
dbus-run-session -- cargo test --release --locked --manifest-path "$scratch/Cargo.toml"
cargo build --release --locked --manifest-path "$scratch/Cargo.toml"

mkdir -p "$out"
dir=$out/$id
rm -rf "$dir" "$dir.tar.zst" "$dir.description.txt" "$dir.svg"
mkdir -p "$dir/bin"
install -m 0755 "$CARGO_TARGET_DIR/release/trindle-screen-reader" "$dir/bin/trindle-screen-reader"
install -m 0644 "$top/packaging/plugin.ini" "$top/LICENSE" "$top/share/licences/THIRD-PARTY.txt" "$top/packaging/screen-reader.svg" "$dir/"
install -m 0644 "$top/packaging/description.txt" "$dir.description.txt"
install -m 0644 "$top/packaging/screen-reader.svg" "$dir.svg"

if [ -n "$(find "$dir" -type l)" ]; then echo "the folder holds a symbolic link" >&2; exit 1; fi
[ -x "$dir/bin/trindle-screen-reader" ] || { echo "bin/trindle-screen-reader is not an executable file" >&2; exit 1; }
for program in "$dir"/bin/*; do
    extra=$(ldd "$program" | awk '{print $1}' | grep -v -e '^linux-vdso' -e '^/lib64/ld-linux' -e '^libc\.so' -e '^libm\.so' -e '^libgcc_s\.so' || true)
    [ -z "$extra" ] || { echo "$program needs $extra, which would have to be carried in the folder" >&2; exit 1; }
done
[ "$(du -sm "$dir" | cut -f1)" -lt 500 ] || { echo "the plugin is over 500 MB" >&2; exit 1; }

tar --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 \
    -C "$out" -cf - "$id" | zstd -q -19 -o "$dir.tar.zst"

echo "$id $version: $(du -sh "$dir" | cut -f1) as a folder, $(du -h "$dir.tar.zst" | cut -f1) to download"
