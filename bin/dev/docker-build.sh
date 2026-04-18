#!/usr/bin/env bash
# Build a local fabro Docker image from the current working tree.
#
# The Docker image uses musl binaries for a small Alpine-based runtime. We
# compile fabro-cli for the host's musl target inside rust:1-bookworm so the
# binary matches what the Alpine runtime expects, regardless of host OS.
# Cargo registry and target dir are cached in named volumes so subsequent
# runs are incremental.

set -euo pipefail

cd "$(dirname "$0")/../.."

case "$(uname -m)" in
  x86_64|amd64)
    arch=amd64
    target=x86_64-unknown-linux-musl
    ;;
  aarch64|arm64)
    arch=arm64
    target=aarch64-unknown-linux-musl
    ;;
  *)
    echo "unsupported host arch: $(uname -m)" >&2
    exit 1
    ;;
esac

case "$arch" in
  amd64) cc_var=CC_x86_64_unknown_linux_musl linker_var=CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER ;;
  arm64) cc_var=CC_aarch64_unknown_linux_musl linker_var=CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER ;;
esac

echo "Building fabro-cli for $target inside rust:1-bookworm..."
docker run --rm --platform "linux/$arch" \
  -v "$PWD:/src" \
  -v fabro-docker-cargo-registry:/usr/local/cargo/registry \
  -v fabro-docker-cargo-target:/target \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e "$cc_var=musl-gcc" \
  -e "$linker_var=musl-gcc" \
  -e LIBZ_SYS_STATIC=1 \
  rust:1-bookworm \
  sh -c "
    apt-get update -qq && apt-get install -y -qq musl-tools pkg-config perl make >/dev/null &&
    rustup target add $target &&
    cargo build --release -p fabro-cli --target $target
  "

echo "Extracting binary from builder cache..."
mkdir -p "docker-context/$arch"
docker run --rm --platform "linux/$arch" \
  -v fabro-docker-cargo-target:/target \
  -v "$PWD/docker-context/$arch:/out" \
  rust:1-bookworm \
  cp "/target/$target/release/fabro" /out/fabro

echo "Building Docker image..."
docker build -t fabro .
