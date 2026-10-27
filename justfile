set shell := ["bash", "-uc"]

# Show available recipes
default:
    @just --list

# Run the app. Each argument reaches it whole, spaces and all
[group('run')]
[positional-arguments]
run *args:
    cargo run -- "$@"

# Record doc/demo.gif from demo.tape
[group('run')]
demo:
    cargo build --locked --release
    mkdir -p doc
    vhs demo.tape

# Cut each sound recording from its CC0 preview on Freesound, with no
# re-encode. Typing: "Custom Mechanical Keyboard" by tofu_delivery,
# https://freesound.org/people/tofu_delivery/sounds/588650/
[group('dev')]
sounds:
    mkdir -p sounds
    ffmpeg -y -loglevel error -ss 1 -t 24 -i https://cdn.freesound.org/previews/588/588650_6529953-hq.mp3 -c copy -map_metadata -1 sounds/typing.mp3

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

# Pinned, so a newer cargo-deny cannot change the verdict on one machine.
cargo_deny := "0.20.2"

# Install the tools the gate needs beyond the Rust toolchain
[group('dev')]
tools:
    cargo install --locked cargo-deny@{{ cargo_deny }}

# Check advisories, licences, duplicate crates and sources against deny.toml
[group('dev')]
deny:
    cargo deny --locked check

# `cargo fmt --check` belongs to no other recipe: `fmt` rewrites files,
# which is the opposite of what a gate should do.

# Run the whole quality gate
[group('dev')]
check: lint test deny
    cargo fmt --all -- --check

# Build the release binary, sign it ad hoc, and copy it to dist/
[group('deploy')]
build:
    cargo build --locked --release
    mkdir -p dist
    cp target/release/hearth dist/
    codesign --force --sign - dist/hearth

# Install the dist binary to ~/self-made-bin/
[group('deploy')]
ship: build
    cp dist/hearth ~/self-made-bin/
