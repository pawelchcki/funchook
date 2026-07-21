# Bash `execve` logger

This Linux-only `no_std` preload library imports the Rust `funchook` crate and
patches `execve` during dynamic-loader initialization. Every intercepted call
appends the requested path to `/tmp/funchook-execve.log` before continuing
through funchook's trampoline.

The example supports Linux x86_64 and aarch64. Its allocator, initialization,
and logging use raw system calls; the resulting shared object has no libc or
other dynamic dependency. A single x86_64 build runs unchanged on musl and
glibc systems, including CentOS 6 with glibc 2.12.

The build also defines linker-local aliases for compiler-generated `memcpy`,
`memmove`, `memset`, and `strncpy` libcalls. They resolve to funchook's hidden
freestanding implementations and are not exposed as interposable `LD_PRELOAD`
symbols.

Build and run it with:

```sh
cd examples/bash_execve_logger
cargo build --release
LD_PRELOAD="$PWD/target/release/libbash_execve_logger.so" \
    bash -c '/usr/bin/printf "hello from bash\\n"'
tail /tmp/funchook-execve.log
```

Or run `./test.sh` to build it, verify that the shared object has no `NEEDED`
entries, and check a real Bash `execve` call.

On a Linux x86_64 host with Docker, `./test-portability.sh` builds the library
once and mounts that exact artifact read-only into pinned Alpine/musl, Ubuntu,
and CentOS 6 containers. The test verifies the artifact checksum in each
container before exercising Bash.

This is an instrumentation example, not a production auditing mechanism. The
hook is process-local, the fixed log can contain sensitive command paths, and
hook installation must happen before the process starts additional threads.
