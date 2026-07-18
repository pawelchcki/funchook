use funchook::{raw, Funchook};

#[test]
fn raw_create_and_destroy_resolve_static_symbols() {
    unsafe {
        let handle = raw::funchook_create();
        assert!(!handle.is_null());
        assert_eq!(raw::funchook_destroy(handle), raw::FUNCHOOK_ERROR_SUCCESS);
    }
}

#[test]
fn owning_handle_lifecycle() {
    let handle = Funchook::new().expect("native handle allocation failed");
    assert!(!handle.as_raw().is_null());
    assert!(!handle.is_installed());
    assert_eq!(handle.error_message().to_bytes(), b"");
    drop(handle);
}

// The native project documents executable-memory limitations for macOS arm64.
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
mod runtime {
    use super::*;
    use core::ffi::c_void;
    use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
    use funchook::{PrehookInfo, PrepareParams};

    type Unary = extern "C" fn(i32) -> i32;
    static mut UNARY_TRAMPOLINE: Option<Unary> = None;

    #[inline(never)]
    extern "C" fn unary_target(value: i32) -> i32 {
        value + 1
    }

    #[inline(never)]
    extern "C" fn unary_hook(value: i32) -> i32 {
        unsafe { UNARY_TRAMPOLINE.expect("trampoline was not initialized")(value) + 10 }
    }

    #[test]
    fn prepare_install_trampoline_and_uninstall() {
        let mut handle = Funchook::new().unwrap();
        let mut trampoline = unary_target as *const () as *mut c_void;
        unsafe {
            handle
                .prepare(&mut trampoline, unary_hook as *const () as *mut c_void)
                .unwrap();
            UNARY_TRAMPOLINE = Some(core::mem::transmute::<*mut c_void, Unary>(trampoline));
            handle.install().unwrap();
        }

        let call = std::hint::black_box(unary_target as Unary);
        assert_eq!(call(5), 16);
        unsafe { handle.uninstall().unwrap() };
        assert_eq!(call(5), 6);
    }

    static ARG0: AtomicI32 = AtomicI32::new(0);
    static ARG1: AtomicI32 = AtomicI32::new(0);
    static USER_DATA: AtomicUsize = AtomicUsize::new(0);

    #[inline(never)]
    extern "C" fn binary_target(a: i32, b: i32) -> i32 {
        a + b
    }

    #[inline(never)]
    extern "C" fn routed_hook(a: i32, b: i32) -> i32 {
        a * b
    }

    unsafe extern "C" fn route_prehook(raw_info: *mut raw::funchook_info_t) {
        let mut info = PrehookInfo::from_raw(raw_info).expect("null prehook info");
        USER_DATA.store(info.user_data() as usize, Ordering::SeqCst);
        let mut arguments = info.arguments().expect("null argument handle");

        #[cfg(target_arch = "x86")]
        let (a, b) = (
            arguments.stack(0).unwrap().read::<i32>(),
            arguments.stack(1).unwrap().read::<i32>(),
        );
        #[cfg(not(target_arch = "x86"))]
        let (a, b) = (
            arguments.integer_register(0).unwrap().read::<i32>(),
            arguments.integer_register(1).unwrap().read::<i32>(),
        );

        ARG0.store(a, Ordering::SeqCst);
        ARG1.store(b, Ordering::SeqCst);
        info.set_hook(routed_hook as *const () as *mut c_void);
    }

    #[test]
    fn prehook_routes_user_data_and_argument_locations() {
        let marker = 0x1234usize as *mut c_void;
        let params = PrepareParams::default()
            .with_prehook(Some(route_prehook))
            .with_user_data(marker);
        let mut trampoline = binary_target as *const () as *mut c_void;
        let mut handle = Funchook::new().unwrap();
        unsafe {
            handle
                .prepare_with_params(&mut trampoline, &params)
                .unwrap();
            handle.install().unwrap();
        }

        let call = std::hint::black_box(binary_target as extern "C" fn(i32, i32) -> i32);
        assert_eq!(call(6, 7), 42);
        assert_eq!(ARG0.load(Ordering::SeqCst), 6);
        assert_eq!(ARG1.load(Ordering::SeqCst), 7);
        assert_eq!(USER_DATA.load(Ordering::SeqCst), marker as usize);
        unsafe { handle.uninstall().unwrap() };
        assert_eq!(call(6, 7), 13);
    }
}
