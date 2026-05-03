FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    bash \
    coreutils \
    findutils \
    grep \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --chmod=0755 out/fabro-sandboxd /usr/local/bin/fabro-sandboxd
WORKDIR /workspace
CMD ["fabro-sandboxd"]
