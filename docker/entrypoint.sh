#!/bin/sh
set -eu

# When started as root (the default), ensure the storage volume is writable
# by the unprivileged fabro user, then drop privileges. File capabilities on
# /usr/local/bin/fabro grant CAP_NET_BIND_SERVICE so it can still bind port 80.
if [ "$(id -u)" = 0 ]; then
    chown fabro:fabro /storage
    exec runuser -u fabro -- "$@"
fi

exec "$@"
