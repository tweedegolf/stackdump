use self::{value::Value, variable_type::VariableType};
use std::{ops::Range, fmt::Debug};
use thiserror::Error;

pub mod value;
pub mod variable_type;
pub mod rendering;

pub type TypeValueNode<ADDR> = trees::Node<TypeValue<ADDR>>;
pub type TypeValueTree<ADDR> = trees::Tree<TypeValue<ADDR>>;

#[derive(Debug, Clone)]
pub struct TypeValue<ADDR: AddressType> {
    pub name: String,
    pub variable_type: VariableType,
    pub bit_range: Range<u64>,
    pub variable_value: Result<Value<ADDR>, VariableDataError>,
}

impl<ADDR: AddressType> TypeValue<ADDR> {
    pub fn bit_length(&self) -> u64 {
        self.bit_range.end - self.bit_range.start
    }

    pub fn bit_range_usize(&self) -> Range<usize> {
        self.bit_range.start as usize..self.bit_range.end as usize
    }
}

impl<ADDR: AddressType> Default for TypeValue<ADDR> {
    fn default() -> Self {
        Self {
            name: Default::default(),
            variable_type: Default::default(),
            bit_range: Default::default(),
            variable_value: Err(VariableDataError::NoDataAvailable),
        }
    }
}

#[derive(Error, Debug, Clone)]
pub enum VariableDataError {
    #[error("The data of the variable has an invalid size of {bits} bits")]
    InvalidSize { bits: usize },
    #[error("The base type {base_type} is not supported (yet). Data: {data:X?}")]
    UnsupportedBaseType {
        base_type: gimli::DwAte,
        data: bitvec::prelude::BitVec<u8, bitvec::order::Lsb0>,
    },
    #[error("The data of the pointer is invalid")]
    InvalidPointerData,
    #[error("The data of the variable is not available")]
    NoDataAvailable,
    #[error("The data is not available in device memory: {0}")]
    NoDataAvailableAt(String),
    #[error("Optimized away")]
    OptimizedAway,
    #[error("A required step of the location evaluation logic has not been implemented yet: {0}")]
    UnimplementedLocationEvaluationStep(String),
}

pub trait AddressType: Debug + Copy {}

impl AddressType for u8 {}
impl AddressType for u16 {}
impl AddressType for u32 {}
impl AddressType for u64 {}
impl AddressType for u128 {}
