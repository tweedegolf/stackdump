use arrayvec::ArrayVec;
use core::ops::Range;
use serde::{Deserialize, Serialize};

pub trait MemoryRegion {
    fn address_range(&self) -> Range<usize>;
    fn read_slice(&self, index: Range<usize>) -> Option<&[u8]>;
    fn len(&self) -> usize;

    fn read_u8(&self, index: usize) -> Option<u8> {
        self.read_slice(index..index + 1).map(|b| b[0])
    }

    #[cfg(feature = "std")]
    fn read_u32(&self, index: usize, endianness: gimli::RunTimeEndian) -> Option<u32> {
        let slice = self.read_slice(index..index + 4)?.try_into().unwrap();

        if gimli::Endianity::is_little_endian(endianness) {
            Some(u32::from_le_bytes(slice))
        } else {
            Some(u32::from_be_bytes(slice))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct ArrayVecMemoryRegion<const SIZE: usize> {
    pub start_address: u64,
    pub data: ArrayVec<u8, SIZE>,
}

impl<const SIZE: usize> ArrayVecMemoryRegion<SIZE> {
    pub fn new(start_address: u64, data: ArrayVec<u8, SIZE>) -> Self {
        Self {
            start_address,
            data,
        }
    }
}

impl<const SIZE: usize> MemoryRegion for ArrayVecMemoryRegion<SIZE> {
    fn address_range(&self) -> Range<usize> {
        self.start_address as usize..(self.start_address as usize + self.data.len())
    }

    fn read_slice(&self, index: Range<usize>) -> Option<&[u8]> {
        let start = index.start.checked_sub(self.start_address as usize)?;
        let end = index.end.checked_sub(self.start_address as usize)?;
        self.data.get(start..end)
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}

#[cfg(feature = "std")]
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct VecMemoryRegion {
    pub start_address: u64,
    pub data: Vec<u8>,
}

#[cfg(feature = "std")]
impl VecMemoryRegion {
    pub fn new(start_address: u64, data: Vec<u8>) -> Self {
        Self {
            start_address,
            data,
        }
    }
}

#[cfg(feature = "std")]
impl MemoryRegion for VecMemoryRegion {
    fn address_range(&self) -> Range<usize> {
        self.start_address as usize..(self.start_address as usize + self.data.len())
    }

    fn read_slice(&self, index: Range<usize>) -> Option<&[u8]> {
        let start = index.start.checked_sub(self.start_address as usize)?;
        let end = index.end.checked_sub(self.start_address as usize)?;
        self.data.get(start..end)
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}
