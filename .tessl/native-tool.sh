#!/usr/bin/env bash
set -eo pipefail

# Installed under the missing tool's name in ~/.local/bin. Current compiler
# packages supply standalone executables, not CC/CXX activation variables.
task_tool="${0##*/}"
task_prefix="$HOME/.local/share/fabro-native"
export PATH="$task_prefix/bin:$PATH"
case "$task_tool" in
    cc|c++) exec "$task_prefix/bin/$task_tool" \
        "-L$task_prefix/lib" "-Wl,-rpath,$task_prefix/lib" "$@" ;;
    *) exec "$task_prefix/bin/$task_tool" "$@" ;;
esac
