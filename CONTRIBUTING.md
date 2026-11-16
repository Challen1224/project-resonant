# Contributing to Project Resonant

Thank you for your interest in Project Resonant.

## Development Principles

- Keep changes focused and reviewable.
- Preserve real-time audio safety.
- Document public APIs.
- Add tests for new functionality.
- Avoid unnecessary dependencies.
- Keep application, project, and audio-engine responsibilities separated.

## Before Submitting Changes

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Discuss significant architectural changes before introducing
new dependencies or changing public interfaces.
