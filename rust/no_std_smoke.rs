#![no_std]
#![no_main]

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
compile_error!("the no_std smoke fixture currently supports Linux x86_64");

use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::panic::PanicInfo;

struct SysAllocator;

unsafe impl GlobalAlloc for SysAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result: isize;
        asm!(
            "syscall",
            inlateout("rax") 9isize => result,
            in("rdi") 0isize,
            in("rsi") layout.size().max(1),
            in("rdx") 3isize,
            in("r10") 0x22isize,
            in("r8") -1isize,
            in("r9") 0isize,
            lateout("rcx") _,
            lateout("r11") _,
        );
        if result < 0 {
            core::ptr::null_mut()
        } else {
            result as *mut u8
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        asm!(
            "syscall",
            in("rax") 11isize,
            in("rdi") ptr,
            in("rsi") layout.size().max(1),
            lateout("rax") _,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
}

#[global_allocator]
static ALLOCATOR: SysAllocator = SysAllocator;

fn exit(code: isize) -> ! {
    unsafe {
        asm!("syscall", in("rax") 60isize, in("rdi") code, options(noreturn));
    }
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    exit(101)
}

// Debug dependency builds can retain a personality reference even though the
// final smoke binary aborts on panic.
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
