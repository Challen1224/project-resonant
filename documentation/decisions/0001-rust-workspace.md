# ADR 0001: Rust Workspace

- **Status:** Accepted
- **Date:** 2026-11-16

## Context

Project Resonant needs a modular foundation for a multi-subsystem
digital audio workstation.

The application, shared core, audio engine, project model, UI,
and plugin platform must evolve without becoming one tightly coupled
package.

## Decision

Use a Cargo workspace with separate application and library packages.

The initial workspace contains:

- `resonant-studio`: application entry point
- `resonant-core`: shared foundational functionality

Additional crates will be introduced when their boundaries and
responsibilities are justified.

## Consequences

- Workspace dependencies and package metadata can be shared.
- Packages can be tested independently.
- Internal interfaces can evolve incrementally.
- The initial project remains small and easy to build.
