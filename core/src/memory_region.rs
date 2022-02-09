//! Module containing the definitions for memory regions

use arrayvec::ArrayVec;
use core::fmt::Debug;
use core::ops::Range;
use serde::{Deserialize, Serialize};

/// A collection of bytes that capture a memory region
pub trait MemoryRegion: Debug {
    /// The address range of the region where the start is the first address that the region captures
    fn address_range(&self) -> Range<u64>;
    /// Returns the slice of memory that can be found at the given address_range.
    /// If the given address range is not fully within the captured region, then None is returned.
    fn read_slice(&self, address_range: Range<u64>) -> Option<&[u8]>;
    /// Returns the size of the region
    fn len(&self) -> u64 {
        self.address_range().end - self.address_range().start
    }

    /// Reads a byte from the given address if it is present in the region
    fn read_u8(&self, address: u64) -> Option<u8> {
        self.read_slice(address..address + 1).map(|b| b[0])
    }

    /// Reads a u32 from the given address if it is present in the region
    fn read_u32(&self, address: u64, endianness: gimli::RunTimeEndian) -> Option<u32> {
        let slice = self.read_slice(address..address + 4)?.try_into().unwrap();

        if gimli::Endianity::is_little_endian(endianness) {
            Some(u32::from_le_bytes(slice))
        } else {
            Some(u32::from_be_bytes(slice))
        }
    }

    /// Get a byte iterator for this region.
    ///
    /// This iterator can be used to store the region as bytes or to stream over a network.
    /// The iterated bytes include the length so that if you use the FromIterator implementation,
    /// it consumes only the bytes that are part of the collection.
    /// This means you can chain multiple of these iterators after each other.
    ///
    /// ```
    /// use arrayvec::ArrayVec;
    /// use stackdump_core::memory_region::{ArrayMemoryRegion, MemoryRegion};
    ///
    /// let region1 = ArrayMemoryRegion::<4>::new(0, ArrayVec::from([1, 2, 3, 4]));
    /// let region2 = ArrayMemoryRegion::<4>::new(100, ArrayVec::from([5, 6, 7, 8]));
    ///
    /// let mut intermediate_buffer = Vec::new();
    ///
    /// intermediate_buffer.extend(region1.bytes());
    /// intermediate_buffer.extend(region2.bytes());
    ///
    /// let mut intermediate_iter = intermediate_buffer.iter();
    ///
    /// assert_eq!(region1, ArrayMemoryRegion::<4>::from_iter(&mut intermediate_iter));
    /// assert_eq!(region2, ArrayMemoryRegion::<4>::from_iter(&mut intermediate_iter));
    /// ```
    fn bytes(&self) -> MemoryRegionIterator;

    /// Clears the existing memory data and copies the new data from the given pointer
    ///
    /// If the data_len is greater than the capacity of this memory region, then this function will panic.
    fn copy_from_memory(&mut self, data_ptr: *const u8, data_len: usize);
}

/// A memory region that is backed by a stack allocated array
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
pub struct ArrayMemoryRegion<const SIZE: usize> {
    start_address: u64,
    data: ArrayVec<u8, SIZE>,
}

impl<const SIZE: usize> ArrayMemoryRegion<SIZE> {
    /// Creates a new memory region starting at the given address with the given data
    pub fn new(start_address: u64, data: ArrayVec<u8, SIZE>) -> Self {
        Self {
            start_address,
            data,
        }
    }
}

impl<const SIZE: usize> MemoryRegion for ArrayMemoryRegion<SIZE> {
    fn address_range(&self) -> Range<u64> {
        self.start_address..(self.start_address + self.data.len() as u64)
    }

    fn read_slice(&self, index: Range<u64>) -> Option<&[u8]> {
        let start = index.start.checked_sub(self.start_address)?;
        let end = index.end.checked_sub(self.start_address)?;
        self.data.get(start as usize..end as usize)
    }

    fn bytes(&self) -> MemoryRegionIterator {
        MemoryRegionIterator::new(self.start_address, &self.data)
    }

    fn copy_from_memory(&mut self, data_ptr: *const u8, data_len: usize) {
        self.start_address = data_ptr as u64;
        self.data.clear();

        assert!(data_len <= self.data.capacity());

        unsafe {
            self.data.set_len(data_len);
            self.data.as_mut_ptr().copy_from(data_ptr, data_len);
        }
    }
}

impl<'a, const SIZE: usize> FromIterator<&'a u8> for ArrayMemoryRegion<SIZE> {
    fn from_iter<T: IntoIterator<Item = &'a u8>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().copied())
    }
}

impl<const SIZE: usize> FromIterator<u8> for ArrayMemoryRegion<SIZE> {
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

/// A memory region that is backed by a stack allocated array
#[cfg(feature = "std")]
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
pub struct VecMemoryRegion {
    start_address: u64,
    data: Vec<u8>,
}

#[cfg(feature = "std")]
impl VecMemoryRegion {
    /// Creates a new memory region starting at the given address with the given data
    pub fn new(start_address: u64, data: Vec<u8>) -> Self {
        Self {
            start_address,
            data,
        }
    }
}

#[cfg(feature = "std")]
impl MemoryRegion for VecMemoryRegion {
    fn address_range(&self) -> Range<u64> {
        self.start_address..(self.start_address + self.data.len() as u64)
    }

    fn read_slice(&self, index: Range<u64>) -> Option<&[u8]> {
        let start = index.start.checked_sub(self.start_address)?;
        let end = index.end.checked_sub(self.start_address)?;
        self.data.get(start as usize..end as usize)
    }

    fn bytes(&self) -> MemoryRegionIterator {
        MemoryRegionIterator::new(self.start_address, &self.data)
    }

    fn copy_from_memory(&mut self, data_ptr: *const u8, data_len: usize) {
        self.start_address = data_ptr as u64;
        self.data.clear();
        self.data.resize(data_len, 0);

        unsafe {
            self.data.as_mut_ptr().copy_from(data_ptr, data_len);
        }
    }
}

#[cfg(feature = "std")]
impl<'a> FromIterator<&'a u8> for VecMemoryRegion {
    fn from_iter<T: IntoIterator<Item = &'a u8>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().copied())
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

/// An iterator that iterates over the serialized bytes of a memory region
pub struct MemoryRegionIterator<'a> {
    start_address: u64,
    data: &'a [u8],
    index: usize,
}

impl<'a> MemoryRegionIterator<'a> {
    fn new(start_address: u64, data: &'a [u8]) -> Self {
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
        let copied_region = VecMemoryRegion::from_iter(region.bytes());

        assert_eq!(region, copied_region);
    }
}
