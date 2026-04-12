FROM rust:1-bookworm@sha256:4ec71e955e6c08aeb238885083222ddff79d82eb87654a96c76e38e94da1a53b AS rust-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY lib/crates lib/crates
COPY openapi openapi
RUN cargo build --release --bin fabro

FROM oven/bun:1@sha256:c9aa897b6028d54e510c4babd41bcdd481ad4a4b5030ad31135a9cabfe2c24df
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 make g++ ca-certificates nodejs npm && \
    rm -rf /var/lib/apt/lists/*
RUN npm i -g mintlify@4.2.507 && \
    sed -i 's/const version = __VERSION__/const version = "0.0.0"/' \
      /usr/local/lib/node_modules/mintlify/node_modules/katex/dist/katex.mjs

WORKDIR /app

COPY --from=rust-builder /app/target/release/fabro /usr/local/bin/fabro

COPY package.json bun.lock ./
COPY apps/fabro-web/package.json apps/fabro-web/
COPY lib/packages/fabro-api-client/package.json lib/packages/fabro-api-client/
RUN bun install --frozen-lockfile

COPY apps/fabro-web apps/fabro-web
COPY lib/packages/fabro-api-client lib/packages/fabro-api-client
COPY docs docs
COPY docker/entrypoint.ts .

ENTRYPOINT ["bun", "run", "entrypoint.ts"]
