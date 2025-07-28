//! Module containing the definitions for memory regions

use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};

/// The identifier that is being used in the byte iterator to be able to differentiate between memory regions and register data
pub const MEMORY_REGION_IDENTIFIER: u8 = 0x01;

/// A collection of bytes that capture a memory region
#[cfg(feature = "std")]
pub trait MemoryRegion {
    /// Get the address range of this region
    fn range(&self) -> std::ops::Range<u64>;

    /// Returns the slice of memory that can be found at the given address_range.
    /// If the given address range is not fully within the captured region, then None is returned.
    fn read(
        &self,
        address_range: core::ops::Range<u64>,
    ) -> Result<Option<Vec<u8>>, crate::device_memory::MemoryReadError>;

    /// Reads a byte from the given address if it is present in the region
    fn read_u8(&self, address: u64) -> Result<Option<u8>, crate::device_memory::MemoryReadError> {
        Ok(self.read(address..address + 1)?.map(|b| b[0]))
    }

    /// Reads a u32 from the given address if it is present in the region
    fn read_u32(
        &self,
        address: u64,
        endianness: gimli::RunTimeEndian,
    ) -> Result<Option<u32>, crate::device_memory::MemoryReadError> {
        if let Some(slice) = self
            .read(address..address + 4)?
            .map(|slice| slice[..].try_into().unwrap())
        {
            if gimli::Endianity::is_little_endian(endianness) {
                Ok(Some(u32::from_le_bytes(slice)))
            } else {
                Ok(Some(u32::from_be_bytes(slice)))
            }
        } else {
            Ok(None)
        }
    }
}

/// A memory region that is backed by a stack allocated array
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
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
    pub fn bytes(&self) -> MemoryRegionIterator<'_> {
        MemoryRegionIterator::new(self.start_address, &self.data)
    }

    /// Clears the existing memory data and copies the new data from the given pointer
    ///
    /// If the data_len is greater than the capacity of this memory region, then this function will panic.
    ///
    /// ## Safety
    ///
    /// The entire block of memory from `data_ptr .. data_ptr + data_len` must be readable.
    /// (A memcpy must be possible with the pointer as source)
    pub unsafe fn copy_from_memory(&mut self, data_ptr: *const u8, data_len: usize) {
        self.start_address = data_ptr as u64;
        self.data.clear();

        assert!(data_len <= self.data.capacity());

        self.data.set_len(data_len);
        self.data.as_mut_ptr().copy_from(data_ptr, data_len);
    }
}

#[cfg(feature = "std")]
impl<const SIZE: usize> MemoryRegion for ArrayMemoryRegion<SIZE> {
    fn range(&self) -> std::ops::Range<u64> {
        self.start_address..self.start_address + self.data.len() as u64
    }

    fn read(
        &self,
        index: core::ops::Range<u64>,
    ) -> Result<Option<Vec<u8>>, crate::device_memory::MemoryReadError> {
        let start = match index.start.checked_sub(self.start_address) {
            Some(start) => start,
            None => return Ok(None),
        };
        let end = match index.end.checked_sub(self.start_address) {
            Some(end) => end,
            None => return Ok(None),
        };
        Ok(self
            .data
            .get(start as usize..end as usize)
            .map(|slice| slice.to_vec()))
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

        assert_eq!(
            iter.next().unwrap(),
            MEMORY_REGION_IDENTIFIER,
            "The given iterator is not for a memory region"
        );

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
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
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
    pub fn bytes(&self) -> MemoryRegionIterator<'_> {
        MemoryRegionIterator::new(self.start_address, &self.data)
    }

    /// Clears the existing memory data and copies the new data from the given pointer
    ///
    /// If the data_len is greater than the capacity of this memory region, then this function will panic.
    ///
    /// ## Safety
    ///
    /// The entire block of memory from `data_ptr .. data_ptr + data_len` must be readable.
    /// (A memcpy must be possible with the pointer as source)
    pub unsafe fn copy_from_memory(&mut self, data_ptr: *const u8, data_len: usize) {
        self.start_address = data_ptr as u64;
        self.data.clear();
        self.data.resize(data_len, 0);

        self.data.as_mut_ptr().copy_from(data_ptr, data_len);
    }
}

#[cfg(feature = "std")]
impl MemoryRegion for VecMemoryRegion {
    fn range(&self) -> std::ops::Range<u64> {
        self.start_address..self.start_address + self.data.len() as u64
    }

    fn read(
        &self,
        index: core::ops::Range<u64>,
    ) -> Result<Option<Vec<u8>>, crate::device_memory::MemoryReadError> {
        let start = match index.start.checked_sub(self.start_address) {
            Some(start) => start,
            None => return Ok(None),
        };
        let end = match index.end.checked_sub(self.start_address) {
            Some(end) => end,
            None => return Ok(None),
        };
        Ok(self
            .data
            .get(start as usize..end as usize)
            .map(|slice| slice.to_vec()))
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

        assert_eq!(
            iter.next().unwrap(),
            MEMORY_REGION_IDENTIFIER,
            "The given iterator is not for a memory region"
        );

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

/// A memory region that is backed by a slice
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct SliceMemoryRegion<'a> {
    data: &'a [u8],
}

impl<'a> SliceMemoryRegion<'a> {
    /// Creates a new memory region starting at the given address with the given data
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
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
    pub fn bytes(&self) -> MemoryRegionIterator<'_> {
        let start_address = self.data.as_ptr() as u64;
        MemoryRegionIterator::new(start_address, self.data)
    }

    /// This function is especially unsafe.
    /// The memory region will reference the given data for its entire lifetime.
    ///
    /// ## Safety
    ///
    /// The entire block of memory from `data_ptr .. data_ptr + data_len` must be readable.
    /// (A memcpy must be possible with the pointer as source)
    ///
    /// You must not have another reference to this block of memory or any object that resides in this memory
    /// during the entire lifetime of the object
    pub unsafe fn copy_from_memory(&mut self, data_ptr: *const u8, data_len: usize) {
        self.data = core::slice::from_raw_parts(data_ptr, data_len);
    }
}

#[cfg(feature = "std")]
impl<'a> MemoryRegion for SliceMemoryRegion<'a> {
    fn range(&self) -> std::ops::Range<u64> {
        let range = self.data.as_ptr_range();
        range.start as u64..range.end as u64
    }

    fn read(
        &self,
        index: core::ops::Range<u64>,
    ) -> Result<Option<Vec<u8>>, crate::device_memory::MemoryReadError> {
        let start_address = self.data.as_ptr() as u64;
        let start = match index.start.checked_sub(start_address) {
            Some(start) => start,
            None => return Ok(None),
        };
        let end = match index.end.checked_sub(start_address) {
            Some(end) => end,
            None => return Ok(None),
        };
        Ok(self
            .data
            .get(start as usize..end as usize)
            .map(|slice| slice.to_vec()))
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
            0 => {
                self.index += 1;
                Some(MEMORY_REGION_IDENTIFIER)
            }
            index @ 1..=8 => {
                self.index += 1;
                Some(self.start_address.to_le_bytes()[index - 1])
            }
            index @ 9..=16 => {
                self.index += 1;
                Some((self.data.len() as u64).to_le_bytes()[index - 9])
            }
            index => {
                self.index += 1;
                self.data.get(index - 17).copied()
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining_length = 17 + self.data.len() - self.index;
        (remaining_length, Some(remaining_length))
    }
}

impl<'a> ExactSizeIterator for MemoryRegionIterator<'a> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator() {
        let region = VecMemoryRegion::new(0x2000_0000, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let copied_region = VecMemoryRegion::from_iter(region.bytes());

        assert_eq!(region, copied_region);
    }

    #[test]
    fn iterator_len() {
        let region = VecMemoryRegion::new(0x2000_0000, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let iter = region.bytes();
        assert_eq!(iter.len(), iter.count());

        let mut iter = region.bytes();
        iter.nth(10).unwrap();
        assert_eq!(iter.len(), iter.count());
    }
}
