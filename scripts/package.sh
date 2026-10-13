#!/bin/sh
# Build release archives for arct.
#
#   scripts/package.sh [target ...]
#
# With no targets, builds for this computer. Writes
# dist/arct-<version>-<target>.tar.gz (arct, LICENSE, README.md) and
# dist/SHA256SUMS covering every archive in dist/. Publishing is a
# separate, deliberate step: see RELEASING.md.
set -eu

cd "$(dirname "$0")/.."
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[ -n "$version" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
[ "$#" -gt 0 ] || set -- "$(rustc -vV | sed -n 's/^host: //p')"

mkdir -p dist
for target in "$@"; do
    echo "Building arct $version for $target"
    case "$target" in
        *-linux-musl) cargo zigbuild --release --locked -p arct-cli --target "$target" ;;
        *) cargo build --release --locked -p arct-cli --target "$target" ;;
    esac
    name="arct-$version-$target"
    rm -rf "dist/$name"
    mkdir -p "dist/$name"
    cp "target/$target/release/arct" LICENSE README.md "dist/$name/"
    tar -C dist -czf "dist/$name.tar.gz" "$name"
    rm -rf "dist/$name"
done

cd dist
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum arct-*.tar.gz > SHA256SUMS
else
    shasum -a 256 arct-*.tar.gz > SHA256SUMS
fi
cat SHA256SUMS
