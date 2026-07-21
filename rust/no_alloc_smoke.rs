#![no_std]
#![no_main]

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
compile_error!("the no-allocator smoke fixture currently supports Linux x86_64");

use core::arch::asm;
use core::panic::PanicInfo;

fn exit(code: isize) -> ! {
    unsafe {
        asm!("syscall", in("rax") 60isize, in("rdi") code, options(noreturn));
    }
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    exit(101)
}

#[no_mangle]
extern "C" fn rust_eh_personality() {}

#[no_mangle]
extern "C" fn _start() -> ! {
    unsafe {
        let handle = funchook::raw::funchook_create();
        if handle.is_null() || funchook::raw::funchook_destroy(handle) != 0 {
            exit(1);
        }
    }
    exit(0)
}
