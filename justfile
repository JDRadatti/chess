default:
    @just --list

# Install dev tools. mdbook must match the version mdbook-mermaid was built against.
setup:
    cargo install --locked just
    cargo install --locked mdbook --version 0.5.0
    cargo install --locked mdbook-mermaid --version 0.17.1
    git config core.hooksPath .githooks

# Serve docs as HTML with live reload
docs:
    mdbook serve docs --open

# Build docs to target/book
docs-build:
    mdbook build docs

# Format check, lint, build
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo build --workspace

test *args:
    cargo test --workspace {{args}}

# Play in the TUI
run *args:
    cargo run -p tui -- {{args}}

# Run the UCI engine on stdin and stdout
uci:
    cargo run --release -p uci

# Benchmark harness. `just bench help` lists commands.
bench *args:
    cargo run --release -p bench -- {{args}}
