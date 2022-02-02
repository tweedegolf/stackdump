//! # Stackdump Capture
//!
//! This crate defines stackdump capture functions for the platforms it is compiled for.
//! Platform detection is done automatically in the build.rs file.
//!
//! This only helps with capturing the stack and the registers.
//! If you want to capture the heap or any static data, then you'll have to do that yourself.
//!
//! The crate is built on the `stackdump-core` crate.
//! To get stack traces, feed the captured data into the `stackdump-trace` crate.
//!
//! ## Cortex m
//!
//! The cortex m capture comes in two variants:
//! - Without FPU
//!   - Only captures and returns the core registers
//! - With FPU
//!   - Also captures and returns the fpu registers
//!
//! The fpu registers are automatically captured if the compilation target supports it.
//! This does change the capture return type.
//!
//! ### Example
//!
//! With fpu:
//!
//! ``` ignore
//! use stackdump_capture::stackdump_core::memory_region::ArrayMemoryRegion;
//!
//! let mut stack_capture = ArrayMemoryRegion::default();
//! let (core_registers, fpu_registers) = cortex_m::interrupt::free(|cs| {
//!     stackdump_capture::cortex_m::capture(&mut stack_capture, &cs)
//! });
//! ```
//!
//! ## For use when crashing (using cortex m as example target)
//!
//! You probably want to do a stack dump when there's a crash so that you can send it to the server after a reboot.
//!
//! To do that, you do need to do some setup yourself still.
//! Because the stack capture can be quite big, you have to give a reference to it in the capture function.
//! The registers are returned directly.
//!
//! We need to be able to persist all the data across the reboot.
//! If you have a filesystem or something similar, you could write it to there.
//! But most embedded systems have got some SRAM so we could keep it in uninitialized memory.
//!
//! ``` no_run
//! use core::mem::MaybeUninit;
//! use stackdump_core::memory_region::ArrayMemoryRegion;
//! use stackdump_core::register_data::ArrayRegisterData;
//!
//! #[link_section = ".uninit"]
//! static mut STACK_CAPTURE: MaybeUninit<ArrayMemoryRegion<4096>> = MaybeUninit::uninit();
//! #[link_section = ".uninit"]
//! static mut CORE_REGISTERS_CAPTURE: MaybeUninit<ArrayRegisterData<16, u32>> = MaybeUninit::uninit();
//! #[link_section = ".uninit"]
//! static mut FPU_REGISTERS_CAPTURE: MaybeUninit<ArrayRegisterData<32, u32>> = MaybeUninit::uninit();
//! ```
//!
//! We also need to be able to detect at bootup if a stackdump has been captured.
//! The best way is to have an uninitialized integer present that can have a specific value to indicate the dump has been made.
//!
//! ``` no_run
//! use core::mem::MaybeUninit;
//!
//! #[link_section = ".uninit"]
//! static mut CAPTURE_INDICATOR: MaybeUninit<u32> = MaybeUninit::uninit();
//! const CAPTURE_INDICATOR_TRUE: u32 = 0xC0DED1ED; // Code died
//!
//! fn is_capture_made() -> bool {
//!     unsafe {
//!         CAPTURE_INDICATOR.assume_init() == CAPTURE_INDICATOR_TRUE
//!     }
//! }
//!
//! fn reset_capture_made() {
//!     unsafe {
//!         CAPTURE_INDICATOR.write(0);
//!     }
//! }
//!
//!
//! fn set_capture_made() {
//!     unsafe {
//!         CAPTURE_INDICATOR.write(CAPTURE_INDICATOR_TRUE);
//!     }
//! }
//! ```
//!
//! Now we can capture a everything in e.g. a panic.
//!
//! ``` ignore
//! #[panic_handler]
//! fn panic(_info: &core::panic::PanicInfo) -> ! {
//!     cortex_m::interrupt::free(|cs| {
//!         unsafe {
//!             let (core_registers, fpu_registers) = stackdump_capture::cortex_m::capture(STACK_CAPTURE.assume_init_mut(), &cs);
//!             CORE_REGISTERS_CAPTURE.write(core_registers);
//!             CORE_REGISTERS_CAPTURE.write(fpu_registers);
//!
//!             // If you want to capture the heap or the static data, then do that here too
//!         }
//!
//!         set_capture_made();
//!     });
//!     cortex_m::peripheral::SCB::sys_reset()
//! }
//! ```
//!
//! In our main we can then check if there is a stackdump and send it to the server.
//! Actually transporting the data is the responsibility of the user, but the memory regions and register data
//! have an iter function so you can iterate over the bytes.
//!
//! ``` ignore
//! fn main() {
//!     let server = (); // User defined
//!
//!     if is_capture_made() {
//!         reset_capture_made();
//!         
//!         for byte in unsafe { STACK_CAPTURE.assume_init_ref().iter() } {
//!             server.send(byte);
//!         }
//!         for byte in unsafe { CORE_REGISTERS_CAPTURE.assume_init_ref().iter() } {
//!             server.send(byte);
//!         }
//!         for byte in unsafe { FPU_REGISTERS_CAPTURE.assume_init_ref().iter() } {
//!             server.send(byte);
//!         }
//!     }
//! }
//! ```
//!

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]

#[cfg(any(cortex_m, doc))]
pub mod cortex_m;
pub use stackdump_core;
