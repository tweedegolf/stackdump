use self::{fpu_registers::CortexMFpuRegisters, registers::CortexMBaseRegisters};
use serde::{Deserialize, Serialize};
use stackdump_core::{RegisterContainer, Stackdump, Target};

pub mod fpu_registers;
pub mod registers;
#[cfg(feature = "capture")]
mod stack;

#[derive(Debug, Deserialize, Serialize, Default, Clone, PartialEq)]
pub struct CortexMRegisters {
    pub base: registers::CortexMBaseRegisters,
    pub fpu: fpu_registers::CortexMFpuRegisters,
}

impl RegisterContainer for CortexMRegisters {
    const DATA_SIZE: usize = 16 * 4 + 32 * 4;

    fn read(&self, offset: usize, buf: &mut [u8]) {
        let mut data = [0; Self::DATA_SIZE];
        data[..16 * 4].copy_from_slice(&self.base.copy_bytes());
        data[16 * 4..].copy_from_slice(&self.fpu.copy_bytes());
        buf.copy_from_slice(&data[offset..][..buf.len()]);
    }

    fn try_from(data: &[u8]) -> Result<Self, ()> {
        if data.len() < Self::DATA_SIZE {
            return Err(());
        }

        let (base_bytes, fpu_bytes) = data[..Self::DATA_SIZE].split_at(16 * 4);

        Ok(Self {
            base: CortexMBaseRegisters::from_bytes(base_bytes.try_into().unwrap()),
            fpu: CortexMFpuRegisters::from_bytes(fpu_bytes.try_into().unwrap()),
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CortexMTarget {}
impl Target for CortexMTarget {
    type Registers = CortexMRegisters;

    #[cfg(feature = "capture")]
    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>) {
        target.registers.base.capture();
        #[cfg(has_fpu)]
        target.registers.fpu.capture();
        target.stack.start_address = *target.registers.base.sp() as u64;
        unsafe {
            stack::capture_stack(*target.registers.base.sp(), &mut target.stack.data);
        }
    }

    #[cfg(not(feature = "capture"))]
    fn capture<const STACK_SIZE: usize>(_target: &mut Stackdump<Self, STACK_SIZE>) {
        unimplemented!("Activate the 'capture' feature to have this functionality");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_container_read_try_from() {
        let mut registers = CortexMRegisters::default();
        for i in 0..16 {
            *registers.base.register_mut(i) = i as u32;
        }
        for i in 0..32 {
            *registers.fpu.fpu_register_mut(i) = i as u32;
        }

        // Get the bytes
        let mut registers_buffer = [0; CortexMRegisters::DATA_SIZE];
        registers.read(0, &mut registers_buffer);

        // Turn the bytes into registers again
        let new_registers = RegisterContainer::try_from(&registers_buffer).unwrap();

        assert_eq!(registers, new_registers);
    }
}
