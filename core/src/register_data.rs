use arrayvec::ArrayVec;
use core::fmt::Debug;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub trait RegisterBacking: Debug + Serialize + DeserializeOwned + Copy {
    /// Get the register in Little Endian format.
    /// This step is important on Big Endian architectures.
    fn to_le(&self) -> Self;
    /// Get Self from a slice of bytes in Little Endian format.
    fn from_le_slice(data: &[u8]) -> Self;
}
impl RegisterBacking for u8 {
    fn to_le(&self) -> Self {
        u8::to_le(*self)
    }

    fn from_le_slice(data: &[u8]) -> Self {
        Self::from_le_bytes(data.try_into().unwrap())
    }
}
impl RegisterBacking for u16 {
    fn to_le(&self) -> Self {
        u16::to_le(*self)
    }

    fn from_le_slice(data: &[u8]) -> Self {
        Self::from_le_bytes(data.try_into().unwrap())
    }
}
impl RegisterBacking for u32 {
    fn to_le(&self) -> Self {
        u32::to_le(*self)
    }

    fn from_le_slice(data: &[u8]) -> Self {
        Self::from_le_bytes(data.try_into().unwrap())
    }
}
impl RegisterBacking for u64 {
    fn to_le(&self) -> Self {
        u64::to_le(*self)
    }

    fn from_le_slice(data: &[u8]) -> Self {
        Self::from_le_bytes(data.try_into().unwrap())
    }
}

pub trait RegisterData<RB: RegisterBacking>: Debug {
    #[cfg(feature = "std")]
    fn register(&self, register: gimli::Register) -> Option<RB>;
    #[cfg(feature = "std")]
    fn register_ref(&self, register: gimli::Register) -> Option<&RB>;
    #[cfg(feature = "std")]
    fn register_mut(&mut self, register: gimli::Register) -> Option<&mut RB>;
}

#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
pub struct ArrayRegisterData<const SIZE: usize, RB> {
    /// The DWARF register number of the first register
    starting_register_number: u16,
    /// The values of the registers.
    /// The first value is of the register that is correlated with `starting_register_number`.
    /// Must be contiguous.
    pub registers: ArrayVec<RB, SIZE>,
}

impl<const SIZE: usize, RB: RegisterBacking> ArrayRegisterData<SIZE, RB> {
    pub fn new(starting_register_number: u16, registers: ArrayVec<RB, SIZE>) -> Self {
        Self {
            starting_register_number,
            registers,
        }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = u8> + 'a {
        RegisterDataIterator {
            index: 0,
            starting_register_number: self.starting_register_number,
            registers: &self.registers,
        }
    }
}

impl<const SIZE: usize, RB: RegisterBacking> RegisterData<RB> for ArrayRegisterData<SIZE, RB> {
    #[cfg(feature = "std")]
    fn register(&self, register: gimli::Register) -> Option<RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get(local_register_index as usize).copied()
    }
    #[cfg(feature = "std")]
    fn register_ref(&self, register: gimli::Register) -> Option<&RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get(local_register_index as usize)
    }
    #[cfg(feature = "std")]
    fn register_mut(&mut self, register: gimli::Register) -> Option<&mut RB> {
        let local_register_index = register.0.checked_sub(self.starting_register_number)?;
        self.registers.get_mut(local_register_index as usize)
    }
}

impl<const SIZE: usize, RB: RegisterBacking> FromIterator<u8> for ArrayRegisterData<SIZE, RB> {
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        let starting_register_number =
            u16::from_le_bytes([iter.next().unwrap(), iter.next().unwrap()]);

        let register_count = u16::from_le_bytes([iter.next().unwrap(), iter.next().unwrap()]);

        let mut registers = ArrayVec::new();
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

#[cfg(feature = "std")]
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq)]
pub struct VecRegisterData<RB> {
    /// The DWARF register number of the first register
    starting_register_number: u16,
    /// The values of the registers.
    /// The first value is of the register that is correlated with `starting_register_number`.
    /// Must be contiguous.
    pub registers: Vec<RB>,
}

#[cfg(feature = "std")]
impl<RB: RegisterBacking> VecRegisterData<RB> {
    pub fn new(starting_register_number: u16, registers: Vec<RB>) -> Self {
        Self {
            starting_register_number,
            registers,
        }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = u8> + 'a {
        RegisterDataIterator {
            index: 0,
            starting_register_number: self.starting_register_number,
            registers: &self.registers,
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

pub struct RegisterDataIterator<'a, RB: RegisterBacking> {
    starting_register_number: u16,
    registers: &'a [RB],
    index: usize,
}

impl<'a, RB: RegisterBacking> RegisterDataIterator<'a, RB> {
    pub fn new(starting_register_number: u16, registers: &'a [RB]) -> Self {
        Self {
            starting_register_number,
            registers,
            index: 0,
        }
    }
}

impl<'a, RB: RegisterBacking> Iterator for RegisterDataIterator<'a, RB> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterator() {
        let data = VecRegisterData::new(0xACAC, vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9, 0]);
        let copied_data = VecRegisterData::from_iter(data.iter());

        assert_eq!(data, copied_data);
    }
}
