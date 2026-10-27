#!/bin/sh
# Fails when a macOS release binary cannot start. dyld refuses a binary that
# names one library twice, and nothing before a launch shows it.
set -eu
binary=$1
target=$2
twice=$(otool -L "$binary" | tail -n +2 | awk '{print $1}' | sort | uniq -d)
if [ -n "$twice" ]; then
    echo "$binary links $twice more than once" >&2
    exit 1
fi
# The release runner is arm64, so only that binary can start there.
if [ "$target" = aarch64-apple-darwin ]; then
    "$binary" --version
fi
