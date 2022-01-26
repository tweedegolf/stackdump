use stackdump_core::{memory_region::ArrayVecMemoryRegion, register_data::ArrayVecRegisterData};

pub fn capture<const SIZE: usize>(
    stack: &mut ArrayVecMemoryRegion<SIZE>,
    _cs: &bare_metal::CriticalSection,
) -> ArrayVecRegisterData<16, u32> {
    let core_registers = capture_core_registers();
    capture_stack(core_registers.registers[13], stack);
    core_registers
}

#[cfg(has_fpu)]
pub fn capture_with_fpu<const SIZE: usize>(
    stack: &mut ArrayVecMemoryRegion<SIZE>,
    _cs: &bare_metal::CriticalSection,
) -> (ArrayVecRegisterData<16, u32>, ArrayVecRegisterData<32, u32>) {
    let fpu_registers = capture_fpu_registers();
    (capture(stack, _cs), fpu_registers)
}

fn capture_core_registers() -> ArrayVecRegisterData<16, u32> {
    use core::arch::asm;

    let mut register_array = arrayvec::ArrayVec::new();

    unsafe {
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
            in(reg) register_array.as_mut_ptr(),
            tmp = out(reg) _,
        );
    }

    ArrayVecRegisterData::new(0, register_array)
}

fn capture_fpu_registers() -> ArrayVecRegisterData<32, u32> {
    use core::arch::asm;

    let mut register_array = arrayvec::ArrayVec::new();

    unsafe {
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
            in(reg) register_array.as_mut_ptr(),
        );
    }

    ArrayVecRegisterData::new(256, register_array)
}

fn capture_stack<const SIZE: usize>(stack_pointer: u32, stack: &mut ArrayVecMemoryRegion<SIZE>) {
    extern "C" {
        static mut _stack_start: core::ffi::c_void;
    }

    /// Get the start address of the stack. The stack grows to lower addresses,
    /// so this should be the highest stack address you can get.
    fn stack_start() -> u32 {
        unsafe { &_stack_start as *const _ as u32 }
    }

    stack.start_address = stack_pointer as u64;
    stack.data.clear();
    let stack_size = stack_start()
        .saturating_sub(stack_pointer)
        .min(stack.data.capacity() as u32);

    unsafe {
        stack.data.set_len(stack_size as usize);
        stack
            .data
            .as_mut_ptr()
            .copy_from(stack_pointer as *const u8, stack_size as usize);
    }
}
