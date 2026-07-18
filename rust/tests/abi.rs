use core::ffi::{c_int, c_uint, c_void};
use core::mem::{align_of, size_of};

use funchook::{raw, Error, PrepareParams};

#[test]
fn public_structs_match_the_c_layout() {
    assert_eq!(
        size_of::<raw::funchook_info_t>(),
        6 * size_of::<*mut c_void>()
    );
    assert_eq!(
        align_of::<raw::funchook_info_t>(),
        align_of::<*mut c_void>()
    );

    let pointer_fields = 3 * size_of::<*mut c_void>();
    let unpadded_size = pointer_fields + size_of::<c_uint>();
    let pointer_alignment = align_of::<*mut c_void>();
    let expected_size = unpadded_size.div_ceil(pointer_alignment) * pointer_alignment;
    assert_eq!(size_of::<raw::funchook_params_t>(), expected_size);
    assert_eq!(align_of::<raw::funchook_params_t>(), pointer_alignment);
}

#[test]
fn nullable_prehook_has_pointer_layout() {
    assert_eq!(size_of::<raw::funchook_hook_t>(), size_of::<*mut c_void>());
    assert_eq!(
        align_of::<raw::funchook_hook_t>(),
        align_of::<*mut c_void>()
    );
}

#[test]
fn parameter_builders_preserve_every_field() {
    unsafe extern "C" fn prehook(_: *mut raw::funchook_info_t) {}

    let hook = 0x1234usize as *mut c_void;
    let user_data = 0x5678usize as *mut c_void;
    let params = PrepareParams::new(hook)
        .with_prehook(Some(prehook))
        .with_user_data(user_data)
        .with_flags(9);
    let raw = params.as_raw();
    assert_eq!(raw.hook_func, hook);
    assert!(raw.prehook.is_some());
    assert_eq!(raw.user_data, user_data);
    assert_eq!(raw.flags, 9);

    let default = PrepareParams::default();
    assert!(default.as_raw().hook_func.is_null());
    assert!(default.as_raw().prehook.is_none());
    assert!(default.as_raw().user_data.is_null());
    assert_eq!(default.as_raw().flags, 0);
}

#[test]
fn every_public_error_code_is_exposed_and_mapped() {
    use raw::*;
    let codes: [c_int; 14] = [
        FUNCHOOK_ERROR_INTERNAL_ERROR,
        FUNCHOOK_ERROR_SUCCESS,
        FUNCHOOK_ERROR_OUT_OF_MEMORY,
        FUNCHOOK_ERROR_ALREADY_INSTALLED,
        FUNCHOOK_ERROR_DISASSEMBLY,
        FUNCHOOK_ERROR_IP_RELATIVE_OFFSET,
        FUNCHOOK_ERROR_CANNOT_FIX_IP_RELATIVE,
        FUNCHOOK_ERROR_FOUND_BACK_JUMP,
        FUNCHOOK_ERROR_TOO_SHORT_INSTRUCTIONS,
        FUNCHOOK_ERROR_MEMORY_ALLOCATION,
        FUNCHOOK_ERROR_MEMORY_FUNCTION,
        FUNCHOOK_ERROR_NOT_INSTALLED,
        FUNCHOOK_ERROR_NO_AVAILABLE_REGISTERS,
        FUNCHOOK_ERROR_NO_SPACE_NEAR_TARGET_ADDR,
    ];
    assert_eq!(codes, [-1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);

    for code in codes {
        if code == 0 {
            assert_eq!(Error::from_code(code), None);
        } else {
            let error = Error::from_code(code).unwrap();
            assert_eq!(error.code(), code);
        }
    }
    assert_eq!(Error::from_code(99), Some(Error::Unknown(99)));
}
