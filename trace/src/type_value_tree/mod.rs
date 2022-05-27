use self::{value::Value, variable_type::VariableType};
use std::{ops::Range, fmt::{Debug, UpperHex}};
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
            variable_value: Err(VariableDataError::Unknown),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum VariableDataError {
    #[error("Data has invalid size of {bits} bits")]
    InvalidSize { bits: usize },
    #[error("Unsupported base type {base_type}. Data: {data:X?}")]
    UnsupportedBaseType {
        base_type: gimli::DwAte,
        data: bitvec::prelude::BitVec<u8, bitvec::order::Lsb0>,
    },
    #[error("Pointer data is invalid")]
    InvalidPointerData,
    #[error("Data not available")]
    NoDataAvailable,
    #[error("Data not available: {0}")]
    NoDataAvailableAt(String),
    #[error("Optimized away")]
    OptimizedAway,
    #[error("Required step of location evaluation logic not implemented: {0}")]
    UnimplementedLocationEvaluationStep(String),
    #[error("Unknown")]
    Unknown,

}

pub trait AddressType: UpperHex + Debug + Copy + Eq {}

impl AddressType for u8 {}
impl AddressType for u16 {}
impl AddressType for u32 {}
impl AddressType for u64 {}
impl AddressType for u128 {}
