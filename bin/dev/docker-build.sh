#!/usr/bin/env bash
# Build a local fabro Docker image from the current working tree.
#
# The release workflow uses prebuilt release tarballs. For local iteration we
# compile inside rust:1-bookworm so the binary is a Linux ELF that matches the
# runtime image, regardless of host OS. Cargo registry and target dir are
# cached in named volumes so subsequent runs are incremental.

set -euo pipefail

cd "$(dirname "$0")/../.."

case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  aarch64|arm64) arch=arm64 ;;
  *)
    echo "unsupported host arch: $(uname -m)" >&2
    exit 1
    ;;
esac

echo "Building fabro-cli inside rust:1-bookworm (target arch: linux/$arch)..."
docker run --rm \
  -v "$PWD:/src" \
  -v fabro-docker-cargo-registry:/usr/local/cargo/registry \
  -v fabro-docker-cargo-target:/target \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  rust:1-bookworm \
  cargo build --release -p fabro-cli

echo "Extracting binary from builder cache..."
mkdir -p "docker-context/$arch"
docker run --rm \
  -v fabro-docker-cargo-target:/target \
  -v "$PWD/docker-context/$arch:/out" \
  rust:1-bookworm \
  cp /target/release/fabro /out/fabro

echo "Building Docker image..."
docker build -t fabro .
