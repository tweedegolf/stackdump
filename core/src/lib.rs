#![cfg_attr(not(feature = "std"), no_std)]

use arrayvec::ArrayVec;
use core::marker::PhantomData;

pub struct Stackdump<T: Target, const STACK_SIZE: usize> {
    registers: T::Registers,
    stack: ArrayVec<u8, STACK_SIZE>,
    _phantom: PhantomData<T>,
}

impl<T: Target, const STACK_SIZE: usize> Stackdump<T, STACK_SIZE> {
    pub fn new() -> Self {
        Self {
            registers: Default::default(),
            stack: Default::default(),
            _phantom: PhantomData,
        }
    }
}

pub trait Target {
    type Registers: Registers;
}

pub trait Registers: Default {
    fn capture(&mut self);
}
