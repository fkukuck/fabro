#!/usr/bin/env bash
set -eo pipefail

# Installed under the missing tool's name in ~/.local/bin. Run with the
# compiler package's activation so headers, sysroot, and runtime libraries
# work even when the caller has never activated the package environment.
task_tool="${0##*/}"
exec "$HOME/.local/bin/micromamba" run --no-rc \
    --root-prefix "$HOME/.local/share/fabro-mamba" \
    --prefix "$HOME/.local/share/fabro-native" \
    bash -c '
        task_tool="$1"
        shift
        case "$task_tool" in
            cc) exec "${CC:?C compiler activation is missing}" \
                "-L$CONDA_PREFIX/lib" "-Wl,-rpath,$CONDA_PREFIX/lib" "$@" ;;
            c++) exec "${CXX:?C++ compiler activation is missing}" \
                "-L$CONDA_PREFIX/lib" "-Wl,-rpath,$CONDA_PREFIX/lib" "$@" ;;
            ar|ranlib) exec "$CONDA_PREFIX/bin/${CONDA_TOOLCHAIN_HOST:?Compiler activation is missing}-$task_tool" "$@" ;;
            *) exec "$CONDA_PREFIX/bin/$task_tool" "$@" ;;
        esac
    ' fabro-native-tool "$task_tool" "$@"
