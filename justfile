set shell := ["bash", "-uc"]

# Show available recipes
default:
    @just --list

# Run the app. Each argument reaches it whole, spaces and all
[group('run')]
[positional-arguments]
run *args:
    cargo run -- "$@"

# Lint with the same denial level CI uses
[group('dev')]
lint:
    cargo clippy --locked --all-targets -- -D warnings

# Format the crate in place
[group('dev')]
fmt:
    cargo fmt --all

# Run the full test suite
[group('dev')]
test:
    cargo test --locked --all-targets

# `cargo fmt --check` belongs to no other recipe: `fmt` rewrites files,
# which is the opposite of what a gate should do.

# Run the whole quality gate
[group('dev')]
check: lint test
    cargo fmt --all -- --check
