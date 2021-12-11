#![cfg_attr(not(feature = "std"), no_std)]

use arrayvec::ArrayVec;
use core::fmt::Debug;
use core::marker::PhantomData;
use memory_region::ArrayVecMemoryRegion;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "std")]
pub mod device_memory;
pub mod memory_region;

#[derive(Debug, Deserialize, Serialize)]
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
            stack: ArrayVecMemoryRegion::new(stack_start_address, stack),
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
