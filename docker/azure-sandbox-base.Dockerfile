FROM --platform=$TARGETPLATFORM rust:1-bookworm AS builder

WORKDIR /src

COPY . .

RUN cargo build -p fabro-sandboxd --release \
    && install -D -m 0755 target/release/fabro-sandboxd /out/fabro-sandboxd

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

COPY --from=builder /out/fabro-sandboxd /usr/local/bin/fabro-sandboxd
WORKDIR /workspace
CMD ["fabro-sandboxd"]
