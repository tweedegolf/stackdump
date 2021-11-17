use serde::{Deserialize, Serialize};
use stackdump_core::{RegisterContainer, Stackdump, Target};

pub mod fpu_registers;
pub mod registers;
#[cfg(feature = "capture")]
mod stack;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct CortexMRegisters {
    pub base: registers::CortexMBaseRegisters,
    pub fpu: fpu_registers::CortexMFpuRegisters,
}

impl RegisterContainer for CortexMRegisters {}

#[derive(Debug, Deserialize, Serialize)]
pub struct CortexMTarget {}
impl Target for CortexMTarget {
    type Registers = CortexMRegisters;

    #[cfg(feature = "capture")]
    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>) {
        target.registers.base.capture();
        if cfg!(feature = "cortex-m-fpu") {
            target
                .registers
                .fpu
                .capture();
        }
        unsafe {
            stack::capture_stack(*target.registers.base.sp(), &mut target.stack);
        }
    }

    #[cfg(not(feature = "capture"))]
    fn capture<const STACK_SIZE: usize>(_target: &mut Stackdump<Self, STACK_SIZE>) {
        unimplemented!("Activate the 'capture' feature to have this functionality");
    }
}
