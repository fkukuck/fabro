#!/usr/bin/env bash
set -eo pipefail

# Run from within this checkout. Only the disposable clone and container change.
task_repo="$(git rev-parse --show-toplevel)"
task_runtime="${FACTORY_CONTAINER_RUNTIME:-}"
if [ -z "$task_runtime" ]; then
    if command -v docker >/dev/null 2>&1; then task_runtime=docker;
    else task_runtime=podman; fi
fi
command -v "$task_runtime" >/dev/null 2>&1 || {
    printf 'Start Docker or Podman before running this test.\n' >&2
    exit 1
}
# Native execution avoids emulator-specific Rust crashes on Apple Silicon.
# Set FACTORY_TEST_PLATFORM=linux/amd64 to exercise the cloud architecture.
case "$(uname -m)" in
    arm64|aarch64) task_default_platform=linux/arm64 ;;
    *) task_default_platform=linux/amd64 ;;
esac
task_platform="${FACTORY_TEST_PLATFORM:-$task_default_platform}"
printf 'Testing cloud setup on %s\n' "$task_platform"
task_scratch="$(mktemp -d)"
trap 'rm -rf "$task_scratch"' EXIT
git clone --quiet --depth 1 "file://$task_repo" "$task_scratch/repo"
for task_file in cloud-launch-setup.sh setup-native-tools.sh native-tool.sh; do
    cp "$task_repo/.tessl/$task_file" "$task_scratch/repo/.tessl/$task_file"
done

"$task_runtime" build --platform "$task_platform" \
    --file "$task_repo/.tessl/test-cloud-setup.Containerfile" \
    --tag localhost/fabro-factory-setup-test "$task_repo/.tessl"
task_container="fabro-factory-setup-test-$$"
"$task_runtime" run --detach --name "$task_container" --platform "$task_platform" \
    --volume "$task_scratch/repo:/source:ro" \
    localhost/fabro-factory-setup-test sleep infinity
if "$task_runtime" exec "$task_container" bash -c '
        set -eo pipefail
        test "$(id -u)" -ne 0
        ! command -v cc
        ! command -v sudo
        cp -R /source/. /workspace/repo/
        for task_attempt in 1 2; do
            printf "\nCloud setup attempt %s\n" "$task_attempt"
            bash .tessl/cloud-launch-setup.sh < /dev/null
            # This is a fresh process, after the setup shell has exited.
            bash -c '\''
                set -eo pipefail
                cc --version
                c++ --version
                rustc --version
                cargo nextest --version
                bun --version
                task_probe=$(mktemp -d)
                printf "fn main() { println!(\"Rust linker ready\"); }\n" |
                    rustc - -o "$task_probe/rust-probe"
                "$task_probe/rust-probe"
                rm -rf "$task_probe"
            '\''
        done
        git diff --exit-code -- bun.lock
    '; then
    "$task_runtime" rm --force "$task_container" >/dev/null
    printf '\nCloud setup passed twice, including fresh-process compiler/linker checks.\n'
else
    printf '\nFailed container retained for debugging: %s\n' "$task_container" >&2
    printf 'Copy changed hooks into /workspace/repo/.tessl and rerun them with %s exec.\n' "$task_runtime" >&2
    exit 1
fi
