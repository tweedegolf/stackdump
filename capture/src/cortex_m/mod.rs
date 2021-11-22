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

impl RegisterContainer for CortexMRegisters {
    fn try_from(data: &[u8]) -> Result<(Self, &[u8]), ()> {
        let mut s = Self {
            base: Default::default(),
            fpu: Default::default(),
        };

        let mut data_left = data;

        for i in 0..17 {
            *s.base.register_mut(i) =
                u32::from_le_bytes((&data_left[..4]).try_into().map_err(|_| ())?);
            data_left = &data_left[4..];
        }

        for i in 0..33 {
            *s.fpu.fpu_register_mut(i) =
                u32::from_le_bytes((&data_left[..4]).try_into().map_err(|_| ())?);
            data_left = &data_left[4..];
        }

        Ok((s, data_left))
    }

    fn min_data_size() -> usize {
        17 * 4 + 33 * 4
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CortexMTarget {}
impl Target for CortexMTarget {
    type Registers = CortexMRegisters;

    #[cfg(feature = "capture")]
    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>) {
        target.registers.base.capture();
        if cfg!(feature = "cortex-m-fpu") {
            target.registers.fpu.capture();
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
