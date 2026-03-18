# Contributing to Fabro

Thanks for your interest in contributing to Fabro!

## How to contribute

Fabro uses an **issue-based contribution model**. Instead of accepting outside pull requests, we accept bug reports and feature requests as GitHub Issues.

1. **Open an issue** -- File an issue on [GitHub Issues](https://github.com/fabro-sh/fabro/issues) with a bug report or feature request. The more detail your issue contains, the easier it will be for us to address it quickly and successfully.
2. **We build it** -- A Fabro maintainer will follow our software development process to create a patch, supervising AI coding agents and workflows.
3. **You get credit** -- We will include you as a co-author on the commit which lands the change.

See the [README](README.md#contributing-to-fabro) for more on why we use this model.

## Development setup

If you are maintaining a fork, the instructions below will help you build and test locally.

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (for the web frontend)
- Git

### Build and test

```bash
# Build all Rust crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Check formatting and lint
cargo fmt --check --all
cargo clippy --workspace -- -D warnings
```

### Web frontend (fabro-web)

```bash
cd apps/fabro-web
bun install
bun run dev        # start dev server
bun test           # run tests
bun run typecheck  # type check
```

## Development workflow

1. Create a branch from `main`
2. Make your changes
3. Ensure `cargo test --workspace`, `cargo fmt --check --all`, and `cargo clippy --workspace -- -D warnings` pass

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE.md).
