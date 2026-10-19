#!/usr/bin/env bash
set -eo pipefail

task_bin="$HOME/.local/bin"
task_prefix="$HOME/.local/share/fabro-native"
task_mamba_root="$HOME/.local/share/fabro-mamba"
export PATH="$task_bin:$PATH"

# A preparation retry can reuse launchers from an earlier failed attempt.
if [ -f "$task_bin/fabro-native-tool" ]; then
    cp .tessl/native-tool.sh "$task_bin/fabro-native-tool"
    chmod +x "$task_bin/fabro-native-tool"
fi

verify_tools() {
    printf '[factory setup] Verify C and C++ compilation, linking, and execution.\n'
    task_probe="$(mktemp -d)"
    trap 'rm -rf "$task_probe"' EXIT
    printf '#include <stdio.h>\nint main(void) { puts("C toolchain ready"); return 0; }\n' |
        cc -x c -c - -o "$task_probe/c-probe.o"
    ar rcs "$task_probe/probe.a" "$task_probe/c-probe.o"
    ranlib "$task_probe/probe.a"
    cc "$task_probe/probe.a" -o "$task_probe/c-probe"
    "$task_probe/c-probe"
    printf '#include <iostream>\nint main() { std::cout << "C++ toolchain ready\\n"; }\n' |
        c++ -x c++ - -o "$task_probe/cxx-probe"
    "$task_probe/cxx-probe"
    make --version >/dev/null
    cmake --version >/dev/null
    pkg-config --version
    perl -e 'print "Perl ready\n"'
}

task_missing=()
for task_tool in cc c++ ar ranlib make cmake pkg-config perl unzip; do
    if ! command -v "$task_tool" >/dev/null 2>&1 ||
       { [ "$task_bin/$task_tool" -ef "$task_bin/fabro-native-tool" ] &&
         [ ! -f "$task_prefix/conda-meta/history" ]; }; then
        task_missing+=("$task_tool")
    fi
done
if [ "${#task_missing[@]}" -eq 0 ]; then
    printf '[factory setup] Native build tools are already available.\n'
    verify_tools
    exit 0
fi

printf '[factory setup] Installing prebuilt native tools for: %s\n' "${task_missing[*]}"
case "$(uname -s)/$(uname -m)" in
    Linux/x86_64) task_platform='linux-64' ;;
    Linux/aarch64|Linux/arm64) task_platform='linux-aarch64' ;;
    *)
        printf '[factory setup] Automatic native tool installation is supported on Linux; install the missing tools on this machine.\n' >&2
        exit 1
        ;;
esac

mkdir -p "$task_bin"
if [ ! -x "$task_bin/micromamba" ]; then
    task_release='https://github.com/mamba-org/micromamba-releases/releases/download/2.9.0-0'
    curl -fsSL "$task_release/micromamba-$task_platform" -o "$task_bin/micromamba.download"
    curl -fsSL "$task_release/micromamba-$task_platform.sha256" -o "$task_bin/micromamba.sha256"
    task_expected="$(awk 'NR == 1 { print $1 }' "$task_bin/micromamba.sha256")"
    printf '%s  %s\n' "$task_expected" "$task_bin/micromamba.download" | sha256sum --check -
    chmod +x "$task_bin/micromamba.download"
    mv "$task_bin/micromamba.download" "$task_bin/micromamba"
fi

# An interrupted package transaction is repaired on the next attempt.
task_operation='create'
if [ -f "$task_prefix/conda-meta/history" ]; then
    task_operation='install'
fi
# Global options must precede the subcommand: run otherwise treats them
# as command arguments and silently falls back to its default prefix.
"$task_bin/micromamba" --no-rc --root-prefix "$task_mamba_root" \
    "$task_operation" --yes --prefix "$task_prefix" \
    --override-channels --channel conda-forge \
    c-compiler cxx-compiler binutils make cmake pkg-config perl unzip

# Activation in this hook would disappear before the agent starts. Each
# launcher activates the private tool environment for its own process instead.
cp .tessl/native-tool.sh "$task_bin/fabro-native-tool"
chmod +x "$task_bin/fabro-native-tool"
for task_tool in "${task_missing[@]}"; do
    ln -sf "$task_bin/fabro-native-tool" "$task_bin/$task_tool"
done

verify_tools
