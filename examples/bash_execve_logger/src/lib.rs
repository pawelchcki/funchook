#![no_std]

use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use core::ffi::{c_char, c_int, c_long, c_void};
use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use funchook::Funchook;

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("bash-execve-logger supports Linux x86_64 and aarch64");

const AT_FDCWD: c_long = -100;
const O_WRONLY: c_long = 1;
const O_CREAT: c_long = 0o100;
const O_APPEND: c_long = 0o2000;
const PROT_READ: c_long = 1;
const PROT_WRITE: c_long = 2;
const MAP_PRIVATE: c_long = 2;
const MAP_ANONYMOUS: c_long = 0x20;
const LOG_PATH: &[u8] = b"/tmp/funchook-execve.log\0";
const LOG_PREFIX: &[u8] = b"execve: ";

#[cfg(target_arch = "x86_64")]
const SYS_WRITE: c_long = 1;
#[cfg(target_arch = "x86_64")]
const SYS_CLOSE: c_long = 3;
#[cfg(target_arch = "x86_64")]
const SYS_MMAP: c_long = 9;
#[cfg(target_arch = "x86_64")]
const SYS_MUNMAP: c_long = 11;
#[cfg(target_arch = "x86_64")]
const SYS_OPENAT: c_long = 257;
#[cfg(target_arch = "x86_64")]
const SYS_EXIT_GROUP: c_long = 231;

#[cfg(target_arch = "aarch64")]
const SYS_OPENAT: c_long = 56;
#[cfg(target_arch = "aarch64")]
const SYS_CLOSE: c_long = 57;
#[cfg(target_arch = "aarch64")]
const SYS_WRITE: c_long = 64;
#[cfg(target_arch = "aarch64")]
const SYS_EXIT_GROUP: c_long = 94;
#[cfg(target_arch = "aarch64")]
const SYS_MUNMAP: c_long = 215;
#[cfg(target_arch = "aarch64")]
const SYS_MMAP: c_long = 222;

type Execve =
    unsafe extern "C" fn(*const c_char, *const *const c_char, *const *const c_char) -> c_int;

extern "C" {
    #[link_name = "execve"]
    fn system_execve(
        path: *const c_char,
        argv: *const *const c_char,
        envp: *const *const c_char,
    ) -> c_int;
}

static EXECVE_TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

#[used]
#[link_section = ".init_array"]
static INIT_HOOK: unsafe extern "C" fn() = init_hook;

unsafe extern "C" fn init_hook() {
    let mut target = system_execve as *const () as *mut c_void;
    let Ok(mut hooks) = Funchook::new() else {
        return;
    };
    if hooks
        .prepare(&mut target, hook_execve as *const () as *mut c_void)
        .is_err()
    {
        return;
    }
    EXECVE_TRAMPOLINE.store(target, Ordering::Release);
    if hooks.install().is_ok() {
        core::mem::forget(hooks);
    } else {
        EXECVE_TRAMPOLINE.store(ptr::null_mut(), Ordering::Release);
    }
}

unsafe extern "C" fn hook_execve(
    path: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    log_execve(path);
    let trampoline = EXECVE_TRAMPOLINE.load(Ordering::Acquire);
    if trampoline.is_null() {
        return -1;
    }
    let original = core::mem::transmute::<*mut c_void, Execve>(trampoline);
    original(path, argv, envp)
}

unsafe fn log_execve(path: *const c_char) {
    let fd = syscall6(
        SYS_OPENAT,
        AT_FDCWD,
        LOG_PATH.as_ptr() as c_long,
        O_WRONLY | O_CREAT | O_APPEND,
        0o600,
        0,
        0,
    );
    if syscall_failed(fd) {
        return;
    }

    let mut record = MaybeUninit::<[u8; 4096]>::uninit();
    let record_ptr = record.as_mut_ptr().cast::<u8>();
    let mut length = 0;
    while length < LOG_PREFIX.len() {
        record_ptr.add(length).write(LOG_PREFIX[length]);
        length += 1;
    }
    if path.is_null() {
        for byte in b"<null>" {
            record_ptr.add(length).write(*byte);
            length += 1;
        }
    } else {
        let path = path.cast::<u8>();
        let mut path_index = 0;
        while length + 1 < 4096 {
            let byte = path.add(path_index).read();
            if byte == 0 {
                break;
            }
            record_ptr.add(length).write(byte);
            length += 1;
            path_index += 1;
        }
    }
    record_ptr.add(length).write(b'\n');
    length += 1;
    let _ = syscall6(
        SYS_WRITE,
        fd,
        record_ptr as c_long,
        length as c_long,
        0,
        0,
        0,
    );
    let _ = syscall6(SYS_CLOSE, fd, 0, 0, 0, 0, 0);
}

#[repr(C)]
struct Mapping {
    base: *mut u8,
    length: usize,
}

struct MmapAllocator;

#[global_allocator]
static ALLOCATOR: MmapAllocator = MmapAllocator;

unsafe impl GlobalAlloc for MmapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let header_size = core::mem::size_of::<Mapping>();
        let Some(length) = layout
            .size()
            .max(1)
            .checked_add(layout.align() - 1)
            .and_then(|length| length.checked_add(header_size))
        else {
            return ptr::null_mut();
        };
        let base = syscall6(
            SYS_MMAP,
            0,
            length as c_long,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        );
        if syscall_failed(base) {
            return ptr::null_mut();
        }
        let base = base as *mut u8;
        let payload = (base as usize + header_size + layout.align() - 1) & !(layout.align() - 1);
        let header = (payload - header_size) as *mut Mapping;
        header.write_unaligned(Mapping { base, length });
        payload as *mut u8
    }

    unsafe fn dealloc(&self, allocation: *mut u8, _: Layout) {
        let header = allocation
            .sub(core::mem::size_of::<Mapping>())
            .cast::<Mapping>()
            .read_unaligned();
        let _ = syscall6(
            SYS_MUNMAP,
            header.base as c_long,
            header.length as c_long,
            0,
            0,
            0,
            0,
        );
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // Anonymous mappings are already zero-filled by the kernel.
        self.alloc(layout)
    }

    unsafe fn realloc(&self, allocation: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let Ok(new_layout) = Layout::from_size_align(new_size, layout.align()) else {
            return ptr::null_mut();
        };
        let replacement = self.alloc(new_layout);
        if replacement.is_null() {
            return ptr::null_mut();
        }
        let copy_size = core::cmp::min(layout.size(), new_size);
        for index in 0..copy_size {
            replacement
                .add(index)
                .write_volatile(allocation.add(index).read_volatile());
        }
        self.dealloc(allocation, layout);
        replacement
    }
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe { exit_group(101) }
}

#[no_mangle]
extern "C" fn rust_eh_personality() {}

#[inline(always)]
fn syscall_failed(result: c_long) -> bool {
    result as usize >= (-4095isize) as usize
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall6(
    number: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
    a5: c_long,
    a6: c_long,
) -> c_long {
    let result;
    asm!(
        "syscall",
        inlateout("rax") number => result,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        in("r8") a5,
        in("r9") a6,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack),
    );
    result
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
unsafe fn syscall6(
    number: c_long,
    a1: c_long,
    a2: c_long,
    a3: c_long,
    a4: c_long,
    a5: c_long,
    a6: c_long,
) -> c_long {
    let result;
    asm!(
        "svc 0",
        inlateout("x0") a1 => result,
        in("x1") a2,
        in("x2") a3,
        in("x3") a4,
        in("x4") a5,
        in("x5") a6,
        in("x8") number,
        options(nostack),
    );
    result
}

#[cfg(target_arch = "x86_64")]
unsafe fn exit_group(code: c_int) -> ! {
    asm!(
        "syscall",
        in("rax") SYS_EXIT_GROUP,
        in("rdi") code as c_long,
        options(noreturn),
    )
}

#[cfg(target_arch = "aarch64")]
unsafe fn exit_group(code: c_int) -> ! {
    asm!(
        "svc 0",
        in("x8") SYS_EXIT_GROUP,
        in("x0") code as c_long,
        options(noreturn),
    )
}
