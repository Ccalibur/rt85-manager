#!/usr/bin/env bash
# Build release binaries inside the Freedesktop 24.08 SDK (glibc 2.40) so they
# run on any distro with glibc >= 2.40: Fedora 41+, Arch, etc. Building on a
# rolling distro would make them require its newer glibc.
#
# The SDK's own Rust is too old for libcosmic, so an official rustup toolchain
# is installed into target/sdk-toolchain (inside the SDK sandbox).
#
# Needs (user install is fine):  flatpak install flathub org.freedesktop.Sdk//24.08
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK="runtime/org.freedesktop.Sdk/x86_64/24.08"
TC="$ROOT/target/sdk-toolchain"
mkdir -p "$TC"

flatpak run --share=network --filesystem="$ROOT" \
  --env=RUSTUP_HOME="$TC/rustup" --env=CARGO_HOME="$TC/cargo" \
  --command=bash "$SDK" -c "
    set -euo pipefail
    export PATH=\"$TC/cargo/bin:\$PATH\"
    if ! command -v cargo >/dev/null; then
      curl -fsSL -o '$TC/rustup-init' https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init
      chmod +x '$TC/rustup-init'
      '$TC/rustup-init' -y --no-modify-path --profile minimal --default-toolchain stable
    fi
    rustup update stable >/dev/null
    cd '$ROOT'
    ldd --version | sed -n 1p
    cargo --version
    cargo build --release --locked --workspace --target-dir target/portable
  "

OUT="$ROOT/target/portable/release"
strip "$OUT/rt85-manager" "$OUT/rt85"
echo "Built:"
ls -lh "$OUT/rt85-manager" "$OUT/rt85"
echo "Highest glibc symbol version required:"
objdump -T "$OUT/rt85-manager" "$OUT/rt85" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1
