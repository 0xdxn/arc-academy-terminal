#!/bin/sh
# arct installer: https://arcacademy.sh
#
#   curl -fsSL https://arcacademy.sh/install.sh | sh
#
# Downloads a prebuilt arct for this computer, checks it against the
# published SHA-256 sums, and puts it in ~/.local/bin. No admin rights,
# nothing sent anywhere except the download itself. If there is no
# prebuilt binary for this computer and Rust is already installed, it
# builds arct with cargo instead. It never installs Rust without asking.
#
# Settings (optional, as environment variables):
#   ARCT_VERSION        version to install (default: the release this script ships with)
#   ARCT_INSTALL_DIR    where to put arct (default: ~/.local/bin)
#   ARCT_DOWNLOAD_BASE  where releases live (default: https://arcacademy.sh/releases)

set -eu

VERSION="${ARCT_VERSION:-1.1.0}"
INSTALL_DIR="${ARCT_INSTALL_DIR:-$HOME/.local/bin}"
BASE="${ARCT_DOWNLOAD_BASE:-https://arcacademy.sh/releases}"

say() { printf '%s\n' "$*"; }
fail() { printf 'arct install: %s\n' "$*" >&2; exit 1; }

# Ask a yes/no question on the terminal (works under `curl | sh`); default no
ask() {
    if [ -r /dev/tty ]; then
        printf '%s [y/N] ' "$1" > /dev/tty
        read -r answer < /dev/tty || answer=""
        case "$answer" in y|Y|yes|YES) return 0 ;; esac
    fi
    return 1
}

fetch() {
    # Plain http only for a local test server; everything else is https
    case "$1" in
        http://127.0.0.1:*|http://localhost:*) proto="=http,https" ;;
        *) proto="=https" ;;
    esac
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto "$proto" --tlsv1.2 "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q --https-only "$1" -O "$2"
    else
        fail "needs curl or wget to download"
    fi
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        fail "needs sha256sum or shasum to check the download"
    fi
}

target=""
case "$(uname -s)" in
    Darwin)
        case "$(uname -m)" in
            arm64|aarch64) target="aarch64-apple-darwin" ;;
            x86_64) target="x86_64-apple-darwin" ;;
        esac ;;
    Linux)
        case "$(uname -m)" in
            x86_64|amd64) target="x86_64-unknown-linux-musl" ;;
            aarch64|arm64) target="aarch64-unknown-linux-musl" ;;
        esac ;;
esac

say "arct $VERSION installer"
say ""

install_prebuilt() {
    [ -n "$target" ] || return 1
    archive="arct-$VERSION-$target.tar.gz"
    work="$(mktemp -d 2>/dev/null || mktemp -d -t arct)"
    trap 'rm -rf "$work"' EXIT

    say "Downloading $archive"
    fetch "$BASE/v$VERSION/$archive" "$work/$archive" || return 1
    fetch "$BASE/v$VERSION/SHA256SUMS" "$work/SHA256SUMS" || fail "could not download SHA256SUMS to check the file"

    expected="$(grep " $archive\$" "$work/SHA256SUMS" | cut -d' ' -f1)"
    [ -n "$expected" ] || fail "$archive is not listed in SHA256SUMS"
    actual="$(sha256_of "$work/$archive")"
    [ "$expected" = "$actual" ] || fail "checksum mismatch for $archive: refusing to install"
    say "Checksum OK"

    tar -xzf "$work/$archive" -C "$work"
    mkdir -p "$INSTALL_DIR"
    cp "$work/arct-$VERSION-$target/arct" "$INSTALL_DIR/arct.new"
    chmod 755 "$INSTALL_DIR/arct.new"
    mv "$INSTALL_DIR/arct.new" "$INSTALL_DIR/arct"
    say "Installed $INSTALL_DIR/arct"
    return 0
}

install_with_cargo() {
    command -v cargo >/dev/null 2>&1 || return 1
    say "Building arct $VERSION with cargo (this takes a few minutes)"
    cargo install --locked arct-cli --version "$VERSION"
}

if install_prebuilt; then
    :
elif install_with_cargo; then
    INSTALL_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
else
    say "There's no prebuilt arct for this computer ($(uname -s) $(uname -m)),"
    say "and Rust isn't installed, so arct can't be built here yet."
    say ""
    if ask "Install Rust from https://rustup.rs and build arct? (about 1 GB, several minutes)"; then
        fetch https://sh.rustup.rs "${TMPDIR:-/tmp}/rustup-init.sh"
        sh "${TMPDIR:-/tmp}/rustup-init.sh" -y --profile minimal
        rm -f "${TMPDIR:-/tmp}/rustup-init.sh"
        # shellcheck disable=SC1091
        . "$HOME/.cargo/env"
        install_with_cargo || fail "cargo could not build arct"
        INSTALL_DIR="$HOME/.cargo/bin"
    else
        say "Nothing installed. See https://arcacademy.sh/docs for other ways to get arct."
        exit 1
    fi
fi

say ""
case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        say "Done. Start with:  arct" ;;
    *)
        say "Done. $INSTALL_DIR isn't on your PATH yet. Add it with:"
        say ""
        case "$(basename "${SHELL:-sh}")" in
            zsh)  say "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc && exec zsh" ;;
            bash) say "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc && exec bash" ;;
            fish) say "  fish_add_path $INSTALL_DIR" ;;
            *)    say "  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
        esac
        say ""
        say "Then start with:  arct" ;;
esac
