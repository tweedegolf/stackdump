use std::{cell::RefCell, rc::Rc};

use probe_rs::MemoryInterface;
use stackdump_core::{
    device_memory::MemoryReadError,
    memory_region::MemoryRegion,
    register_data::{RegisterBacking, VecRegisterData},
};

pub struct StackdumpCapturer<'a, 'probe>(RefCell<&'a mut probe_rs::Core<'probe>>);

impl<'a, 'probe> StackdumpCapturer<'a, 'probe> {
    pub fn new(core: &'a mut probe_rs::Core<'probe>) -> Self {
        Self(RefCell::new(core))
    }

    pub fn capture_registers<RB: RegisterBacking>(&mut self) -> VecRegisterData<RB> {
        todo!()
    }
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
            .read(address_range.start as u32, &mut buffer)
        {
            Ok(_) => Ok(Some(buffer)),
            Err(e) => Err(MemoryReadError(Rc::new(e))),
        }
    }
}
