//! Module containing the definitions for register data

use arrayvec::ArrayVec;
use core::fmt::Debug;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// A trait for reading registers from a register collection
///
/// This
pub trait RegisterData<RB: RegisterBacking>: Debug {
    /// Try to get the value of the given register.
    /// Returns None if the register is not present in this collection.
    fn register(&self, register: gimli::Register) -> Option<RB>;
    /// Try to get a reference to the given register.
    /// Returns None if the register is not present in this collection.
    fn register_ref(&self, register: gimli::Register) -> Option<&RB>;
    /// Try to get a mutable reference to the given register.
    /// Returns None if the register is not present in this collection.
    fn register_mut(&mut self, register: gimli::Register) -> Option<&mut RB>;

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
    /// let mut intermediate_iter = intermediate_buffer.iter();
    ///
    /// assert_eq!(regs1, ArrayRegisterData::<4, u32>::from_iter(&mut intermediate_iter));
    /// assert_eq!(regs2, ArrayRegisterData::<4, u32>::from_iter(&mut intermediate_iter));
    /// ```
    fn bytes<'a>(&'a self) -> RegisterDataBytesIterator<'a, RB>;
}

/// A collection of registers, backed by a stack allocated array.
///
/// SIZE is the maximum amount of registers this collection can hold.
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
pub struct ArrayRegisterData<const SIZE: usize, RB> {
    /// The DWARF register number of the first register
    starting_register_number: u16,
    /// The values of the registers.
    /// The first value is of the register that is correlated with `starting_register_number`.
    /// Must be contiguous.
    registers: ArrayVec<RB, SIZE>,
}

impl<const SIZE: usize, RB: RegisterBacking> ArrayRegisterData<SIZE, RB> {
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
}

impl<const SIZE: usize, RB: RegisterBacking> RegisterData<RB> for ArrayRegisterData<SIZE, RB> {
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

    fn bytes<'a>(&'a self) -> RegisterDataBytesIterator<'a, RB> {
        RegisterDataBytesIterator {
            index: 0,
            starting_register_number: self.starting_register_number,
            registers: &self.registers,
        }
    }
}

impl<'a, const SIZE: usize, RB: RegisterBacking> FromIterator<&'a u8>
    for ArrayRegisterData<SIZE, RB>
{
    fn from_iter<T: IntoIterator<Item = &'a u8>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().copied())
    }
}

impl<const SIZE: usize, RB: RegisterBacking> FromIterator<u8> for ArrayRegisterData<SIZE, RB> {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        // Get the iterator. We assume that it is in the same format as the bytes function outputs
        let mut iter = iter.into_iter();

        // First the starting number is encoded
        let starting_register_number =
            u16::from_le_bytes([iter.next().unwrap(), iter.next().unwrap()]);

        // Second is how many registers there are
        let register_count = u16::from_le_bytes([iter.next().unwrap(), iter.next().unwrap()]);

        // Create the buffer we're storing the registers in
        let mut registers = ArrayVec::new();

        // We process everything byte-by-byte generically so every register has an unknown length
        // So we need to store the bytes temporarily until we have enough to fully read the bytes as a register
        let register_size = core::mem::size_of::<RB>();
        let mut register_bytes_buffer = ArrayVec::<u8, 16>::new();

        for byte in (0..register_count as usize * register_size).map(|_| iter.next().unwrap()) {
            register_bytes_buffer.push(byte);

            if register_bytes_buffer.len() == register_size {
                registers.push(RB::from_le_slice(&register_bytes_buffer));
                register_bytes_buffer.clear();
            }
        }

        assert!(register_bytes_buffer.is_empty());

        Self {
            starting_register_number,
            registers,
        }
    }
}

/// A collection of registers, backed by a vec.
#[cfg(feature = "std")]
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
pub struct VecRegisterData<RB> {
    /// The DWARF register number of the first register
    starting_register_number: u16,
    /// The values of the registers.
    /// The first value is of the register that is correlated with `starting_register_number`.
    /// Must be contiguous.
    registers: Vec<RB>,
}

#[cfg(feature = "std")]
impl<RB: RegisterBacking> VecRegisterData<RB> {
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
}

#[cfg(feature = "std")]
impl<RB: RegisterBacking> RegisterData<RB> for VecRegisterData<RB> {
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

    fn bytes<'a>(&'a self) -> RegisterDataBytesIterator<'a, RB> {
        RegisterDataBytesIterator {
            index: 0,
            starting_register_number: self.starting_register_number,
            registers: &self.registers,
        }
    }
}

#[cfg(feature = "std")]
impl<'a, RB: RegisterBacking> FromIterator<&'a u8> for VecRegisterData<RB> {
    fn from_iter<T: IntoIterator<Item = &'a u8>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().copied())
    }
}

#[cfg(feature = "std")]
impl<RB: RegisterBacking> FromIterator<u8> for VecRegisterData<RB> {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        let starting_register_number =
            u16::from_le_bytes([iter.next().unwrap(), iter.next().unwrap()]);

        let register_count = u16::from_le_bytes([iter.next().unwrap(), iter.next().unwrap()]);

        let mut registers = Vec::new();
        let register_size = core::mem::size_of::<RB>();

        let mut register_bytes_buffer = ArrayVec::<u8, 16>::new();

        for byte in (0..register_count as usize * register_size).map(|_| iter.next().unwrap()) {
            register_bytes_buffer.push(byte);

            if register_bytes_buffer.len() == register_size {
                registers.push(RB::from_le_slice(&register_bytes_buffer));
                register_bytes_buffer.clear();
            }
        }

        assert!(register_bytes_buffer.is_empty());

        Self {
            starting_register_number,
            registers,
        }
    }
}

/// An iterator that iterates over the serialized bytes of register data
pub struct RegisterDataBytesIterator<'a, RB: RegisterBacking> {
    starting_register_number: u16,
    registers: &'a [RB],
    index: usize,
}

impl<'a, RB: RegisterBacking> Iterator for RegisterDataBytesIterator<'a, RB> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        match self.index {
            index @ 0..=1 => {
                self.index += 1;
                Some(self.starting_register_number.to_le_bytes()[index])
            }
            index @ 2..=3 => {
                self.index += 1;
                Some((self.registers.len() as u16).to_le_bytes()[index - 2])
            }
            index => {
                self.index += 1;

                let index = index - 4;
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
}

/// A trait representing the types that can be used to back the data of a register.
/// This is different for every platform, so we need to be generic about it.
pub trait RegisterBacking: Debug + Serialize + DeserializeOwned + Copy {
    /// Get the register in Little Endian format.
    /// This step is important on Big Endian architectures.
    fn to_le(&self) -> Self;
    /// Get Self from a slice of bytes in Little Endian format.
    fn from_le_slice(data: &[u8]) -> Self;
}

macro_rules! impl_register_backing {
    ($t:ty) => {
        impl RegisterBacking for $t {
            fn to_le(&self) -> Self {
                Self::to_le(*self)
            }

            fn from_le_slice(data: &[u8]) -> Self {
                Self::from_le_bytes(data.try_into().unwrap())
            }
        }
    };
}

impl_register_backing!(u8);
impl_register_backing!(u16);
impl_register_backing!(u32);
impl_register_backing!(u64);
impl_register_backing!(u128);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator() {
        let data = VecRegisterData::new(gimli::Arm::S12, vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let copied_data = VecRegisterData::from_iter(data.bytes());

        assert_eq!(data, copied_data);
    }
}