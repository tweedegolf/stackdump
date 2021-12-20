#![cfg_attr(not(feature = "std"), no_std)]

use arrayvec::ArrayVec;
use core::fmt::Debug;
use core::marker::PhantomData;
use memory_region::ArrayVecMemoryRegion;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "std")]
pub mod device_memory;
pub mod memory_region;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Stackdump<T, const STACK_SIZE: usize>
where
    T: Target,
{
    pub registers: T::Registers,
    pub stack: ArrayVecMemoryRegion<STACK_SIZE>,
    _phantom: PhantomData<T>,
}

impl<T, const STACK_SIZE: usize> Stackdump<T, STACK_SIZE>
where
    T: Target,
{
    pub fn new() -> Self {
        Self {
            registers: Default::default(),
            stack: Default::default(),
            _phantom: PhantomData,
        }
    }

    pub fn capture(&mut self) {
        T::capture(self);
    }

    pub fn get_reader<'s>(&'s self) -> StackdumpReader<'s, T, STACK_SIZE> {
        StackdumpReader {
            stackdump: self,
            bytes_read: 0,
        }
    }
}

impl<T, const STACK_SIZE: usize> TryFrom<&[u8]> for Stackdump<T, STACK_SIZE>
where
    T: Target,
{
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() < T::Registers::DATA_SIZE + 8 {
            return Err(());
        }

        let (register_bytes, stack_bytes) = value.split_at(T::Registers::DATA_SIZE);
        let (stack_address_bytes, stack_data_bytes) = stack_bytes.split_at(8);

        let registers = <<T as Target>::Registers as RegisterContainer>::try_from(register_bytes)?;

        let stack_start_address = u64::from_le_bytes(stack_address_bytes.try_into().unwrap());

        let mut stack = ArrayVec::new();
        stack
            .try_extend_from_slice(stack_data_bytes)
            .map_err(|_| ())?;

        Ok(Self {
            registers,
            stack: ArrayVecMemoryRegion::new(stack_start_address, stack),
            _phantom: PhantomData,
        })
    }
}

pub struct StackdumpReader<'s, T, const STACK_SIZE: usize>
where
    T: Target,
{
    stackdump: &'s Stackdump<T, STACK_SIZE>,
    bytes_read: usize,
}

impl<'s, T, const STACK_SIZE: usize> StackdumpReader<'s, T, STACK_SIZE>
where
    T: Target,
{
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, core::convert::Infallible> {
        // Should we read the registers?
        if self.bytes_read < T::Registers::DATA_SIZE {
            let data_amount = buf.len().min(T::Registers::DATA_SIZE - self.bytes_read);
            self.stackdump
                .registers
                .read(self.bytes_read, &mut buf[self.bytes_read..][..data_amount]);
            self.bytes_read += data_amount;
            return Ok(data_amount);
        }

        // Should we read the stack start address?
        if self.bytes_read < T::Registers::DATA_SIZE + 8 {
            let address_bytes_read = self.bytes_read - T::Registers::DATA_SIZE;
            let data_amount = buf.len().min(8 - address_bytes_read);
            let address_data = self.stackdump.stack.start_address.to_le_bytes();
            buf[..data_amount].copy_from_slice(&address_data[address_bytes_read..][..data_amount]);
            self.bytes_read += data_amount;
            return Ok(data_amount);
        }

        let stack_len = self.stackdump.stack.data.len();

        // Should we read the stack data?
        if self.bytes_read < T::Registers::DATA_SIZE + 8 + stack_len {
            let stack_bytes_read = self.bytes_read - T::Registers::DATA_SIZE - 8;
            let data_amount = buf.len().min(stack_len - stack_bytes_read);
            buf[..data_amount]
                .copy_from_slice(&self.stackdump.stack.data[stack_bytes_read..][..data_amount]);
            self.bytes_read += data_amount;
            return Ok(data_amount);
        }

        Ok(0)
    }
}

#[cfg(feature = "std")]
impl<'s, T, const STACK_SIZE: usize> std::io::Read for StackdumpReader<'s, T, STACK_SIZE>
where
    T: Target,
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(Self::read(self, buf).unwrap())
    }
}

pub trait Target: Debug + DeserializeOwned + Serialize {
    type Registers: RegisterContainer;
    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>)
    where
        Self: Sized;
}

pub trait RegisterContainer: Default + Debug + DeserializeOwned + Serialize + Clone {
    /// The size of the data of the registers
    const DATA_SIZE: usize;

    /// Reads the data of the registers into the given buffer.
    /// Panics when `offset + buf.len() > [DATA_SIZE]`
    fn read(&self, offset: usize, buf: &mut [u8]);

    /// Try to get the registers container from a slice
    fn try_from(data: &[u8]) -> Result<Self, ()>;
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
    struct TestRegisters(u8);
    impl RegisterContainer for TestRegisters {
        const DATA_SIZE: usize = 1;

        fn read(&self, offset: usize, buf: &mut [u8]) {
            if offset != 0 || buf.len() == 0 {
                panic!();
            }

            buf[0] = self.0
        }

        fn try_from(data: &[u8]) -> Result<Self, ()> {
            data.get(0).map(|v| TestRegisters(*v)).ok_or(())
        }
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestTarget {}
    impl Target for TestTarget {
        type Registers = TestRegisters;

        fn capture<const STACK_SIZE: usize>(_target: &mut Stackdump<Self, STACK_SIZE>)
        where
            Self: Sized,
        {
            todo!()
        }
    }

    #[test]
    fn stackdump_reader() {
        let mut stackdump: Stackdump<TestTarget, 2048> = Stackdump::new();

        stackdump.registers.0 = 55;
        stackdump.stack.start_address = 12345;
        stackdump
            .stack
            .data
            .try_extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 1, 2, 3, 4, 5, 6, 7, 8, 9])
            .unwrap();

        let mut buffer = Vec::new();
        let mut reader = stackdump.get_reader();
        reader.read_to_end(&mut buffer).unwrap();

        let new_stackdump =
            Stackdump::<TestTarget, 2048>::try_from(dbg!(buffer).as_slice()).unwrap();

        assert_eq!(stackdump, new_stackdump);
    }
}
