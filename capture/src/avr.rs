//! Capture functions for the avr platform

use stackdump_core::register_data::RegisterData;
use stackdump_core::{memory_region::ArrayMemoryRegion, register_data::ArrayRegisterData};

/// Capture the core registers and the stack
pub fn capture<const SIZE: usize>(
    stack: &mut ArrayMemoryRegion<SIZE>,
    core_registers: &mut ArrayRegisterData<34, u16>,
) {
    capture_core_registers(core_registers);
    capture_stack(
        core_registers
            .register(stackdump_core::gimli::Register(32))
            .unwrap(),
        stack,
    );
}

fn capture_core_registers(buffer: &mut ArrayRegisterData<34, u16>) {
    #[cfg(avr)]
    use core::arch::asm;

    // This array is going to hold the register data
    let mut register_array = arrayvec::ArrayVec::new();

    unsafe {
        register_array.set_len(34);

        #[cfg(avr)]
        asm!(
            "ldi {tmp},0",
            "st Z+,r0",
            "st Z+,{tmp}",
            "st Z+,r1",
            "st Z+,{tmp}",
            "st Z+,r2",
            "st Z+,{tmp}",
            "st Z+,r3",
            "st Z+,{tmp}",
            "st Z+,r4",
            "st Z+,{tmp}",
            "st Z+,r5",
            "st Z+,{tmp}",
            "st Z+,r6",
            "st Z+,{tmp}",
            "st Z+,r7",
            "st Z+,{tmp}",
            "st Z+,r8",
            "st Z+,{tmp}",
            "st Z+,r9",
            "st Z+,{tmp}",
            "st Z+,r10",
            "st Z+,{tmp}",
            "st Z+,r11",
            "st Z+,{tmp}",
            "st Z+,r12",
            "st Z+,{tmp}",
            "st Z+,r13",
            "st Z+,{tmp}",
            "st Z+,r14",
            "st Z+,{tmp}",
            "st Z+,r15",
            "st Z+,{tmp}",
            "st Z+,r16",
            "st Z+,{tmp}",
            "st Z+,r17",
            "st Z+,{tmp}",
            "st Z+,r18",
            "st Z+,{tmp}",
            "st Z+,r19",
            "st Z+,{tmp}",
            "st Z+,r20",
            "st Z+,{tmp}",
            "st Z+,r21",
            "st Z+,{tmp}",
            "st Z+,r22",
            "st Z+,{tmp}",
            "st Z+,r23",
            "st Z+,{tmp}",
            "st Z+,r24",
            "st Z+,{tmp}",
            "st Z+,r25",
            "st Z+,{tmp}",
            "st Z+,r26",
            "st Z+,{tmp}",
            "st Z+,r27",
            "st Z+,{tmp}",
            "st Z+,r28",
            "st Z+,{tmp}",
            "st Z+,r29",
            "st Z+,{tmp}",
            "st Z+,r30",
            "st Z+,{tmp}",
            "st Z+,r31",
            "st Z+,{tmp}",
            "in {tmp},__SP_H__",
            "st Z+,{tmp}",
            "in {tmp},__SP_L__",
            "st Z+,{tmp}",
            "rcall 1f",
            "1:",
            "pop {tmp}",
            "pop {tmp2}",
            "st Z+,{tmp}",
            "st Z+,{tmp2}",
            in("Z") register_array.as_mut_ptr(), // Every register is going to be written to an offset of this pointer
            tmp = out(reg) _, // We need a temporary register
            tmp2 = out(reg) _, // We need a temporary register
        );
    }

    *buffer = ArrayRegisterData::new(stackdump_core::gimli::Register(0), register_array);
}

/// Capture the stack from the current given stack pointer until the start of the stack into the given stack memory region.
/// The captured stack will be the smallest of the sizes of the current stack size or the memory region size.
///
/// If the memory region is too small, it will contain the top stack space and miss the bottom stack space.
/// This is done because the top of the stack is often more interesting than the bottom.
fn capture_stack<const SIZE: usize>(stack_pointer: u16, stack: &mut ArrayMemoryRegion<SIZE>) {
    extern "C" {
        static mut __stack: core::ffi::c_void;
    }

    /// Get the start address of the stack. The stack grows to lower addresses,
    /// so this should be the highest stack address you can get.
    fn stack_start() -> u16 {
        unsafe { &__stack as *const _ as u16 }
    }

    let stack_size = stack_start().saturating_sub(stack_pointer).min(SIZE as u16);
    unsafe {
        stack.copy_from_memory(stack_pointer as *const u8, stack_size as usize);
    }
}
