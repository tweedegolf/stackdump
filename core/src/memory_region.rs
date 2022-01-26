use arrayvec::ArrayVec;
use core::fmt::Debug;
use core::ops::Range;
use serde::{Deserialize, Serialize};

pub trait MemoryRegion: Debug {
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

#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
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

    pub fn iter(&self) -> MemoryRegionIterator {
        MemoryRegionIterator::new(self.start_address, &self.data)
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

impl<const SIZE: usize> FromIterator<u8> for ArrayVecMemoryRegion<SIZE> {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        let start_address = u64::from_le_bytes([
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        ]);

        let length = u64::from_le_bytes([
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        ]);

        let data = ArrayVec::from_iter(iter.take(length as usize));

        Self {
            start_address,
            data,
        }
    }
}

#[cfg(feature = "std")]
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
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

    pub fn iter(&self) -> MemoryRegionIterator {
        MemoryRegionIterator::new(self.start_address, &self.data)
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

#[cfg(feature = "std")]
impl FromIterator<u8> for VecMemoryRegion {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        let start_address = u64::from_le_bytes([
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        ]);

        let length = u64::from_le_bytes([
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        ]);

        let data = Vec::from_iter(iter.take(length as usize));

        Self {
            start_address,
            data,
        }
    }
}

pub struct MemoryRegionIterator<'a> {
    start_address: u64,
    data: &'a [u8],
    index: usize,
}

impl<'a> MemoryRegionIterator<'a> {
    pub fn new(start_address: u64, data: &'a [u8]) -> Self {
        Self {
            start_address,
            data,
            index: 0,
        }
    }
}

impl<'a> Iterator for MemoryRegionIterator<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        match self.index {
            index @ 0..=7 => {
                self.index += 1;
                Some(self.start_address.to_le_bytes()[index])
            }
            index @ 8..=15 => {
                self.index += 1;
                Some((self.data.len() as u64).to_le_bytes()[index - 8])
            }
            index => {
                self.index += 1;
                self.data.get(index - 16).copied()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator() {
        let region = VecMemoryRegion::new(0x2000_0000, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let copied_region = VecMemoryRegion::from_iter(region.iter());

        assert_eq!(region, copied_region);
    }
}
