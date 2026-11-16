# Development Guide

## Requirements

- Rust stable toolchain
- Cargo
- Git

## Common Commands

Build:

```bash
cargo build --workspace
```

Run the application:

```bash
cargo run -p resonant-studio
```

Test:

```bash
cargo test --workspace --all-targets
```

Format:

```bash
cargo fmt --all
```

Verify formatting:

```bash
cargo fmt --all -- --check
```

Lint:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

## Engineering Workflow

1. Define a small milestone and its acceptance criteria.
2. Implement one focused change.
3. Run formatting, build, lint, and relevant tests.
4. Review the diff for unintended changes.
5. Document architectural decisions.
6. Commit a verified milestone.

## Dependency Policy

Dependencies should have a clear purpose and acceptable maintenance,
licensing, security, and platform characteristics.

Prefer focused dependencies over large frameworks that dictate
the architecture.
