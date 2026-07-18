# funchook

`funchook` intercepts native function calls and provides both raw bindings and
an owning Rust interface. The target-side crate is Rust 2021, uses `#![no_std]`
and `core` only, and has no normal Rust dependencies.

Funchook and Capstone 5.0.9 are compiled from the sources packaged with the
crate and bundled statically into the Rust `rlib`. A system funchook or
Capstone installation is never used. The platform C runtime and system
libraries remain external.

Supported targets are:

- Linux x86, x86_64, and aarch64, including glibc and musl toolchains
- macOS x86_64 and aarch64
- Windows x86, x86_64, and aarch64 with GNU or MSVC toolchains

macOS aarch64 builds and links, but retains the native project's documented
runtime executable-memory limitation.

## Basic hook and trampoline

Preparing replaces the supplied function pointer with a trampoline. Keep that
pointer so the hook can call the original implementation.

```rust,no_run
use core::ffi::c_void;
use funchook::Funchook;

type Add = extern "C" fn(i32, i32) -> i32;
static mut ORIGINAL_ADD: Option<Add> = None;

extern "C" fn add_hook(a: i32, b: i32) -> i32 {
    // Production code must synchronize access to callback state.
    unsafe { ORIGINAL_ADD.unwrap()(a, b) + 1 }
}

# extern "C" fn add(a: i32, b: i32) -> i32 { a + b }
let mut add_trampoline = add as *const () as *mut c_void;
let mut hooks = Funchook::new()?;
unsafe {
    hooks.prepare(
        &mut add_trampoline,
        add_hook as *const () as *mut c_void,
    )?;
    ORIGINAL_ADD = Some(core::mem::transmute::<*mut c_void, Add>(add_trampoline));
    hooks.install()?;
}

// Ensure no thread is executing the target or trampoline while restoring it.
unsafe { hooks.uninstall()? };
# Ok::<(), funchook::Error>(())
```

## Prehooks, routing, and arguments

A prehook runs immediately before dispatch. It may inspect user data and
argument locations and may select a different replacement for that invocation.
Passing a null hook routes to the trampoline.

```rust,no_run
use core::ffi::c_void;
use funchook::{raw, PrepareParams, PrehookInfo};

extern "C" fn replacement(a: usize) -> usize { a + 10 }

unsafe extern "C" fn before_call(raw: *mut raw::funchook_info_t) {
    // Never panic or unwind out of this callback.
    let mut info = PrehookInfo::from_raw(raw).unwrap();
    let state = info.user_data();
    if !state.is_null() {
        let mut args = info.arguments().unwrap();
        #[cfg(not(target_arch = "x86"))]
        args.integer_register(0).unwrap().write::<usize>(7);
        #[cfg(target_arch = "x86")]
        args.stack(0).unwrap().write::<usize>(7);
    }
    info.set_hook(replacement as *const () as *mut c_void);
}

let state = 1usize;
let params = PrepareParams::default()
    .with_prehook(Some(before_call))
    .with_user_data((&state as *const usize).cast_mut().cast());
# let _ = params;
```

Register and stack positions are ABI slots, not source-language argument
indices for every signature. The caller must apply the target platform's C ABI,
including register classes, stack placement, widths, and alignment.

## Safety and concurrency

The replacement, target, and trampoline must have exactly compatible calling
conventions and signatures. Callback pointers and user data must remain valid
until uninstall completes, callbacks must not unwind across the C ABI, and the
application must stop concurrent execution while executable code is installed
or restored. Installed hooks must be explicitly uninstalled to reclaim their
native handle.

Owning handles are neither `Send` nor `Sync`. Wrapper control operations are
globally serialized to protect native global state. Calls through `raw` bypass
that serialization and must provide it themselves.

`Drop` destroys an uninstalled native handle. It intentionally retains an
installed handle because implicit uninstallation cannot satisfy the executable
code safety requirements and live patched targets must retain their trampolines.

## Build and linkage contract

Building requires Rust 1.87.0 or newer. The repository pins that toolchain in
`rust-toolchain.toml`, so rustup installs and selects the MSRV automatically.

The `cmake` crate is a pinned build-only dependency; build scripts may use
`std`, but the library cannot. Offline builds require the Rust build
dependencies to be present in Cargo's cache, while all C and assembly sources
needed for funchook and the x86/aarch64 Capstone backends are in the crate.

Only system runtime libraries are dynamically linked: libc/libdl on Unix where
required and Win32/PSAPI on Windows. The final program does not load funchook or
Capstone dynamically.

Funchook is distributed under GPL-2.0 with its independent-module linking
exception; see `LICENSE`. Vendored Capstone is BSD-licensed; see
`vendor/capstone/LICENSE.TXT`.
