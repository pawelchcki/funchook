#!/bin/sh
set -eu

target=${TARGET:-x86_64-unknown-linux-gnu}
smoke_target_dir=${CARGO_TARGET_DIR:-target/no-std-smoke}
freestanding_target_dir=$smoke_target_dir/freestanding
hosted_target_dir=$smoke_target_dir/hosted
freestanding_output=${TMPDIR:-/tmp}/funchook-no-std-smoke
hosted_output=${TMPDIR:-/tmp}/funchook-no-alloc-smoke

cargo rustc --lib --target-dir "$freestanding_target_dir" --target "$target" -- -C panic=abort
rlib=$(ls -t "$freestanding_target_dir/$target/debug/deps"/libfunchook-*.rlib | head -1)

rustc rust/no_std_smoke.rs \
    --edition 2021 \
    --target "$target" \
    --extern "funchook=$rlib" \
    -C panic=abort \
    -C default-linker-libraries=no \
    -C relocation-model=static \
    -C link-arg=-nostdlib \
    -C link-arg=-static \
    -o "$freestanding_output"

if nm -u "$freestanding_output" | grep .; then
    echo "unexpected undefined symbols in $freestanding_output" >&2
    exit 1
fi
if readelf -d "$freestanding_output" | grep NEEDED; then
    echo "unexpected dynamic dependency in $freestanding_output" >&2
    exit 1
fi
"$freestanding_output"

# The hosted feature must not require a Rust global allocator. It delegates
# native allocation to libc, while this consumer intentionally defines none.
cargo rustc --lib --features libc --target-dir "$hosted_target_dir" --target "$target" -- \
    -C panic=abort
rlib=$(ls -t "$hosted_target_dir/$target/debug/deps"/libfunchook-*.rlib | head -1)

rustc rust/no_alloc_smoke.rs \
    --edition 2021 \
    --target "$target" \
    --extern "funchook=$rlib" \
    -C panic=abort \
    -C link-arg=-nostartfiles \
    -o "$hosted_output"
"$hosted_output"
