//! Capture functions for the cortex-m platform

use stackdump_core::register_data::RegisterData;
use stackdump_core::{
    memory_region::{ArrayMemoryRegion, MemoryRegion},
    register_data::ArrayRegisterData,
};

/// Capture the core registers and the stack
#[cfg(not(has_fpu))]
pub fn capture<const SIZE: usize>(
    stack: &mut ArrayMemoryRegion<SIZE>,
    _cs: &bare_metal::CriticalSection,
) -> ArrayRegisterData<16, u32> {
    let core_registers = capture_core_registers();
    capture_stack(
        core_registers
            .register(stackdump_core::gimli::Arm::SP)
            .unwrap(),
        stack,
    );
    core_registers
}

/// Capture the core & fpu registers and the stack
#[cfg(has_fpu)]
pub fn capture<const SIZE: usize>(
    stack: &mut ArrayMemoryRegion<SIZE>,
    _cs: &bare_metal::CriticalSection,
) -> (ArrayRegisterData<16, u32>, ArrayRegisterData<32, u32>) {
    let core_registers = capture_core_registers();
    let fpu_registers = capture_fpu_registers();
    capture_stack(
        core_registers
            .register(stackdump_core::gimli::Arm::SP)
            .unwrap(),
        stack,
    );
    (core_registers, fpu_registers)
}

fn capture_core_registers() -> ArrayRegisterData<16, u32> {
    use core::arch::asm;

    // This array is going to hold the register data
    let mut register_array = arrayvec::ArrayVec::new();

    unsafe {
        // We've got 16 registers, so make space for that
        register_array.set_len(16);

        asm!(
            "str r0, [{0}, #0]",
            "str r1, [{0}, #4]",
            "str r2, [{0}, #8]",
            "str r3, [{0}, #12]",
            "str r4, [{0}, #16]",
            "str r5, [{0}, #20]",
            "str r6, [{0}, #24]",
            "str r7, [{0}, #28]",
            "str r8, [{0}, #32]",
            "str r9, [{0}, #36]",
            "str r10, [{0}, #40]",
            "str r11, [{0}, #44]",
            "str r12, [{0}, #48]",
            "str sp, [{0}, #52]",
            "str lr, [{0}, #56]",
            "mov {tmp}, pc", // We can't use the str instruction with the PC register directly, so store it in tmp
            "str {tmp}, [{0}, #60]",
            in(reg) register_array.as_mut_ptr(), // Every register is going to be written to an offset of this pointer
            tmp = out(reg) _, // We need a temporary register
        );
    }

    ArrayRegisterData::new(stackdump_core::gimli::Arm::R0, register_array)
}

#[cfg(has_fpu)]
fn capture_fpu_registers() -> ArrayRegisterData<32, u32> {
    use core::arch::asm;

    // This array is going to hold the register data
    let mut register_array = arrayvec::ArrayVec::new();

    unsafe {
        // We've got 32 registers, so make space for that
        register_array.set_len(32);

        asm!(
            "vstr s0, [{0}, #0]",
            "vstr s1, [{0}, #4]",
            "vstr s2, [{0}, #8]",
            "vstr s3, [{0}, #12]",
            "vstr s4, [{0}, #16]",
            "vstr s5, [{0}, #20]",
            "vstr s6, [{0}, #24]",
            "vstr s7, [{0}, #28]",
            "vstr s8, [{0}, #32]",
            "vstr s9, [{0}, #36]",
            "vstr s10, [{0}, #40]",
            "vstr s11, [{0}, #44]",
            "vstr s12, [{0}, #48]",
            "vstr s13, [{0}, #52]",
            "vstr s14, [{0}, #56]",
            "vstr s15, [{0}, #60]",
            "vstr s16, [{0}, #64]",
            "vstr s17, [{0}, #68]",
            "vstr s18, [{0}, #72]",
            "vstr s19, [{0}, #76]",
            "vstr s20, [{0}, #80]",
            "vstr s21, [{0}, #84]",
            "vstr s22, [{0}, #88]",
            "vstr s23, [{0}, #92]",
            "vstr s24, [{0}, #96]",
            "vstr s25, [{0}, #100]",
            "vstr s26, [{0}, #104]",
            "vstr s27, [{0}, #108]",
            "vstr s28, [{0}, #112]",
            "vstr s29, [{0}, #116]",
            "vstr s30, [{0}, #120]",
            "vstr s31, [{0}, #124]",
            in(reg) register_array.as_mut_ptr(), // Every register is going to be written to an offset of this pointer
        );
    }

    ArrayRegisterData::new(stackdump_core::gimli::Arm::S0, register_array)
}

/// Capture the stack from the current given stack pointer until the start of the stack into the given stack memory region.
/// The captured stack will be the smallest of the sizes of the current stack size or the memory region size.
///
/// If the memory region is too small, it will contain the top stack space and miss the bottom stack space.
/// This is done because the top of the stack is often more interesting than the bottom.
fn capture_stack<const SIZE: usize>(stack_pointer: u32, stack: &mut ArrayMemoryRegion<SIZE>) {
    extern "C" {
        static mut _stack_start: core::ffi::c_void;
    }

    /// Get the start address of the stack. The stack grows to lower addresses,
    /// so this should be the highest stack address you can get.
    fn stack_start() -> u32 {
        unsafe { &_stack_start as *const _ as u32 }
    }

    let stack_size = stack_start().saturating_sub(stack_pointer).min(SIZE as u32);
    unsafe {
        stack.copy_from_memory(stack_pointer as *const u8, stack_size as usize);
    }
}
