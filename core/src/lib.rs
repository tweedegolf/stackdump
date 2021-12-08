#![cfg_attr(not(feature = "std"), no_std)]

use arrayvec::ArrayVec;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::ops::Range;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Stackdump<T, const STACK_SIZE: usize>
where
    T: Target,
{
    pub registers: T::Registers,
    pub stack: MemoryRegion<STACK_SIZE>,
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
}

impl<T, const STACK_SIZE: usize> TryFrom<&[u8]> for Stackdump<T, STACK_SIZE>
where
    T: Target,
{
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let (registers, left_over_slice) =
            <<T as Target>::Registers as RegisterContainer>::try_from(value)?;

        let stack_start_address =
            u64::from_le_bytes(left_over_slice.get(0..8).ok_or(())?.try_into().unwrap());
        let left_over_slice = &left_over_slice[8..];

        let mut stack = ArrayVec::new();
        stack
            .try_extend_from_slice(left_over_slice)
            .map_err(|_| ())?;

        Ok(Self {
            registers,
            stack: MemoryRegion::new(stack_start_address, stack),
            _phantom: PhantomData,
        })
    }
}

#[cfg(feature = "fuzzer")]
impl<'a, T, const STACK_SIZE: usize> arbitrary::Arbitrary<'a> for Stackdump<T, STACK_SIZE>
where
    T: Target,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        TryFrom::try_from(u.bytes(u.len())?).map_err(|_| arbitrary::Error::IncorrectFormat)
    }

    fn size_hint(depth: usize) -> (usize, Option<usize>) {
        let _ = depth;
        (
            T::Registers::min_data_size(),
            Some(T::Registers::min_data_size() + 8 + STACK_SIZE),
        )
    }
}

pub trait Target: Debug + DeserializeOwned + Serialize {
    type Registers: RegisterContainer;
    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>)
    where
        Self: Sized;
}

pub trait RegisterContainer: Default + Debug + DeserializeOwned + Serialize + Clone {
    fn try_from(data: &[u8]) -> Result<(Self, &[u8]), ()>;
    fn min_data_size() -> usize;
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct MemoryRegion<const SIZE: usize> {
    pub start_address: u64,
    pub data: ArrayVec<u8, SIZE>,
}

impl<const SIZE: usize> MemoryRegion<SIZE> {
    pub fn new(start_address: u64, data: ArrayVec<u8, SIZE>) -> Self {
        Self {
            start_address,
            data,
        }
    }

    pub fn address_range(&self) -> Range<usize> {
        self.start_address as usize..(self.start_address as usize + self.data.len())
    }

    pub fn read_slice(&self, index: Range<usize>) -> Option<&[u8]> {
        let start = index.start.checked_sub(self.start_address as usize)?;
        let end = index.end.checked_sub(self.start_address as usize)?;
        self.data.get(start..end)
    }

    #[cfg(feature = "std")]
    pub fn read_u8(&self, index: usize) -> Option<u8> {
        self.read_slice(index..index + 1).map(|b| b[0])
    }

    #[cfg(feature = "std")]
    pub fn read_u32<E: gimli::Endianity>(&self, index: usize, endianness: E) -> Option<u32> {
        let slice = self.read_slice(index..index + 4)?.try_into().unwrap();

        if endianness.is_little_endian() {
            Some(u32::from_le_bytes(slice))
        } else {
            Some(u32::from_be_bytes(slice))
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}
