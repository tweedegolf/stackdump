use std::{fmt::Display, ops::Range};

use crate::{
    memory_region::MemoryRegion,
    register_data::{RegisterBacking, RegisterData},
};

#[derive(Debug, Clone, Copy)]
pub struct MissingRegisterError(gimli::Register);
impl Display for MissingRegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Missing register: {}", self.0 .0)
    }
}
impl std::error::Error for MissingRegisterError {}

/// Object containing all memory regions (we have available) of the device
pub struct DeviceMemory<RB: RegisterBacking> {
    register_data: Vec<Box<dyn RegisterData<RB>>>,
    memory_regions: Vec<Box<dyn MemoryRegion>>,
}

impl<RB: RegisterBacking> DeviceMemory<RB> {
    pub fn new() -> Self {
        Self {
            register_data: Vec::new(),
            memory_regions: Vec::new(),
        }
    }

    pub fn add_memory_region<M: MemoryRegion + 'static>(&mut self, region: M) {
        self.memory_regions.push(Box::new(region));
    }
    pub fn add_memory_region_boxed(&mut self, region: Box<dyn MemoryRegion>) {
        self.memory_regions.push(region);
    }

    pub fn add_register_data<RD: RegisterData<RB> + 'static>(&mut self, data: RD) {
        self.register_data.push(Box::new(data));
    }
    pub fn add_register_data_boxed(&mut self, data: Box<dyn RegisterData<RB>>) {
        self.register_data.push(data);
    }

    pub fn read_slice(&self, index: Range<usize>) -> Option<&[u8]> {
        let index = &index;
        self.memory_regions
            .iter()
            .find_map(|mr| mr.read_slice(index.clone()))
    }

    pub fn read_u8(&self, index: usize) -> Option<u8> {
        self.read_slice(index..index + 1).map(|b| b[0])
    }

    pub fn read_u32(&self, index: usize, endianness: gimli::RunTimeEndian) -> Option<u32> {
        let slice = self.read_slice(index..index + 4)?.try_into().unwrap();

        if gimli::Endianity::is_little_endian(endianness) {
            Some(u32::from_le_bytes(slice))
        } else {
            Some(u32::from_be_bytes(slice))
        }
    }

    pub fn register(&self, register: gimli::Register) -> Result<RB, MissingRegisterError> {
        self.register_data
            .iter()
            .find_map(|registers| registers.register(register))
            .ok_or_else(|| MissingRegisterError(register))
    }
    pub fn register_ref(&self, register: gimli::Register) -> Result<&RB, MissingRegisterError> {
        self.register_data
            .iter()
            .find_map(|registers| registers.register_ref(register))
            .ok_or_else(|| MissingRegisterError(register))
    }
    pub fn register_mut(
        &mut self,
        register: gimli::Register,
    ) -> Result<&mut RB, MissingRegisterError> {
        self.register_data
            .iter_mut()
            .find_map(|registers| registers.register_mut(register))
            .ok_or_else(|| MissingRegisterError(register))
    }
}

impl<RB: RegisterBacking> Default for DeviceMemory<RB> {
    fn default() -> Self {
        Self::new()
    }
}
