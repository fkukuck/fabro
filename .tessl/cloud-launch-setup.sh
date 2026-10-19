#!/usr/bin/env bash
set -eo pipefail

step() { printf '\n[factory setup] %s\n' "$*"; }
fail() { printf '[factory setup] ERROR: %s\n' "$*" >&2; exit 1; }
at_least() {
    awk -v actual="$1" -v minimum="$2" 'BEGIN {
        split(actual, a, "."); split(minimum, b, ".");
        for (i = 1; i <= 3; i++) {
            if (a[i] + 0 > b[i] + 0) exit 0;
            if (a[i] + 0 < b[i] + 0) exit 1;
        }
        exit 0;
    }'
}

step 'Check native Rust build prerequisites (no sudo or system package installs)'
for tool in curl git tar cc c++ make cmake pkg-config; do
    command -v "$tool" >/dev/null 2>&1 ||
        fail "Missing $tool. The cloud runner must supply native build prerequisites for this Rust workspace."
done

step 'Make installed tools available to subsequent agent processes'
task_bin="$HOME/.local/bin"
mkdir -p "$task_bin"
export PATH="$task_bin:$HOME/.cargo/bin:$PATH"

step 'Ensure Rust is managed by rustup'
if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs |
        sh -s -- -y --profile minimal --no-modify-path --default-toolchain stable
fi
task_rustup="$(command -v rustup)"
for tool in rustup cargo rustc rustdoc cargo-fmt cargo-clippy rustfmt clippy-driver; do
    if [ "$task_bin/$tool" != "$task_rustup" ]; then
        ln -sf "$task_rustup" "$task_bin/$tool"
    fi
done

step 'Ensure a compatible stable Rust compiler (1.97 or newer)'
task_rust_version="$(rustc --version 2>/dev/null | awk '{print $2}' || true)"
if ! at_least "${task_rust_version:-0}" '1.97.0'; then
    rustup toolchain install stable --profile minimal
    rustup default stable
fi
task_rust_version="$(rustc --version | awk '{print $2}')"
at_least "$task_rust_version" '1.97.0' ||
    fail "Rust >=1.97 is required; found $task_rust_version."

step 'Ensure the nightly formatting/lint toolchain required by AGENTS.md'
task_nightly='nightly-2026-04-14'
if ! rustup run "$task_nightly" rustfmt --version >/dev/null 2>&1 ||
   ! rustup run "$task_nightly" cargo-clippy --version >/dev/null 2>&1; then
    rustup toolchain install "$task_nightly" --profile minimal --component clippy,rustfmt
fi

step 'Ensure cargo-nextest (prebuilt binary)'
if ! cargo nextest --version >/dev/null 2>&1; then
    case "$(uname -s)/$(uname -m)" in
        Linux/x86_64) task_nextest_platform='linux-musl' ;;
        Linux/aarch64|Linux/arm64) task_nextest_platform='linux-arm-musl' ;;
        Darwin/*) task_nextest_platform='mac' ;;
        *) fail 'No nextest download configured for this operating system/architecture.' ;;
    esac
    curl -fsSL "https://get.nexte.st/latest/$task_nextest_platform" |
        tar xzf - -C "$task_bin"
elif [ "$(command -v cargo-nextest)" != "$task_bin/cargo-nextest" ]; then
    ln -sf "$(command -v cargo-nextest)" "$task_bin/cargo-nextest"
fi
cargo nextest --version

step 'Ensure Bun (1.3 or newer, compatible with the workspace lockfile)'
task_bun_version='0'
if command -v bun >/dev/null 2>&1; then
    task_bun_version="$(bun --version)"
fi
if ! at_least "$task_bun_version" '1.3.0'; then
    command -v unzip >/dev/null 2>&1 || fail 'Bun installation requires unzip on the cloud runner.'
    case "$(uname -s)/$(uname -m)" in
        Linux/x86_64) task_bun_target='linux-x64-baseline' ;;
        Linux/aarch64|Linux/arm64) task_bun_target='linux-aarch64' ;;
        Darwin/arm64) task_bun_target='darwin-aarch64' ;;
        Darwin/x86_64) task_bun_target='darwin-x64' ;;
        *) fail 'No Bun download configured for this operating system/architecture.' ;;
    esac
    task_bun_archive="$(mktemp)"
    trap 'rm -f "$task_bun_archive"' EXIT
    curl -fsSL "https://github.com/oven-sh/bun/releases/latest/download/bun-$task_bun_target.zip" -o "$task_bun_archive"
    unzip -p "$task_bun_archive" "bun-$task_bun_target/bun" > "$task_bin/bun"
    chmod +x "$task_bin/bun"
elif [ "$(command -v bun)" != "$task_bin/bun" ]; then
    ln -sf "$(command -v bun)" "$task_bin/bun"
fi
task_bun_version="$(bun --version)"
at_least "$task_bun_version" '1.3.0' || fail "Bun >=1.3 is required; found $task_bun_version."

step 'Install workspace dependencies without changing the lockfile'
bun install --frozen-lockfile

step 'Setup complete; Docker-backed and live Azure tests require their own available services and credentials'
