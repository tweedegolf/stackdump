//! All error types of the crate

use stackdump_core::device_memory::MissingRegisterError;
use thiserror::Error;

/// The main error type during the tracing procedure
#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum TraceError {
    #[error("The elf file does not contain the required `{0}` section")]
    MissingElfSection(String),
    #[error("The elf file could not be read: {0}")]
    ObjectReadError(#[from] addr2line::object::Error),
    #[error("An IO error occured: {0}")]
    IOError(#[from] std::io::Error),
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
    #[error("The type `{type_name}` has not been implemented yet")]
    TypeNotImplemented { type_name: String },
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
}
