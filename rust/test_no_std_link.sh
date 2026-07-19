#!/bin/sh
set -eu

target=${TARGET:-x86_64-unknown-linux-gnu}
output=${TMPDIR:-/tmp}/funchook-no-std-smoke

cargo rustc --lib --target "$target" -- -C panic=abort
rlib=$(ls -t "target/$target/debug/deps"/libfunchook-*.rlib | head -1)

rustc rust/no_std_smoke.rs \
    --edition 2021 \
    --target "$target" \
    --extern "funchook=$rlib" \
    -C panic=abort \
    -C default-linker-libraries=no \
    -C relocation-model=static \
    -C link-arg=-nostdlib \
    -C link-arg=-static \
    -o "$output"

if nm -u "$output" | grep .; then
    echo "unexpected undefined symbols in $output" >&2
    exit 1
fi
if readelf -d "$output" | grep NEEDED; then
    echo "unexpected dynamic dependency in $output" >&2
    exit 1
fi
"$output"
