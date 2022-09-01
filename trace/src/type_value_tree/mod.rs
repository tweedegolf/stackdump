use self::{value::Value, variable_type::VariableType};
use stackdump_core::device_memory::MemoryReadError;
use std::{fmt::Debug, ops::Range};
use thiserror::Error;

pub mod rendering;
pub mod value;
pub mod variable_type;

pub type TypeValueNode<ADDR> = trees::Node<TypeValue<ADDR>>;
pub type TypeValueTree<ADDR> = trees::Tree<TypeValue<ADDR>>;

#[derive(Debug, Clone)]
pub struct TypeValue<ADDR: funty::Integral> {
    pub name: String,
    pub variable_type: VariableType,
    pub bit_range: Range<u64>,
    pub variable_value: Result<Value<ADDR>, VariableDataError>,
}

impl<ADDR: funty::Integral> TypeValue<ADDR> {
    pub fn bit_length(&self) -> u64 {
        self.bit_range.end - self.bit_range.start
    }

    pub fn bit_range_usize(&self) -> Range<usize> {
        self.bit_range.start as usize..self.bit_range.end as usize
    }
}

impl<ADDR: funty::Integral> Default for TypeValue<ADDR> {
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
    #[error("nullptr")]
    NullPointer,
    #[error("Some memory could not be read: {0}")]
    MemoryReadError(#[from] MemoryReadError),
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
    #[error("An operation is not implemented yet. Please open an issue at 'https://github.com/tweedegolf/stackdump': @ {file}:{line} => '{operation}'")]
    OperationNotImplemented {
        operation: String,
        file: &'static str,
        line: u32,
    },
}
