use serde::{Deserialize, Serialize};
use stackdump_core::{Stackdump, Target};

#[cfg(feature = "cortex-m-fpu")]
pub mod fpu_registers;
pub mod registers;
mod stack;

#[cfg(not(feature = "cortex-m-fpu"))]
#[derive(Debug)]
pub struct CortexMTarget {}
#[cfg(not(feature = "cortex-m-fpu"))]
impl Target for CortexMTarget {
    type Registers = registers::CortexMRegisters;

    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>) {
        target.registers.capture();
        unsafe {
            stack::capture_stack(*target.registers.sp(), &mut target.stack);
        }
    }
}

#[cfg(feature = "cortex-m-fpu")]
#[derive(Debug, Deserialize, Serialize)]
pub struct CortexMFpuTarget {}
#[cfg(feature = "cortex-m-fpu")]
impl Target for CortexMFpuTarget {
    type Registers = fpu_registers::CortexMFpuRegisters;

    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>) {
        target.registers.capture();
        unsafe {
            stack::capture_stack(*target.registers.sp(), &mut target.stack);
        }
    }
}
