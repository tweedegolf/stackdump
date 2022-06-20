use probe_rs::MemoryInterface;
use stackdump_core::{
    device_memory::MemoryReadError, memory_region::MemoryRegion, register_data::VecRegisterData,
};
use std::{cell::RefCell, rc::Rc};

pub struct StackdumpCapturer<'a, 'probe>(RefCell<&'a mut probe_rs::Core<'probe>>);

impl<'a, 'probe> StackdumpCapturer<'a, 'probe> {
    pub fn new(core: &'a mut probe_rs::Core<'probe>) -> Self {
        Self(RefCell::new(core))
    }

    pub fn capture_core_registers(&mut self) -> Result<VecRegisterData<u32>, probe_rs::Error> {
        let mut register_data = Vec::new();
        let registers = self.0.get_mut().registers();

        for register in registers.registers() {
            register_data.push(self.0.get_mut().read_core_reg(register)?)
        }

        let starting_register = match self.0.get_mut().architecture() {
            probe_rs::Architecture::Arm => stackdump_core::gimli::Arm::R0,
            probe_rs::Architecture::Riscv => stackdump_core::gimli::RiscV::X0,
        };

        Ok(VecRegisterData::new(starting_register, register_data))
    }

    // Available on probe-rs master:
    // pub fn capture_fpu_registers(
    //     &mut self,
    // ) -> Result<Option<VecRegisterData<u32>>, probe_rs::Error> {
    //     let registers = self.0.get_mut().registers();

    //     match registers.fpu_registers() {
    //         Some(fpu_registers) => {
    //             let mut register_data = Vec::new();

    //             for register in fpu_registers {
    //                 register_data.push(self.0.get_mut().read_core_reg(register)?)
    //             }

    //             let starting_register = match self.0.get_mut().architecture() {
    //                 probe_rs::Architecture::Arm => stackdump_core::gimli::Arm::S0,
    //                 probe_rs::Architecture::Riscv => stackdump_core::gimli::RiscV::F0,
    //             };

    //             Ok(Some(VecRegisterData::new(starting_register, register_data)))
    //         }
    //         None => Ok(None),
    //     }
    // }
}

impl<'a, 'probe> MemoryRegion for StackdumpCapturer<'a, 'probe> {
    fn read(
        &self,
        address_range: std::ops::Range<u64>,
    ) -> Result<Option<Vec<u8>>, MemoryReadError> {
        let mut buffer = vec![0; address_range.clone().count()];

        // Truncating to u32 is alright because probe-rs only supports 32-bit devices
        match self
            .0
            .borrow_mut()
            .read(address_range.start as _, &mut buffer)
        {
            Ok(_) => Ok(Some(buffer)),
            Err(e) => Err(MemoryReadError(Rc::new(e))),
        }
    }
}
