#!/bin/sh
set -eu

example_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
manifest="$example_dir/Cargo.toml"
target_dir=${CARGO_TARGET_DIR:-$example_dir/target}

if [ "$(uname -s)-$(uname -m)" != Linux-x86_64 ]; then
    echo "the portability test requires a Linux x86_64 build host" >&2
    exit 1
fi

# Build exactly once. Every runtime below receives this same read-only file.
cargo build --release --locked --manifest-path "$manifest"
library=$(realpath "$target_dir/release/libbash_execve_logger.so")

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

checksum=$(sha256sum "$library" | awk '{ print $1 }')

run_test() {
    name=$1
    image=$2
    shell=$3
    echo "Testing $name with $checksum"
    docker run --rm --platform linux/amd64 \
        --entrypoint "$shell" \
        --env EXPECTED_SHA256="$checksum" \
        --mount "type=bind,src=$library,dst=/libbash_execve_logger.so,readonly" \
        "$image" -c '
            actual=$(sha256sum /libbash_execve_logger.so | awk "{ print \$1 }")
            test "$actual" = "$EXPECTED_SHA256"
            LD_PRELOAD=/libbash_execve_logger.so bash -c "/bin/true"
            grep -Fx "execve: /bin/true" /tmp/funchook-execve.log
        '
}

run_test "Alpine/musl" \
    "bash:5.2@sha256:534a5f1d11652aadaa9f08838f6637ac11a46a8b4b736a4cbf09c5945e38516f" \
    /usr/local/bin/bash
run_test "Ubuntu/glibc 2.35" \
    "ubuntu:22.04@sha256:0d779ea97881505f5ef0039336ee85edba27519bdba968c284c86ee066a973c8" \
    /bin/bash
run_test "CentOS 6/glibc 2.12" \
    "centos:6@sha256:a93df2e96e07f56ea48f215425c6f1673ab922927894595bb5c0ee4c5a955133" \
    /bin/bash
