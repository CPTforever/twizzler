#![feature(fn_traits)]
#![feature(unboxed_closures)]
#![feature(tuple_trait)]
#![feature(naked_functions)]
#![feature(auto_traits)]
#![feature(negative_impls)]
#![feature(linkage)]
#![feature(maybe_uninit_as_bytes)]

use core::ffi::{c_char, CStr};
use std::{
    cell::UnsafeCell,
    fmt::Debug,
    marker::{PhantomData, Tuple},
    mem::MaybeUninit,
};

pub use secgate_macros::*;
use twizzler_abi::object::ObjID;
use twizzler_rt_abi::error::{ResourceError, TwzError};

pub mod util;

/// A struct of information about a secure gate. These are auto-generated by the
/// [crate::secure_gate] macro, and stored in a special ELF section (.twz_secgate_info) as an array.
/// The dynamic linker and monitor can then use this to easily enumerate gates.
#[repr(C)]
pub struct SecGateInfo<F> {
    /// A pointer to the implementation entry function. This must be a pointer, and we statically
    /// check that is has the same size as usize (sorry cheri, we'll fix this another time)
    pub imp: F,
    /// The name of this secure gate. This must be a pointer to a null-terminated C string.
    name: *const c_char,
}

impl<F> core::fmt::Debug for SecGateInfo<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecGateInfo({:p})", self.name)
    }
}

impl<F> SecGateInfo<F> {
    pub const fn new(imp: F, name: &'static CStr) -> Self {
        Self {
            imp,
            name: name.as_ptr(),
        }
    }

    pub fn name(&self) -> &CStr {
        // Safety: we only ever construct self from a static CStr.
        unsafe { CStr::from_ptr(self.name) }
    }
}

// Safety: If F is Send, we are too because the name field points to a static C string that cannot
// be written to.
unsafe impl<F: Send> Send for SecGateInfo<F> {}
// Safety: If F is Sync, we are too because the name field points to a static C string that cannot
// be written to.
unsafe impl<F: Sync> Sync for SecGateInfo<F> {}

/// Minimum alignment of secure trampolines.
pub const SECGATE_TRAMPOLINE_ALIGN: usize = 0x10;

/// Non-generic and non-pointer-based SecGateInfo, for use during dynamic linking.
pub type RawSecGateInfo = SecGateInfo<usize>;
// Ensure that these are the same size because the dynamic linker uses the raw variant.
static_assertions::assert_eq_size!(RawSecGateInfo, SecGateInfo<&fn()>);

/// Arguments that will be passed to the secure call. Concrete versions of this are generated by the
/// macro.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Arguments<Args: Tuple + Crossing + Copy> {
    args: Args,
}

impl<Args: Tuple + Crossing + Copy> Arguments<Args> {
    pub fn with_alloca<F, R>(args: Args, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        alloca::alloca(|stack_space| {
            stack_space.write(Self { args });
            // Safety: we init the MaybeUninit just above.
            f(unsafe { stack_space.assume_init_mut() })
        })
    }

    pub fn into_inner(self) -> Args {
        self.args
    }
}

/// Return value to be filled by the secure call. Concrete versions of this are generated by the
/// macro.
#[derive(Copy)]
#[repr(C)]
pub struct Return<T: Crossing + Copy> {
    isset: bool,
    ret: MaybeUninit<T>,
}

impl<T: Copy + Crossing> Clone for Return<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Crossing + Copy> Return<T> {
    pub fn with_alloca<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        alloca::alloca(|stack_space| {
            stack_space.write(Self {
                isset: false,
                ret: MaybeUninit::uninit(),
            });
            // Safety: we init the MaybeUninit just above.
            f(unsafe { stack_space.assume_init_mut() })
        })
    }

    /// If a previous call to set is made, or this was constructed by new(), then into_inner
    /// returns the inner value. Otherwise, returns None.
    pub fn into_inner(self) -> Option<T> {
        if self.isset {
            Some(unsafe { self.ret.assume_init() })
        } else {
            None
        }
    }

    /// Construct a new, uninitialized Self.
    pub fn new_uninit() -> Self {
        Self {
            isset: false,
            ret: MaybeUninit::uninit(),
        }
    }

    /// Set the inner value. Future call to into_inner will return Some(val).
    pub fn set(&mut self, val: T) {
        self.ret.write(val);
        self.isset = true;
    }
}

/// An auto trait that limits the types that can be send across to another compartment. These are:
/// 1. Types other than references, UnsafeCell, raw pointers, slices.
/// 2. #[repr(C)] structs and enums made from Crossing types.
///
/// # Safety
/// The type must meet the above requirements.
pub unsafe auto trait Crossing {}

impl<T> !Crossing for &T {}
impl<T> !Crossing for &mut T {}
impl<T: ?Sized> !Crossing for UnsafeCell<T> {}
impl<T> !Crossing for *const T {}
impl<T> !Crossing for *mut T {}
impl<T> !Crossing for &[T] {}
impl<T> !Crossing for &mut [T] {}

unsafe impl<T: Crossing + Copy> Crossing for Result<T, TwzError> {}

/// Required to put in your source if you call any secure gates.
// TODO: this isn't ideal, but it's the only solution I have at the moment. For some reason,
// the linker doesn't even bother linking the libcalloca.a library that alloca creates. This forces
// that to happen.
#[macro_export]
macro_rules! secgate_prelude {
    () => {
        #[link(name = "calloca", kind = "static")]
        extern "C" {
            pub fn c_with_alloca();
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct GateCallInfo {
    thread_id: ObjID,
    src_ctx: ObjID,
}

impl GateCallInfo {
    /// Allocate a new GateCallInfo on the stack for the closure.
    pub fn with_alloca<F, R>(thread_id: ObjID, src_ctx: ObjID, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        alloca::alloca(|stack_space| {
            stack_space.write(Self { thread_id, src_ctx });
            // Safety: we init the MaybeUninit just above.
            f(unsafe { stack_space.assume_init_mut() })
        })
    }

    /// Get the ID of the source context, or None if the call was not cross-context.
    pub fn source_context(&self) -> Option<ObjID> {
        if self.src_ctx.raw() == 0 {
            None
        } else {
            Some(self.src_ctx)
        }
    }

    /// Get the ID of the calling thread.
    pub fn thread_id(&self) -> ObjID {
        if self.thread_id.raw() == 0 {
            twizzler_abi::syscall::sys_thread_self_id()
        } else {
            self.thread_id
        }
    }

    /// Ensures that the data is filled out (may read thread ID from kernel if necessary).
    pub fn canonicalize(self) -> Self {
        Self {
            thread_id: self.thread_id(),
            src_ctx: self.src_ctx,
        }
    }
}

pub fn get_thread_id() -> ObjID {
    twizzler_abi::syscall::sys_thread_self_id()
}

pub fn get_sctx_id() -> ObjID {
    twizzler_abi::syscall::sys_thread_active_sctx_id()
}

pub fn runtime_preentry() -> Result<(), TwzError> {
    twizzler_rt_abi::core::twz_rt_cross_compartment_entry()
}

pub struct SecFrame {
    tp: usize,
    sctx: ObjID,
}

pub fn frame() -> SecFrame {
    let mut val: usize;
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("rdfsbase {}", out(reg) val);
        #[cfg(not(target_arch = "x86_64"))]
        core::arch::asm!("mrs {}, tpidr_el0", out(reg) val);
    }
    // TODO: do this without calling the kernel.
    let sctx = twizzler_abi::syscall::sys_thread_active_sctx_id();
    SecFrame { tp: val, sctx }
}

pub fn restore_frame(frame: SecFrame) {
    if frame.tp != 0 {
        twizzler_abi::syscall::sys_thread_settls(frame.tp as u64);
    }
    twizzler_abi::syscall::sys_thread_set_active_sctx_id(frame.sctx).unwrap();
}

#[derive(Clone, Copy)]
pub struct DynamicSecGate<'comp, A, R> {
    address: usize,
    _pd: PhantomData<&'comp (A, R)>,
}

impl<'a, A: Tuple + Crossing + Copy, R: Crossing + Copy> Fn<A> for DynamicSecGate<'a, A, R> {
    extern "rust-call" fn call(&self, args: A) -> Self::Output {
        unsafe { dynamic_gate_call(*self, args) }
    }
}

impl<'a, A: Tuple + Crossing + Copy, R: Crossing + Copy> FnMut<A> for DynamicSecGate<'a, A, R> {
    extern "rust-call" fn call_mut(&mut self, args: A) -> Self::Output {
        unsafe { dynamic_gate_call(*self, args) }
    }
}

impl<'a, A: Tuple + Crossing + Copy, R: Crossing + Copy> FnOnce<A> for DynamicSecGate<'a, A, R> {
    type Output = Result<R, TwzError>;

    extern "rust-call" fn call_once(self, args: A) -> Self::Output {
        unsafe { dynamic_gate_call(self, args) }
    }
}

impl<'a, A, R> Debug for DynamicSecGate<'a, A, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DynamicSecGate [{} -> {}] {{ address: {:x} }}",
            std::any::type_name::<A>(),
            std::any::type_name::<R>(),
            self.address
        )
    }
}

impl<'comp, A, R> DynamicSecGate<'comp, A, R> {
    pub unsafe fn new(address: usize) -> Self {
        Self {
            address,
            _pd: PhantomData,
        }
    }
}

pub unsafe fn dynamic_gate_call<A: Tuple + Crossing + Copy, R: Crossing + Copy>(
    target: DynamicSecGate<A, R>,
    args: A,
) -> Result<R, TwzError> {
    let frame = frame();
    // Allocate stack space for args + ret. Args::with_alloca also inits the memory.
    let ret = GateCallInfo::with_alloca(get_thread_id(), get_sctx_id(), |info| {
        Arguments::<A>::with_alloca(args, |args| {
            Return::<Result<R, TwzError>>::with_alloca(|ret| {
                // Call the trampoline in the mod.
                unsafe {
                        //#mod_name::#trampoline_name_without_prefix(info as *const _, args as *const _, ret as *mut _);
                        #[cfg(target_arch = "x86_64")]
                        core::arch::asm!("call {target}", target = in(reg) target.address, in("rdi") info as *const _, in("rsi") args as *const _, in("rdx") ret as *mut _, clobber_abi("C"));
                        #[cfg(not(target_arch = "x86_64"))]
                        todo!()
                    }
                ret.into_inner()
            })
        })
    });
    restore_frame(frame);
    ret.ok_or(ResourceError::Unavailable)?
}
