//! Module containing the definitions for device memory, a summation of all available memory that was captured

use crate::{
    memory_region::MemoryRegion,
    register_data::{RegisterBacking, RegisterData},
};
use std::{error::Error, fmt::Display, ops::Range, rc::Rc};

/// An error to signal that a register is not present
#[derive(Debug, Clone, Copy)]
pub struct MissingRegisterError(gimli::Register);
impl Display for MissingRegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Missing register: {}",
            gimli::Arm::register_name(self.0)
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("{}", self.0 .0))
        )
    }
}
impl Error for MissingRegisterError {}

/// An error to signal that memory could not be read
#[derive(Debug, Clone)]
pub struct MemoryReadError(pub Rc<dyn Error>);
impl Display for MemoryReadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Memory read error: {}", self.0)
    }
}
impl Error for MemoryReadError {}
impl PartialEq for MemoryReadError {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Object containing all memory regions (we have available) of the device
pub struct DeviceMemory<RB: RegisterBacking> {
    register_data: Vec<Box<dyn RegisterData<RB>>>,
    memory_regions: Vec<Box<dyn MemoryRegion>>,
}

impl<RB: RegisterBacking> DeviceMemory<RB> {
    /// Creates a new instance of the device memory
    pub fn new() -> Self {
        Self {
            register_data: Vec::new(),
            memory_regions: Vec::new(),
        }
    }

    /// Adds a memory region to the device memory
    pub fn add_memory_region<M: MemoryRegion + 'static>(&mut self, region: M) {
        self.memory_regions.push(Box::new(region));
    }
    /// Adds a memory region to the device memory
    pub fn add_memory_region_boxed(&mut self, region: Box<dyn MemoryRegion>) {
        self.memory_regions.push(region);
    }
    /// Adds register data to the device memory
    pub fn add_register_data<RD: RegisterData<RB> + 'static>(&mut self, data: RD) {
        self.register_data.push(Box::new(data));
    }
    /// Adds register data to the device memory
    pub fn add_register_data_boxed(&mut self, data: Box<dyn RegisterData<RB>>) {
        self.register_data.push(data);
    }

    /// Returns the slice of memory that can be found at the given address_range.
    /// If the given address range is not fully within one of the captured regions present in the device memory, then None is returned.
    pub fn read_slice(
        &self,
        address_range: Range<u64>,
    ) -> Result<Option<Vec<u8>>, MemoryReadError> {
        for mr in self.memory_regions.iter() {
            if let Some(v) = mr.read(address_range.clone())? {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    /// Reads a byte from the given address if it is present in one of the captured regions present in the device memory
    pub fn read_u8(&self, address: u64) -> Result<Option<u8>, MemoryReadError> {
        for mr in self.memory_regions.iter() {
            if let Some(v) = mr.read_u8(address)? {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    /// Reads a u32 from the given address if it is present in one of the captured regions present in the device memory
    pub fn read_u32(
        &self,
        address: u64,
        endianness: gimli::RunTimeEndian,
    ) -> Result<Option<u32>, MemoryReadError> {
        for mr in self.memory_regions.iter() {
            if let Some(v) = mr.read_u32(address, endianness)? {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    /// Try to get the value of the given register. Returns an error if the register is not present in any of the register collections.
    pub fn register(&self, register: gimli::Register) -> Result<RB, MissingRegisterError> {
        self.register_data
            .iter()
            .find_map(|registers| registers.register(register))
            .ok_or(MissingRegisterError(register))
    }

    /// Try to get a reference to the given register. Returns an error if the register is not present in any of the register collections.
    pub fn register_ref(&self, register: gimli::Register) -> Result<&RB, MissingRegisterError> {
        self.register_data
            .iter()
            .find_map(|registers| registers.register_ref(register))
            .ok_or(MissingRegisterError(register))
    }

    /// Try to get a mutable reference to the given register. Returns an error if the register is not present in any of the register collections.
    pub fn register_mut(
        &mut self,
        register: gimli::Register,
    ) -> Result<&mut RB, MissingRegisterError> {
        self.register_data
            .iter_mut()
            .find_map(|registers| registers.register_mut(register))
            .ok_or(MissingRegisterError(register))
    }
}

impl<RB: RegisterBacking> Default for DeviceMemory<RB> {
    fn default() -> Self {
        Self::new()
    }
}
