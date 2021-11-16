use serde::{Deserialize, Serialize};
use stackdump_core::{Stackdump, Target};

pub mod fpu_registers;
pub mod registers;
mod stack;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CortexMRegisters {
    base: registers::CortexMBaseRegisters,
    fpu: Option<fpu_registers::CortexMFpuRegisters>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CortexMTarget {}
impl Target for CortexMTarget {
    type Registers = CortexMRegisters;

    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>) {
        target.registers.base.capture();
        if cfg!(feature = "cortex-m-fpu") {
            target.registers.fpu.insert(fpu_registers::CortexMFpuRegisters::default()).capture();
        }
        unsafe {
            stack::capture_stack(*target.registers.base.sp(), &mut target.stack);
        }
    }
}
