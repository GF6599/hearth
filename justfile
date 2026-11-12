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
    vhs demo.tape

# Cut each sound recording from its CC0 preview on Freesound, with no
# re-encode. README.md credits each source.
# A cut that starts far into its file comes through a pipe, because ffmpeg
# stalls when it seeks that far into a file over HTTP.
[group('dev')]
sounds:
    mkdir -p sounds
    ffmpeg -y -loglevel error -ss 1 -t 24 -i https://cdn.freesound.org/previews/588/588650_6529953-hq.mp3 -c copy -map_metadata -1 sounds/typing.mp3
    curl -sfL https://cdn.freesound.org/previews/734/734248_2391840-hq.mp3 | ffmpeg -y -loglevel error -ss 146 -t 45 -i pipe: -c copy -map_metadata -1 -f mp3 sounds/fire.mp3
    ffmpeg -y -loglevel error -ss 14 -t 45 -i https://cdn.freesound.org/previews/459/459412_627541-hq.mp3 -c copy -map_metadata -1 sounds/river.mp3
    curl -sfL https://cdn.freesound.org/previews/868/868874_1661766-hq.mp3 | ffmpeg -y -loglevel error -ss 109 -t 45 -i pipe: -c copy -map_metadata -1 -f mp3 sounds/tide.mp3
    ffmpeg -y -loglevel error -ss 3.5 -t 45 -i https://cdn.freesound.org/previews/440/440807_8523759-hq.mp3 -c copy -map_metadata -1 sounds/clear_day.mp3
    curl -sfL https://cdn.freesound.org/previews/522/522299_9084007-hq.mp3 | ffmpeg -y -loglevel error -ss 64 -t 45 -i pipe: -c copy -map_metadata -1 -f mp3 sounds/clear_night.mp3
    curl -sfL https://cdn.freesound.org/previews/460/460178_4817223-hq.mp3 | ffmpeg -y -loglevel error -ss 52 -t 45 -i pipe: -c copy -map_metadata -1 -f mp3 sounds/cloud.mp3
    curl -sfL https://cdn.freesound.org/previews/678/678435_801011-hq.mp3 | ffmpeg -y -loglevel error -ss 179.5 -t 45 -i pipe: -c copy -map_metadata -1 -f mp3 sounds/rain.mp3
    ffmpeg -y -loglevel error -ss 8.5 -t 45 -i https://cdn.freesound.org/previews/117/117136_732072-hq.mp3 -c copy -map_metadata -1 sounds/snow.mp3
    ffmpeg -y -loglevel error -ss 3 -t 45 -i https://cdn.freesound.org/previews/425/425174_8323418-hq.mp3 -c copy -map_metadata -1 sounds/storm.mp3
    ffmpeg -y -loglevel error -ss 0 -t 19 -i https://cdn.freesound.org/previews/347/347562_5481182-hq.mp3 -c copy -map_metadata -1 sounds/thunder.mp3
    ffmpeg -y -loglevel error -ss 13.5 -t 45 -i https://cdn.freesound.org/previews/443/443061_6229882-hq.mp3 -c copy -map_metadata -1 sounds/fog.mp3
    ffmpeg -y -loglevel error -ss 4 -t 14 -i https://cdn.freesound.org/previews/128/128093_2338241-hq.mp3 -c copy -map_metadata -1 sounds/foghorn.mp3
    curl -sfL https://cdn.freesound.org/previews/150/150174_229898-hq.mp3 | ffmpeg -y -loglevel error -ss 69.5 -t 45 -i pipe: -c copy -map_metadata -1 -f mp3 sounds/wind.mp3

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
