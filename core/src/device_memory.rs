use std::ops::Range;

use crate::memory_region::MemoryRegion;

/// Object containing all memory regions (we have available) of the device
pub struct DeviceMemory {
    memory_regions: Vec<Box<dyn MemoryRegion>>,
}

impl DeviceMemory {
    pub fn new() -> Self {
        Self {
            memory_regions: Vec::new(),
        }
    }

    pub fn add_memory_region<M: MemoryRegion + 'static>(&mut self, region: M) {
        self.memory_regions.push(Box::new(region));
    }
    pub fn add_memory_region_boxed(&mut self, region: Box<dyn MemoryRegion>) {
        self.memory_regions.push(region);
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
}

impl Default for DeviceMemory {
    fn default() -> Self {
        Self::new()
    }
}
