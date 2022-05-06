use self::{value::Value, variable_type::VariableType};
use std::ops::Range;
use thiserror::Error;

pub mod value;
pub mod variable_type;

pub type TypeValueTree<ADDR> = trees::Tree<TypeValue<ADDR>>;

#[derive(Debug, Clone)]
pub struct TypeValue<ADDR> {
    pub name: String,
    pub variable_type: VariableType,
    pub bit_range: Range<u64>,
    pub variable_value: Result<Value<ADDR>, VariableDataError>,
}

impl<ADDR> Default for TypeValue<ADDR> {
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
}
