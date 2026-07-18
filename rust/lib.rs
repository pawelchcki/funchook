//! A `no_std` interface to the statically bundled funchook library.
//!
//! The target-side Rust API uses only [`core`]. The build script compiles
//! funchook and Capstone 5.0.9 from the sources included in this crate, so no
//! system funchook or Capstone installation is used.
//!
//! Hooking is inherently unsafe. In particular, the replacement and target
//! signatures must be ABI-compatible, no thread may execute code while it is
//! being patched or restored, and prehook callbacks must never unwind.

#![no_std]

use core::ffi::{c_int, c_void, CStr};
use core::hint::spin_loop;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, Ordering};

/// Raw declarations matching `include/funchook.h`.
pub mod raw {
    #![allow(non_camel_case_types)]

    use core::ffi::{c_char, c_int, c_uint, c_void};

    /// Opaque handle returned by [`funchook_create`].
    #[repr(C)]
    pub struct funchook_t {
        _private: [u8; 0],
    }

    /// Opaque snapshot of argument locations supplied to a prehook.
    #[repr(C)]
    pub struct funchook_arg_handle_t {
        _private: [u8; 0],
    }

    /// Information supplied to a prehook callback.
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct funchook_info_t {
        pub original_target_func: *mut c_void,
        pub target_func: *mut c_void,
        pub trampoline_func: *mut c_void,
        pub hook_func: *mut c_void,
        pub user_data: *mut c_void,
        pub arg_handle: *mut funchook_arg_handle_t,
    }

    /// A nullable prehook callback.
    pub type funchook_hook_t = Option<unsafe extern "C" fn(*mut funchook_info_t)>;

    /// Extended preparation parameters.
    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    pub struct funchook_params_t {
        pub hook_func: *mut c_void,
        pub prehook: funchook_hook_t,
        pub user_data: *mut c_void,
        pub flags: c_uint,
    }

    impl funchook_params_t {
        pub const fn new(hook_func: *mut c_void) -> Self {
            Self {
                hook_func,
                prehook: None,
                user_data: core::ptr::null_mut(),
                flags: 0,
            }
        }
    }

    impl Default for funchook_params_t {
        fn default() -> Self {
            Self::new(core::ptr::null_mut())
        }
    }

    pub const FUNCHOOK_ERROR_INTERNAL_ERROR: c_int = -1;
    pub const FUNCHOOK_ERROR_SUCCESS: c_int = 0;
    pub const FUNCHOOK_ERROR_OUT_OF_MEMORY: c_int = 1;
    pub const FUNCHOOK_ERROR_ALREADY_INSTALLED: c_int = 2;
    pub const FUNCHOOK_ERROR_DISASSEMBLY: c_int = 3;
    pub const FUNCHOOK_ERROR_IP_RELATIVE_OFFSET: c_int = 4;
    pub const FUNCHOOK_ERROR_CANNOT_FIX_IP_RELATIVE: c_int = 5;
    pub const FUNCHOOK_ERROR_FOUND_BACK_JUMP: c_int = 6;
    pub const FUNCHOOK_ERROR_TOO_SHORT_INSTRUCTIONS: c_int = 7;
    pub const FUNCHOOK_ERROR_MEMORY_ALLOCATION: c_int = 8;
    pub const FUNCHOOK_ERROR_MEMORY_FUNCTION: c_int = 9;
    pub const FUNCHOOK_ERROR_NOT_INSTALLED: c_int = 10;
    pub const FUNCHOOK_ERROR_NO_AVAILABLE_REGISTERS: c_int = 11;
    pub const FUNCHOOK_ERROR_NO_SPACE_NEAR_TARGET_ADDR: c_int = 12;

    extern "C" {
        pub fn funchook_create() -> *mut funchook_t;
        pub fn funchook_prepare(
            funchook: *mut funchook_t,
            target_func: *mut *mut c_void,
            hook_func: *mut c_void,
        ) -> c_int;
        pub fn funchook_prepare_with_params(
            funchook: *mut funchook_t,
            target_func: *mut *mut c_void,
            params: *const funchook_params_t,
        ) -> c_int;
        pub fn funchook_install(funchook: *mut funchook_t, flags: c_int) -> c_int;
        pub fn funchook_uninstall(funchook: *mut funchook_t, flags: c_int) -> c_int;
        pub fn funchook_destroy(funchook: *mut funchook_t) -> c_int;
        pub fn funchook_error_message(funchook: *const funchook_t) -> *const c_char;
        pub fn funchook_set_debug_file(name: *const c_char) -> c_int;
        pub fn funchook_arg_get_int_reg_addr(
            arg_handle: *const funchook_arg_handle_t,
            pos: c_int,
        ) -> *mut c_void;
        pub fn funchook_arg_get_flt_reg_addr(
            arg_handle: *const funchook_arg_handle_t,
            pos: c_int,
        ) -> *mut c_void;
        pub fn funchook_arg_get_stack_addr(
            arg_handle: *const funchook_arg_handle_t,
            pos: c_int,
        ) -> *mut c_void;
    }
}

/// A native funchook error code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InternalError,
    OutOfMemory,
    AlreadyInstalled,
    Disassembly,
    IpRelativeOffset,
    CannotFixIpRelative,
    FoundBackJump,
    TooShortInstructions,
    MemoryAllocation,
    MemoryFunction,
    NotInstalled,
    NoAvailableRegisters,
    NoSpaceNearTargetAddress,
    Unknown(c_int),
}

impl Error {
    /// Converts a nonzero native result code to an error.
    pub const fn from_code(code: c_int) -> Option<Self> {
        use raw::*;
        match code {
            FUNCHOOK_ERROR_SUCCESS => None,
            FUNCHOOK_ERROR_INTERNAL_ERROR => Some(Self::InternalError),
            FUNCHOOK_ERROR_OUT_OF_MEMORY => Some(Self::OutOfMemory),
            FUNCHOOK_ERROR_ALREADY_INSTALLED => Some(Self::AlreadyInstalled),
            FUNCHOOK_ERROR_DISASSEMBLY => Some(Self::Disassembly),
            FUNCHOOK_ERROR_IP_RELATIVE_OFFSET => Some(Self::IpRelativeOffset),
            FUNCHOOK_ERROR_CANNOT_FIX_IP_RELATIVE => Some(Self::CannotFixIpRelative),
            FUNCHOOK_ERROR_FOUND_BACK_JUMP => Some(Self::FoundBackJump),
            FUNCHOOK_ERROR_TOO_SHORT_INSTRUCTIONS => Some(Self::TooShortInstructions),
            FUNCHOOK_ERROR_MEMORY_ALLOCATION => Some(Self::MemoryAllocation),
            FUNCHOOK_ERROR_MEMORY_FUNCTION => Some(Self::MemoryFunction),
            FUNCHOOK_ERROR_NOT_INSTALLED => Some(Self::NotInstalled),
            FUNCHOOK_ERROR_NO_AVAILABLE_REGISTERS => Some(Self::NoAvailableRegisters),
            FUNCHOOK_ERROR_NO_SPACE_NEAR_TARGET_ADDR => Some(Self::NoSpaceNearTargetAddress),
            other => Some(Self::Unknown(other)),
        }
    }

    pub const fn code(self) -> c_int {
        use raw::*;
        match self {
            Self::InternalError => FUNCHOOK_ERROR_INTERNAL_ERROR,
            Self::OutOfMemory => FUNCHOOK_ERROR_OUT_OF_MEMORY,
            Self::AlreadyInstalled => FUNCHOOK_ERROR_ALREADY_INSTALLED,
            Self::Disassembly => FUNCHOOK_ERROR_DISASSEMBLY,
            Self::IpRelativeOffset => FUNCHOOK_ERROR_IP_RELATIVE_OFFSET,
            Self::CannotFixIpRelative => FUNCHOOK_ERROR_CANNOT_FIX_IP_RELATIVE,
            Self::FoundBackJump => FUNCHOOK_ERROR_FOUND_BACK_JUMP,
            Self::TooShortInstructions => FUNCHOOK_ERROR_TOO_SHORT_INSTRUCTIONS,
            Self::MemoryAllocation => FUNCHOOK_ERROR_MEMORY_ALLOCATION,
            Self::MemoryFunction => FUNCHOOK_ERROR_MEMORY_FUNCTION,
            Self::NotInstalled => FUNCHOOK_ERROR_NOT_INSTALLED,
            Self::NoAvailableRegisters => FUNCHOOK_ERROR_NO_AVAILABLE_REGISTERS,
            Self::NoSpaceNearTargetAddress => FUNCHOOK_ERROR_NO_SPACE_NEAR_TARGET_ADDR,
            Self::Unknown(code) => code,
        }
    }
}

/// Builder for [`Funchook::prepare_with_params`].
#[derive(Clone, Copy, Debug)]
pub struct PrepareParams {
    raw: raw::funchook_params_t,
}

impl PrepareParams {
    pub const fn new(hook_func: *mut c_void) -> Self {
        Self {
            raw: raw::funchook_params_t::new(hook_func),
        }
    }

    pub const fn with_prehook(mut self, prehook: raw::funchook_hook_t) -> Self {
        self.raw.prehook = prehook;
        self
    }

    pub const fn with_user_data(mut self, user_data: *mut c_void) -> Self {
        self.raw.user_data = user_data;
        self
    }

    pub const fn with_flags(mut self, flags: u32) -> Self {
        self.raw.flags = flags;
        self
    }

    pub const fn as_raw(&self) -> &raw::funchook_params_t {
        &self.raw
    }
}

impl Default for PrepareParams {
    fn default() -> Self {
        Self::new(core::ptr::null_mut())
    }
}

/// An owning funchook handle.
///
/// Handles are neither `Send` nor `Sync`. All safe-wrapper control-plane calls
/// are also globally serialized because the native library maintains global
/// allocation state. Raw API users must provide equivalent serialization.
pub struct Funchook {
    raw: NonNull<raw::funchook_t>,
    installed: bool,
    _not_send_or_sync: PhantomData<*mut ()>,
}

impl Funchook {
    pub fn new() -> Result<Self, Error> {
        let _guard = ControlGuard::lock();
        let raw = unsafe { raw::funchook_create() };
        let raw = NonNull::new(raw).ok_or(Error::OutOfMemory)?;
        Ok(Self {
            raw,
            installed: false,
            _not_send_or_sync: PhantomData,
        })
    }

    /// Prepares a hook and replaces `target_func` with its trampoline.
    ///
    /// # Safety
    ///
    /// `target_func` and `hook_func` must denote functions with identical,
    /// ABI-compatible signatures. Both functions and the updated trampoline
    /// pointer must remain valid while the hook can be used.
    pub unsafe fn prepare(
        &mut self,
        target_func: &mut *mut c_void,
        hook_func: *mut c_void,
    ) -> Result<(), Error> {
        let _guard = ControlGuard::lock();
        result(raw::funchook_prepare(
            self.raw.as_ptr(),
            target_func,
            hook_func,
        ))
    }

    /// Prepares a hook with a prehook and user data.
    ///
    /// # Safety
    ///
    /// The requirements of [`Self::prepare`] apply. The prehook must use the C
    /// ABI, must not unwind, and all pointers stored in `params` must remain
    /// valid for every invocation until the hook is uninstalled.
    pub unsafe fn prepare_with_params(
        &mut self,
        target_func: &mut *mut c_void,
        params: &PrepareParams,
    ) -> Result<(), Error> {
        let _guard = ControlGuard::lock();
        result(raw::funchook_prepare_with_params(
            self.raw.as_ptr(),
            target_func,
            params.as_raw(),
        ))
    }

    /// Installs every hook prepared on this handle.
    ///
    /// # Safety
    ///
    /// The caller must prevent every thread from executing the patched code
    /// while installation changes executable memory.
    pub unsafe fn install(&mut self) -> Result<(), Error> {
        let _guard = ControlGuard::lock();
        result(raw::funchook_install(self.raw.as_ptr(), 0))?;
        self.installed = true;
        Ok(())
    }

    /// Restores every installed target.
    ///
    /// # Safety
    ///
    /// The caller must prevent every thread from executing affected targets or
    /// trampolines while code is restored, and from using a trampoline after
    /// this method returns.
    pub unsafe fn uninstall(&mut self) -> Result<(), Error> {
        let _guard = ControlGuard::lock();
        result(raw::funchook_uninstall(self.raw.as_ptr(), 0))?;
        self.installed = false;
        Ok(())
    }

    /// Returns the most recent native diagnostic for this handle.
    pub fn error_message(&self) -> &CStr {
        let ptr = unsafe { raw::funchook_error_message(self.raw.as_ptr()) };
        if ptr.is_null() {
            c""
        } else {
            unsafe { CStr::from_ptr(ptr) }
        }
    }

    /// Borrows the underlying native handle.
    ///
    /// Calls made through it can desynchronize this wrapper's installed state.
    pub const fn as_raw(&self) -> *mut raw::funchook_t {
        self.raw.as_ptr()
    }

    pub const fn is_installed(&self) -> bool {
        self.installed
    }
}

impl Drop for Funchook {
    fn drop(&mut self) {
        let _guard = ControlGuard::lock();
        if self.installed {
            let code = unsafe { raw::funchook_uninstall(self.raw.as_ptr(), 0) };
            if Error::from_code(code).is_some() {
                // The patched code may still jump into this allocation. Leak
                // it rather than invalidating live trampolines.
                return;
            }
            self.installed = false;
        }
        let _ = unsafe { raw::funchook_destroy(self.raw.as_ptr()) };
    }
}

/// A safe field-level view of the native prehook information record.
pub struct PrehookInfo<'a> {
    raw: &'a mut raw::funchook_info_t,
}

impl<'a> PrehookInfo<'a> {
    /// Creates a view for the duration of one native callback.
    ///
    /// # Safety
    ///
    /// `raw` must be the non-aliased pointer supplied to the current prehook
    /// invocation and must remain valid for `'a`.
    pub unsafe fn from_raw(raw: *mut raw::funchook_info_t) -> Option<Self> {
        raw.as_mut().map(|raw| Self { raw })
    }

    pub fn original_target(&self) -> *mut c_void {
        self.raw.original_target_func
    }

    pub fn target(&self) -> *mut c_void {
        self.raw.target_func
    }

    pub fn trampoline(&self) -> *mut c_void {
        self.raw.trampoline_func
    }

    pub fn hook(&self) -> *mut c_void {
        self.raw.hook_func
    }

    /// Selects the replacement for this invocation. Null routes to the
    /// trampoline.
    pub fn set_hook(&mut self, hook: *mut c_void) {
        self.raw.hook_func = hook;
    }

    pub fn user_data(&self) -> *mut c_void {
        self.raw.user_data
    }

    /// Returns a typed mutable view of user data.
    ///
    /// # Safety
    ///
    /// The pointer must be aligned, valid, uniquely borrowed, and point to an
    /// initialized `T` for the returned lifetime.
    pub unsafe fn user_data_mut<T>(&mut self) -> Option<&mut T> {
        (self.raw.user_data as *mut T).as_mut()
    }

    pub fn arguments(&mut self) -> Option<Arguments<'_>> {
        NonNull::new(self.raw.arg_handle).map(|raw| Arguments {
            raw,
            _borrow: PhantomData,
        })
    }
}

/// Argument locations captured by a prehook bridge.
pub struct Arguments<'a> {
    raw: NonNull<raw::funchook_arg_handle_t>,
    _borrow: PhantomData<&'a mut raw::funchook_arg_handle_t>,
}

impl Arguments<'_> {
    pub fn integer_register(&mut self, position: u32) -> Option<ArgumentLocation<'_>> {
        self.location(position, raw::funchook_arg_get_int_reg_addr)
    }

    pub fn floating_register(&mut self, position: u32) -> Option<ArgumentLocation<'_>> {
        self.location(position, raw::funchook_arg_get_flt_reg_addr)
    }

    pub fn stack(&mut self, position: u32) -> Option<ArgumentLocation<'_>> {
        self.location(position, raw::funchook_arg_get_stack_addr)
    }

    fn location(
        &mut self,
        position: u32,
        get: unsafe extern "C" fn(*const raw::funchook_arg_handle_t, c_int) -> *mut c_void,
    ) -> Option<ArgumentLocation<'_>> {
        if position > c_int::MAX as u32 {
            return None;
        }
        let ptr = unsafe { get(self.raw.as_ptr(), position as c_int) };
        NonNull::new(ptr).map(|raw| ArgumentLocation {
            raw,
            _borrow: PhantomData,
        })
    }
}

/// A mutable machine argument slot captured immediately before dispatch.
pub struct ArgumentLocation<'a> {
    raw: NonNull<c_void>,
    _borrow: PhantomData<&'a mut c_void>,
}

impl ArgumentLocation<'_> {
    pub const fn as_ptr(&self) -> *mut c_void {
        self.raw.as_ptr()
    }

    /// Reads a value from this slot without assuming alignment.
    ///
    /// # Safety
    ///
    /// The slot must contain enough initialized bytes and their bit pattern
    /// must be valid for `T` under the platform ABI.
    pub unsafe fn read<T: Copy>(&self) -> T {
        core::ptr::read_unaligned(self.raw.as_ptr().cast::<T>())
    }

    /// Writes a value into this slot without assuming alignment.
    ///
    /// # Safety
    ///
    /// The slot must have enough writable storage and `T` must match the
    /// argument representation expected by the target ABI.
    pub unsafe fn write<T: Copy>(&mut self, value: T) {
        core::ptr::write_unaligned(self.raw.as_ptr().cast::<T>(), value);
    }
}

fn result(code: c_int) -> Result<(), Error> {
    match Error::from_code(code) {
        None => Ok(()),
        Some(error) => Err(error),
    }
}

static CONTROL_LOCK: AtomicBool = AtomicBool::new(false);

struct ControlGuard;

impl ControlGuard {
    fn lock() -> Self {
        while CONTROL_LOCK
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while CONTROL_LOCK.load(Ordering::Relaxed) {
                spin_loop();
            }
        }
        Self
    }
}

impl Drop for ControlGuard {
    fn drop(&mut self) {
        CONTROL_LOCK.store(false, Ordering::Release);
    }
}
