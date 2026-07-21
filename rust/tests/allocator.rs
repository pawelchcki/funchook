#![cfg(not(feature = "libc"))]

use core::ffi::c_void;
use funchook::{
    funchook_rust_alloc, funchook_rust_calloc, funchook_rust_free, funchook_rust_realloc,
};

#[test]
fn allocations_are_aligned_and_zero_size_is_freeable() {
    unsafe {
        for size in [0, 1, 15, 16, 17, 1024] {
            let ptr = funchook_rust_alloc(size);
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % 16, 0);
            funchook_rust_free(ptr);
        }
        funchook_rust_free(core::ptr::null_mut());
    }
}

#[test]
fn calloc_zeroes_and_rejects_overflow() {
    unsafe {
        let ptr = funchook_rust_calloc(17, 7).cast::<u8>();
        assert!(!ptr.is_null());
        assert!((0..119).all(|index| ptr.add(index).read() == 0));
        funchook_rust_free(ptr.cast());

        assert!(funchook_rust_calloc(usize::MAX, 2).is_null());
        assert!(funchook_rust_alloc(usize::MAX).is_null());
    }
}

#[test]
fn realloc_grows_shrinks_and_preserves_bytes() {
    unsafe {
        let mut ptr = funchook_rust_realloc(core::ptr::null_mut(), 32).cast::<u8>();
        assert!(!ptr.is_null());
        for index in 0..32 {
            ptr.add(index).write(index as u8);
        }

        ptr = funchook_rust_realloc(ptr.cast(), 257).cast();
        assert!(!ptr.is_null());
        assert_eq!(ptr as usize % 16, 0);
        for index in 0..32 {
            assert_eq!(ptr.add(index).read(), index as u8);
        }

        ptr = funchook_rust_realloc(ptr.cast(), 8).cast();
        assert!(!ptr.is_null());
        for index in 0..8 {
            assert_eq!(ptr.add(index).read(), index as u8);
        }
        funchook_rust_free(ptr.cast());
    }
}

#[test]
fn realloc_zero_frees_and_returns_null() {
    unsafe {
        let ptr: *mut c_void = funchook_rust_alloc(8);
        assert!(!ptr.is_null());
        assert!(funchook_rust_realloc(ptr, 0).is_null());
    }
}
