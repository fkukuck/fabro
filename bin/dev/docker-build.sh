#!/usr/bin/env bash
# Build a local fabro Docker image from the current working tree.
#
# The release workflow uses prebuilt release tarballs, but for local development
# we cargo build first and stage the binary where the Dockerfile expects it.

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

echo "Building fabro (release, host arch: $arch)..."
cargo build --release -p fabro-cli

mkdir -p "docker-context/$arch"
cp target/release/fabro "docker-context/$arch/fabro"

echo "Building Docker image..."
docker build -t fabro .
