#!/bin/sh
set -eu

example_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
manifest="$example_dir/Cargo.toml"
target_dir=${CARGO_TARGET_DIR:-$example_dir/target}
library="$target_dir/release/libbash_execve_logger.so"
log=/tmp/funchook-execve.log

cargo build --release --locked --manifest-path "$manifest"

if readelf -d "$library" | grep NEEDED; then
    echo "the no_std preload library must not have dynamic dependencies" >&2
    exit 1
fi
unexpected_symbols=$(nm -u "$library" | awk '{ print $2 }' | grep -vx execve || true)
if [ -n "$unexpected_symbols" ]; then
    echo "unexpected dynamic imports:" >&2
    echo "$unexpected_symbols" >&2
    exit 1
fi

if [ -f "$log" ]; then
    before=$(wc -c < "$log")
else
    before=0
fi
LD_PRELOAD="$library" bash -c '/usr/bin/true'
tail -c "+$((before + 1))" "$log" | grep -Fx 'execve: /usr/bin/true'
