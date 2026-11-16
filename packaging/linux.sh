#!/usr/bin/env bash
# A Linux archive from a release build:  bash packaging/linux.sh <version or tag>
# Run from the repository root after `cargo build --release -p linnea-cli -p linnea-app` and the UI build.
set -euo pipefail
ver="${1:-}"
ver="${ver#v}"
[[ "$ver" =~ ^[0-9] ]] || ver="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
target="${CARGO_TARGET_DIR:-target}/release"

name="Linnea-$ver-linux-x64"
root="dist/$name"
rm -rf "$root" && mkdir -p "$root/bin" "$root/ui"
cp "$target/linnea-app" "$target/linnea" "$root/bin/"
cp -R library skill README.md LICENSE "$root/"
cp -R ui/dist "$root/ui/dist"
cp assets/icon.png "$root/linnea.png"
cp packaging/linux-install.sh "$root/install.sh"
chmod +x "$root/install.sh" "$root/bin/"*
tar -C dist -czf "dist/$name.tar.gz" "$name"
echo "made dist/$name.tar.gz"
