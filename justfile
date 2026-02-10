# PMC Justfile
# Process Manager CLI - PM2 alternative written in Rust

set shell := ["bash", "-uc"]

# Show all available commands
default:
    @just --list --unsorted

# Build release binary
build:
    cargo build --release

# Install pmc (replaces current binary in ~/.cargo/bin)
install:
    #!/usr/bin/env bash
    killall pmc 2>/dev/null || true
    cargo install --path . --force
    echo "✅ Installed $(pmc --version)"

# Run cargo check
check:
    cargo check

# Format code
fmt:
    cargo fmt

# Lint code
lint:
    cargo clippy

# Clean build artifacts
clean:
    cargo clean

# Run tests
test:
    cargo test

# Shortcuts
alias b := build
alias i := install
alias c := check
alias t := test
