#![doc = include_str!("../README.md")]
// #![warn(missing_docs)]

use gimli::{EndianReader, EvaluationResult, Piece, RunTimeEndian};
use std::{fmt::{Display, Debug}, rc::Rc};
use type_value_tree::{TypeValueTree, AddressType, rendering::render_type_value_tree};

pub use stackdump_core;

pub mod cortex_m;
pub mod error;
mod gimli_extensions;
pub mod type_value_tree;

type DefaultReader = EndianReader<RunTimeEndian, Rc<[u8]>>;

/// A source code location
#[derive(Debug, Clone)]
pub struct Location {
    /// The file path of the piece of code
    pub file: Option<String>,
    /// The line of the piece of code
    pub line: Option<u64>,
    /// The column of the piece of code
    pub column: Option<u64>,
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(file) = self.file.clone() {
            write!(f, "{}", file)?;
            if let Some(line) = self.line {
                write!(f, ":{}", line)?;
                if let Some(column) = self.column {
                    write!(f, ":{}", column)?;
                }
            }
        }

        Ok(())
    }
}

/// An object containing a de-inlined stack frame.
/// Exceptions/interrupts are also a frame.
#[derive(Debug, Clone)]
pub struct Frame<ADDR: AddressType> {
    /// The name of the function the frame is in
    pub function: String,
    /// The code location of the frame
    pub location: Location,
    /// The type of the frame
    pub frame_type: FrameType,
    /// The variables and their values that are present in the frame
    pub variables: Vec<Variable<ADDR>>,
}

impl<ADDR: AddressType> Frame<ADDR> {
    /// Get a string that can be displayed to a user
    ///
    /// - `show_parameters`: When true, any variable that is a parameter will be shown
    /// - `show_inlined_vars`: When true, any variable that is inlined will be shown
    /// - `show_zero_sized_vars`: When true, any variable that is zero-sized will be shown
    pub fn display(
        &self,
        show_parameters: bool,
        show_inlined_vars: bool,
        show_zero_sized_vars: bool,
    ) -> String {
        use std::fmt::Write;

        let mut display = String::new();

        writeln!(display, "{} ({:?})", self.function, self.frame_type).unwrap();

        let location_text = self.location.to_string();
        if !location_text.is_empty() {
            writeln!(display, "  at {}", location_text).unwrap();
        }

        let filtered_variables = self.variables.iter().filter(|v| {
            (show_inlined_vars || !v.kind.inlined)
                && (show_zero_sized_vars || !v.kind.zero_sized)
                && (show_parameters || !v.kind.parameter)
        });
        if filtered_variables.clone().count() > 0 {
            writeln!(display, "  variables:").unwrap();
            for variable in filtered_variables {
                write!(display, "    {}", variable).unwrap();
            }
        }

        display
    }
}

impl<ADDR: AddressType> Display for Frame<ADDR> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display(true, false, false))
    }
}

/// The type of a frame
#[derive(Debug, Clone)]
pub enum FrameType {
    /// A real function
    Function,
    /// An inline function (does not really exist in the binary)
    InlineFunction,
    /// An interrupt or exception
    Exception,
    /// The frame could not be (fully) read, so the frame is corrupted. The string says what the problem is.
    Corrupted(String),
    /// This is not really a frame, but has all the statically available data
    Static,
}

/// A variable that was found in the tracing procedure
#[derive(Debug, Clone)]
pub struct Variable<ADDR: AddressType> {
    /// The name of the variable
    pub name: String,
    /// The kind of variable (normal, parameter, etc)
    pub kind: VariableKind,
    pub type_value: TypeValueTree<ADDR>,
    /// The code location of where this variable is declared
    pub location: Location,
}

impl<ADDR: AddressType> Variable<ADDR> {
    pub fn render_type(&self) -> &str {
        &self.type_value.root().data().variable_type.name
    }
    pub fn render_value(&self) -> String {
        render_type_value_tree(&self.type_value)
    }
}

impl<ADDR: AddressType> Display for Variable<ADDR> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut kind_text = self.kind.to_string();
        if !kind_text.is_empty() {
            kind_text = format!("({}) ", kind_text);
        }

        let mut location_text = self.location.to_string();
        if !location_text.is_empty() {
            location_text = format!("at {}", location_text);
        }

        writeln!(
            f,
            "{}{}: {} = {} ({})",
            kind_text,
            self.name,
            self.render_type(),
            self.render_value(),
            location_text,
        )
    }
}

#[derive(Debug, Clone)]
pub enum VariableLocationResult {
    /// The DW_AT_location attribute is missing
    NoLocationAttribute,
    /// The location list could not be found in the ELF
    LocationListNotFound,
    /// This variable is not present in memory at this point
    NoLocationFound,
    /// A required step of the location evaluation logic has not been implemented yet
    LocationEvaluationStepNotImplemented(Rc<EvaluationResult<DefaultReader>>),
    /// The variable is split up into multiple pieces of memory
    LocationsFound(Vec<Piece<DefaultReader, usize>>),
}

/// Type representing what kind of variable something is
#[derive(Debug, Clone, Copy, Default)]
pub struct VariableKind {
    /// The variable is a zero-sized type
    pub zero_sized: bool,
    /// The variable is actually part of another function (either our caller or our callee), but is present in our function already
    pub inlined: bool,
    /// The variable is a parameter of a function
    pub parameter: bool,
}

impl Display for VariableKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut elements = vec![];

        if self.zero_sized {
            elements.push("zero-sized");
        }
        if self.inlined {
            elements.push("inlined");
        }
        if self.parameter {
            elements.push("parameter");
        }

        write!(f, "{}", elements.join(" "))
    }
}

// /// The precise type of a variable according to the debug info
// #[derive(Debug, Clone)]
// #[non_exhaustive]
// pub enum VariableType {
//     /// The variable is a struct type
//     Structure {
//         /// The name of the struct type
//         name: String,
//         /// A collection of possible type parameters
//         type_params: Vec<TemplateTypeParam>,
//         /// The members (fields) of the struct
//         members: Vec<StructureMember>,
//         /// The in-memory size of the struct in bytes
//         byte_size: u64,
//     },
//     /// The variable is a union type
//     Union {
//         /// The name of the union type
//         name: String,
//         /// A collection of possible type parameters
//         type_params: Vec<TemplateTypeParam>,
//         /// The members (fields) of the union
//         members: Vec<StructureMember>,
//         /// The in-memory size of the union in bytes
//         byte_size: u64,
//     },
//     /// The variable is a class type
//     Class {
//         /// The name of the union type
//         name: String,
//         /// A collection of possible type parameters
//         type_params: Vec<TemplateTypeParam>,
//         /// The members (fields) of the union
//         members: Vec<StructureMember>,
//         /// The in-memory size of the union in bytes
//         byte_size: u64,
//     },
//     /// The variable is a primitive type (e.g. integer, float, etc)
//     BaseType {
//         /// The name of the base type
//         name: String,
//         /// The kind of base type this is, encoded as in the DWARF debug format
//         encoding: gimli::DwAte,
//         /// The in-memory size of the base type in bytes
//         byte_size: u64,
//     },
//     /// The variable is a pointer
//     PointerType {
//         /// The type name of the pointer
//         name: String,
//         /// The type of the thing this pointer points at
//         pointee_type: Box<VariableType>,
//     },
//     /// The variable is an array
//     ArrayType {
//         /// The type of the elements in the array
//         array_type: Box<VariableType>,
//         /// The lower bound index
//         lower_bound: i64,
//         /// The amount of elements in the array
//         count: u64,
//         /// The optionally given size in bytes
//         byte_size: Option<u64>,
//     },
//     /// The variable is an enum (c-style)
//     EnumerationType {
//         /// The type name of the enum
//         name: String,
//         /// The type that is used to represent the enum in memory (typically an integer)
//         underlying_type: Box<VariableType>,
//         /// The variants of the enum
//         enumerators: Vec<Enumerator>,
//     },
//     /// The variable is a subroutine (method)
//     Subroutine, // TODO: Do more with this
// }

// impl VariableType {
//     /// Get the name of the type
//     pub fn type_name(&self) -> String {
//         match self {
//             VariableType::Structure {
//                 name: type_name, ..
//             } => type_name.clone(),
//             VariableType::Union {
//                 name: type_name, ..
//             } => type_name.clone(),
//             VariableType::Class {
//                 name: type_name, ..
//             } => type_name.clone(),
//             VariableType::BaseType { name, .. } => name.clone(),
//             VariableType::PointerType { name, .. } => name.clone(),
//             VariableType::ArrayType {
//                 array_type, count, ..
//             } => format!("[{};{}]", array_type.type_name(), count),
//             VariableType::EnumerationType { name, .. } => name.clone(),
//             VariableType::Subroutine => "Unknown subroutine".into(),
//         }
//     }

//     /// Get the size in bytes that this type takes up in memory
//     pub fn byte_size(&self) -> u64 {
//         match self {
//             VariableType::Structure { byte_size, .. } => *byte_size,
//             VariableType::Union { byte_size, .. } => *byte_size,
//             VariableType::Class { byte_size, .. } => *byte_size,
//             VariableType::BaseType { byte_size, .. } => *byte_size,
//             VariableType::PointerType { .. } => 4, // Cortex-m specific
//             VariableType::ArrayType {
//                 array_type,
//                 count,
//                 byte_size,
//                 ..
//             } => byte_size.unwrap_or_else(|| array_type.byte_size() * count),
//             VariableType::EnumerationType {
//                 underlying_type, ..
//             } => underlying_type.byte_size(),
//             VariableType::Subroutine => 0,
//         }
//     }
// }

// /// Description of a member (field) of a structure
// #[derive(Debug, Clone)]
// pub struct StructureMember {
//     /// The name of the member
//     pub name: String,
//     /// The type of the variable of the member
//     pub member_type: VariableType,
//     /// The offset in bytes from the base address of the structure that this member starts
//     pub member_location: u64, // TODO: Sometimes this is not a simple number, but a location expression
// }

// /// A type parameter that can be present on a structure
// #[derive(Debug, Clone)]
// pub struct TemplateTypeParam {
//     /// The name of the type parameter
//     pub name: String,
//     /// The type of the type parameter after it has been monomorphised
//     pub template_type: VariableType,
// }

// /// A variant of an enum
// #[derive(Debug, Clone)]
// pub struct Enumerator {
//     /// The name of the variant
//     pub name: String,
//     /// The value of the variant
//     pub const_value: i64,
// }
