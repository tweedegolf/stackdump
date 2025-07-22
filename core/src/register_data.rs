//! Module containing the definitions for register data

use arrayvec::ArrayVec;
use core::fmt::Debug;
use serde::{Deserialize, Serialize};

/// The identifier that is being used in the byte iterator to be able to differentiate between register data and memory regions
pub const REGISTER_DATA_IDENTIFIER: u8 = 0x02;

/// A trait for reading registers from a register collection
///
/// This
pub trait RegisterData<RB: funty::Integral>: Debug {
    /// Try to get the value of the given register.
    /// Returns None if the register is not present in this collection.
    fn register(&self, register: gimli::Register) -> Option<RB>;
    /// Try to get a reference to the given register.
    /// Returns None if the register is not present in this collection.
    fn register_ref(&self, register: gimli::Register) -> Option<&RB>;
    /// Try to get a mutable reference to the given register.
    /// Returns None if the register is not present in this collection.
    fn register_mut(&mut self, register: gimli::Register) -> Option<&mut RB>;
}

/// A collection of registers, backed by a stack allocated array.
///
/// SIZE is the maximum amount of registers this collection can hold.
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct ArrayRegisterData<const SIZE: usize, RB> {
    /// The DWARF register number of the first register
    starting_register_number: u16,
    /// The values of the registers.
    /// The first value is of the register that is correlated with `starting_register_number`.
    /// Must be contiguous.
    registers: ArrayVec<RB, SIZE>,
}

impl<const SIZE: usize, RB: funty::Integral> ArrayRegisterData<SIZE, RB> {
    /// Create a new register collection backed by an array
    ///
    /// - The registers must be sequential according to the dwarf register numbers.
    /// - All registers that are in the collection must have their true value.
    pub fn new(starting_register: gimli::Register, registers: ArrayVec<RB, SIZE>) -> Self {
        Self {
            starting_register_number: starting_register.0,
            registers,
        }
    }

    /// Get a byte iterator for this collection.
    ///
    /// This iterator can be used to store the collection as bytes or to stream over a network.
    /// The iterated bytes include the length so that if you use the FromIterator implementation,
    /// it consumes only the bytes that are part of the collection.
    /// This means you can chain multiple of these iterators after each other.
    ///
    /// ```
    /// use arrayvec::ArrayVec;
    /// use stackdump_core::register_data::{ArrayRegisterData, RegisterData};
    ///
    /// let regs1 = ArrayRegisterData::<4, u32>::new(stackdump_core::gimli::Arm::R0, ArrayVec::from([1, 2, 3, 4]));
    /// let regs2 = ArrayRegisterData::<4, u32>::new(stackdump_core::gimli::Arm::R0, ArrayVec::from([5, 6, 7, 8]));
    ///
    /// let mut intermediate_buffer = Vec::new();
    ///
    /// intermediate_buffer.extend(regs1.bytes());
    /// intermediate_buffer.extend(regs2.bytes());
    ///
    /// let mut intermediate_iter = intermediate_buffer.iter().copied();
    ///
    /// assert_eq!(regs1, ArrayRegisterData::<4, u32>::from_iter(&mut intermediate_iter));
    /// assert_eq!(regs2, ArrayRegisterData::<4, u32>::from_iter(&mut intermediate_iter));
    /// ```
    pub fn bytes(&self) -> RegisterDataBytesIterator<'_, RB> {
        RegisterDataBytesIterator {
            index: 0,
            starting_register_number: self.starting_register_number,
            registers: &self.registers,
        }
    }
}

impl<const SIZE: usize, RB: funty::Integral> RegisterData<RB> for ArrayRegisterData<SIZE, RB> {
    fn register(&self, register: gimli::Register) -> Option<RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get(local_register_index as usize).copied()
    }
    fn register_ref(&self, register: gimli::Register) -> Option<&RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get(local_register_index as usize)
    }
    fn register_mut(&mut self, register: gimli::Register) -> Option<&mut RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get_mut(local_register_index as usize)
    }
}

impl<const SIZE: usize, RB> ArrayRegisterData<SIZE, RB>
where
    RB: funty::Integral,
    RB::Bytes: for<'a> TryFrom<&'a [u8]>,
{
    /// Try to build a [ArrayRegisterData] from an [IntoIterator<Item = u8>]
    pub fn try_from_iter<I: IntoIterator<Item = u8>>(
        iter: I,
    ) -> Result<Self, RegisterDataFromIterError> {
        use RegisterDataFromIterError::*;
        // Get the iterator.
        let mut iter = iter.into_iter();

        match iter.next() {
            Some(REGISTER_DATA_IDENTIFIER) => {}
            Some(id) => return Err(InvalidIdentifier(id)),
            None => return Err(NotEnoughItems),
        }

        // First the starting number is encoded
        let starting_register_number = u16::from_le_bytes([
            iter.next().ok_or(NotEnoughItems)?,
            iter.next().ok_or(NotEnoughItems)?,
        ]);

        // Second is how many registers there are
        let register_count = u16::from_le_bytes([
            iter.next().ok_or(NotEnoughItems)?,
            iter.next().ok_or(NotEnoughItems)?,
        ]);

        // Create the buffer we're storing the registers in
        let mut registers = ArrayVec::new();

        // We process everything byte-by-byte generically so every register has an unknown length
        // So we need to store the bytes temporarily until we have enough to fully read the bytes as a register
        let register_size = core::mem::size_of::<RB>();

        // Check that all register bytes will fit in `registers`
        if register_count as usize > SIZE {
            return Err(LengthTooBig(register_count, register_size));
        }
        
        let mut register_bytes_buffer = ArrayVec::<u8, 16>::new();
        
        let register_bytes_count = register_count as usize * register_size;
        for byte in (0..register_bytes_count).map(|_| iter.next().ok_or(NotEnoughItems)) {
            let byte = byte?;
            register_bytes_buffer.try_push(byte).map_err(|_| Corrupt)?;

            if register_bytes_buffer.len() == register_size {
                registers.push(RB::from_le_bytes(
                    register_bytes_buffer
                        .as_slice()
                        .try_into()
                        .map_err(|_| Corrupt)?,
                ));
                register_bytes_buffer = ArrayVec::new();
            }
        }

        if !register_bytes_buffer.is_empty() {
            return Err(Corrupt);
        }

        Ok(Self {
            starting_register_number,
            registers,
        })
    }
}

impl<const SIZE: usize, RB> FromIterator<u8> for ArrayRegisterData<SIZE, RB>
where
    RB: funty::Integral,
    RB::Bytes: for<'a> TryFrom<&'a [u8]>,
{
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        Self::try_from_iter(iter).unwrap()
    }
}

/// A collection of registers, backed by a vec.
#[cfg(feature = "std")]
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct VecRegisterData<RB> {
    /// The DWARF register number of the first register
    starting_register_number: u16,
    /// The values of the registers.
    /// The first value is of the register that is correlated with `starting_register_number`.
    /// Must be contiguous.
    registers: Vec<RB>,
}

#[cfg(feature = "std")]
impl<RB: funty::Integral> VecRegisterData<RB> {
    /// Create a new register collection backed by a vec
    ///
    /// - The registers must be sequential according to the dwarf register numbers.
    /// - All registers that are in the collection must have their true value.
    pub fn new(starting_register: gimli::Register, registers: Vec<RB>) -> Self {
        Self {
            starting_register_number: starting_register.0,
            registers,
        }
    }

    /// Get a byte iterator for this collection.
    ///
    /// This iterator can be used to store the collection as bytes or to stream over a network.
    /// The iterated bytes include the length so that if you use the FromIterator implementation,
    /// it consumes only the bytes that are part of the collection.
    /// This means you can chain multiple of these iterators after each other.
    ///
    /// ```
    /// use arrayvec::ArrayVec;
    /// use stackdump_core::register_data::{ArrayRegisterData, RegisterData};
    ///
    /// let regs1 = ArrayRegisterData::<4, u32>::new(stackdump_core::gimli::Arm::R0, ArrayVec::from([1, 2, 3, 4]));
    /// let regs2 = ArrayRegisterData::<4, u32>::new(stackdump_core::gimli::Arm::R0, ArrayVec::from([5, 6, 7, 8]));
    ///
    /// let mut intermediate_buffer = Vec::new();
    ///
    /// intermediate_buffer.extend(regs1.bytes());
    /// intermediate_buffer.extend(regs2.bytes());
    ///
    /// let mut intermediate_iter = intermediate_buffer.iter().copied();
    ///
    /// assert_eq!(regs1, ArrayRegisterData::<4, u32>::from_iter(&mut intermediate_iter));
    /// assert_eq!(regs2, ArrayRegisterData::<4, u32>::from_iter(&mut intermediate_iter));
    /// ```
    pub fn bytes(&self) -> RegisterDataBytesIterator<'_, RB> {
        RegisterDataBytesIterator {
            index: 0,
            starting_register_number: self.starting_register_number,
            registers: &self.registers,
        }
    }
}

#[cfg(feature = "std")]
impl<RB: funty::Integral> RegisterData<RB> for VecRegisterData<RB> {
    fn register(&self, register: gimli::Register) -> Option<RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get(local_register_index as usize).copied()
    }
    fn register_ref(&self, register: gimli::Register) -> Option<&RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get(local_register_index as usize)
    }
    fn register_mut(&mut self, register: gimli::Register) -> Option<&mut RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get_mut(local_register_index as usize)
    }
}

#[cfg(feature = "std")]
impl<RB> VecRegisterData<RB>
where
    RB: funty::Integral,
    RB::Bytes: for<'a> TryFrom<&'a [u8]>,
{
    /// Try to build a [VecRegisterData] from an [IntoIterator<Item = u8>]
    pub fn try_from_iter<I: IntoIterator<Item = u8>>(
        iter: I,
    ) -> Result<Self, RegisterDataFromIterError> {
        use RegisterDataFromIterError::*;

        let mut iter = iter.into_iter();

        match iter.next() {
            Some(REGISTER_DATA_IDENTIFIER) => {}
            Some(id) => return Err(InvalidIdentifier(id)),
            None => return Err(NotEnoughItems),
        }

        let starting_register_number = u16::from_le_bytes([
            iter.next().ok_or(NotEnoughItems)?,
            iter.next().ok_or(NotEnoughItems)?,
        ]);

        let register_count = u16::from_le_bytes([
            iter.next().ok_or(NotEnoughItems)?,
            iter.next().ok_or(NotEnoughItems)?,
        ]);

        let mut registers = Vec::new();
        let register_size = core::mem::size_of::<RB>();

        let mut register_bytes_buffer = ArrayVec::<u8, 16>::new();

        for byte in
            (0..register_count as usize * register_size).map(|_| iter.next().ok_or(NotEnoughItems))
        {
            let byte = byte?;
            register_bytes_buffer.try_push(byte).map_err(|_| Corrupt)?;

            if register_bytes_buffer.len() == register_size {
                registers.push(RB::from_le_bytes(
                    register_bytes_buffer
                        .as_slice()
                        .try_into()
                        .map_err(|_| Corrupt)?,
                ));
                register_bytes_buffer.clear();
            }
        }

        if !register_bytes_buffer.is_empty() {
            return Err(Corrupt);
        }

        Ok(Self {
            starting_register_number,
            registers,
        })
    }
}

#[cfg(feature = "std")]
impl<RB> FromIterator<u8> for VecRegisterData<RB>
where
    RB: funty::Integral,
    RB::Bytes: for<'a> TryFrom<&'a [u8]>,
{
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        Self::try_from_iter(iter).unwrap()
    }
}

/// An iterator that iterates over the serialized bytes of register data
pub struct RegisterDataBytesIterator<'a, RB: funty::Integral> {
    starting_register_number: u16,
    registers: &'a [RB],
    index: usize,
}

impl<'a, RB: funty::Integral> Iterator for RegisterDataBytesIterator<'a, RB> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        match self.index {
            0 => {
                self.index += 1;
                Some(REGISTER_DATA_IDENTIFIER)
            }
            index @ 1..=2 => {
                self.index += 1;
                Some(self.starting_register_number.to_le_bytes()[index - 1])
            }
            index @ 3..=4 => {
                self.index += 1;
                Some((self.registers.len() as u16).to_le_bytes()[index - 3])
            }
            index => {
                self.index += 1;

                let index = index - 5;
                let register_size = core::mem::size_of::<RB>();
                let register_index = index / register_size;
                let byte_index = index % register_size;

                // We get the number in forced little endian format
                let le_register = self.registers.get(register_index)?.to_le();
                // We can take a slice to it because we checked the length and we know it's in little endian
                let register_slice = unsafe {
                    core::slice::from_raw_parts(
                        &le_register as *const RB as *const u8,
                        register_size,
                    )
                };
                Some(register_slice[byte_index])
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining_length = 5 + self.registers.len() * core::mem::size_of::<RB>() - self.index;
        (remaining_length, Some(remaining_length))
    }
}

impl<'a, RB: funty::Integral> ExactSizeIterator for RegisterDataBytesIterator<'a, RB> {}

#[derive(Debug)]
/// Specifies what went wrong building a [RegisterData] from an iterator
pub enum RegisterDataFromIterError {
    /// The given iterator is not for a register set.
    /// First item from iterator yielded invalid identifier. Expected [REGISTER_DATA_IDENTIFIER]
    InvalidIdentifier(u8),
    /// Iterator specified length too big for declared register set
    LengthTooBig(u16, usize),
    /// Iterator did not yield enough items to build register set
    NotEnoughItems,
    /// Iterator data is corrupt in some other way
    Corrupt,
}

impl core::fmt::Display for RegisterDataFromIterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use RegisterDataFromIterError::*;
        match self {
            InvalidIdentifier(id) => write!(f, "Iterator is not for a register set. Started with {id}, expected {REGISTER_DATA_IDENTIFIER}"),
            LengthTooBig(count, size) => write!(f, "Iterator specified length too big for register set: {len}", len = *count as usize * size),
            NotEnoughItems => write!(f, "Iterator did not yield enough items to build register set"),
            Corrupt => write!(f, "Iterator data is corrupt")
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RegisterDataFromIterError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator() {
        let data = VecRegisterData::new(gimli::Arm::S12, vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let copied_data = VecRegisterData::from_iter(data.bytes());

        assert_eq!(data, copied_data);
    }

    #[test]
    fn iterator_length() {
        let data = VecRegisterData::new(gimli::Arm::S12, vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let iter = data.bytes();

        assert_eq!(iter.len(), iter.count());

        let mut iter = data.bytes();
        iter.nth(10).unwrap();
        assert_eq!(iter.len(), iter.count());
    }
}
