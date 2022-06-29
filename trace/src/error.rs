//! All error types of the crate

use std::rc::Rc;

use gimli::EvaluationResult;
use stackdump_core::device_memory::{MemoryReadError, MissingRegisterError};
use thiserror::Error;

use crate::{DefaultReader, type_value_tree::VariableDataError};

/// The main error type during the tracing procedure
#[allow(missing_docs)]
#[derive(Error, Debug, Clone)]
pub enum TraceError {
    #[error("The elf file does not contain the required `{0}` section")]
    MissingElfSection(String),
    #[error("The elf file could not be read: {0}")]
    ObjectReadError(#[from] addr2line::object::Error),
    #[error("An IO error occured: {0}")]
    IOError(Rc<std::io::Error>),
    #[error("Some memory could not be read: {0}")]
    MemoryReadError(#[from] MemoryReadError),
    #[error("Some debug information could not be parsed: {0}")]
    DebugParseError(#[from] gimli::Error),
    #[error("An entry ({entry_tag} (@ .debug_info offset {entry_debug_info_offset:X?})) is missing an expected attribute: {attribute_name}")]
    MissingAttribute {
        entry_debug_info_offset: Option<u64>,
        entry_tag: String,
        attribute_name: String,
    },
    #[error("An attribute ({attribute_name}) has the wrong value type: {value_type_name}")]
    WrongAttributeValueType {
        attribute_name: String,
        value_type_name: &'static str,
    },
    #[error("The tag `{tag_name}` @`{entry_debug_info_offset:#X}` has not been implemented yet")]
    TagNotImplemented {
        tag_name: String,
        entry_debug_info_offset: usize,
    },
    #[error("An operation is not implemented yet. Please open an issue at 'https://github.com/tweedegolf/stackdump': @ {file}:{line} => '{0}'")]
    OperationNotImplemented {
        operation: String,
        file: &'static str,
        line: u32,
    },
    #[error("A child was expected for {entry_tag}, but it was not there")]
    ExpectedChildNotPresent { entry_tag: String },
    #[error("The frame base is not known yet")]
    UnknownFrameBase,
    #[error("The dwarf unit for a `pc` of {pc:#X} could not be found")]
    DwarfUnitNotFound { pc: u64 },
    #[error("A number could not be converted to another type")]
    NumberConversionError,
    #[error("Register {0:?} is required, but is not available in the device memory")]
    MissingRegister(#[from] MissingRegisterError),
    #[error("Memory was expected to be available at address {0:#X}, but wasn't")]
    MissingMemory(u64),
    #[error("{member_name} of {object_name} has unexpected tag {member_tag}")]
    UnexpectedMemberTag {
        object_name: String,
        member_name: String,
        member_tag: gimli::DwTag,
    },
    #[error(
        "A pointer with the name {pointer_name} has an unexpected class value of {class_value}"
    )]
    UnexpectedPointerClass {
        pointer_name: String,
        class_value: gimli::DwAddr,
    },
    #[error("A required step of the location evaluation logic has not been implemented yet: {0:?}")]
    LocationEvaluationStepNotImplemented(Rc<EvaluationResult<DefaultReader>>),
    #[error("A variable couldn't be read: {0}")]
    VariableDataError(#[from]VariableDataError),

}

impl From<std::io::Error> for TraceError {
    fn from(e: std::io::Error) -> Self {
        Self::IOError(Rc::new(e))
    }
}
