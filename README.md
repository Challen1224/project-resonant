# Project Resonant

**Music without boundaries.**

Project Resonant is an open-source digital audio workstation (DAW)
being developed in Rust.

Our goal is to build a professional music production environment
with a unified creative workflow, a custom audio engine, powerful
instruments and effects, and an extensible plugin ecosystem.

## Project Status

Early development — foundational workspace established.

## Development

Requirements:

- Rust stable toolchain
- Cargo

Build the workspace:

```bash
cargo build --workspace
```

Run the application:

```bash
cargo run -p resonant-studio
```

Run tests:

```bash
cargo test --workspace
```

Check formatting:

```bash
cargo fmt --all -- --check
```

Run Clippy:

```bash
cargo clippy --workspace --all-targets
```

## Architecture

The project uses a Cargo workspace. Application and engine
responsibilities will be separated into focused crates as the
architecture develops.

See `documentation/` for design and architecture documents.

## License

The project's open-source licensing is being finalized.
