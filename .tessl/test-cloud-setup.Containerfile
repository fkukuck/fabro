FROM ubuntu:26.04

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Match the cloud's Node major without adding a compiler or sudo.
RUN set -eu; \
    case "$(dpkg --print-architecture)" in \
        amd64) task_arch=x64 ;; arm64) task_arch=arm64 ;; \
        *) exit 1 ;; \
    esac; \
    curl -fsSL https://nodejs.org/dist/latest-v24.x/SHASUMS256.txt -o /tmp/node-checksums; \
    task_archive="$(awk -v arch="$task_arch" '$2 ~ ("-linux-" arch "\\.tar\\.xz$") { print $2 }' /tmp/node-checksums)"; \
    test -n "$task_archive"; \
    curl -fsSL "https://nodejs.org/dist/latest-v24.x/$task_archive" -o "/tmp/$task_archive"; \
    cd /tmp; grep "  $task_archive\$" node-checksums | sha256sum --check -; \
    tar xJf "$task_archive" -C /usr/local --strip-components=1; \
    rm "$task_archive" node-checksums

RUN useradd --create-home --uid 1001 --shell /bin/bash agent \
    && mkdir -p /workspace/repo \
    && chown -R agent:agent /workspace

ENV CI=true
ENV PATH="/home/agent/.local/bin:/usr/local/bin:/usr/bin:/bin"
WORKDIR /workspace/repo
USER agent
