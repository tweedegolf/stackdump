#![cfg_attr(not(feature = "std"), no_std)]

use arrayvec::ArrayVec;
use core::fmt::Debug;
use core::marker::PhantomData;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Stackdump<T, const STACK_SIZE: usize>
where
    T: Target,
{
    pub registers: T::Registers,
    pub stack: ArrayVec<u8, STACK_SIZE>,
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

pub trait Target: Debug + DeserializeOwned + Serialize {
    type Registers: RegisterContainer;
    fn capture<const STACK_SIZE: usize>(target: &mut Stackdump<Self, STACK_SIZE>)
    where
        Self: Sized;
}

pub trait RegisterContainer: Default + Debug + DeserializeOwned + Serialize + Clone {}
