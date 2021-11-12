use arrayvec::ArrayVec;

#[inline(always)]
pub(crate) unsafe fn capture_stack<const STACK_SIZE: usize>(
    stack_pointer: u32,
    stack_buffer: &mut ArrayVec<u8, STACK_SIZE>,
) {
    let stack_start = stack_start();
    let stack_size = stack_start
        .saturating_sub(stack_pointer)
        .min(STACK_SIZE as u32);

    let stack_slice = core::slice::from_raw_parts(stack_pointer as *const u8, stack_size as usize);
    stack_buffer.clear();
    stack_buffer
        .try_extend_from_slice(stack_slice)
        .unwrap_unchecked();
}

extern "C" {
    static mut _stack_start: core::ffi::c_void;
}

/// Get the start address of the stack. The stack grows to lower addresses,
/// so this should be the highest stack address you can get.
fn stack_start() -> u32 {
    unsafe { &_stack_start as *const _ as u32 }
}
