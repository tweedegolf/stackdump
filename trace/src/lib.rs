//! # Stackdump Trace
//!
//! This crate implement stack tracing from the memory that was captured using the stackdump-capture crate.
//!
//! The aim is to extract as much information from the captured memory as possible.
//! As such, not only the stack frames are given, but also the variables that are present in each frame.
//! The value of these variables can only be read if their memory is captured.
//!
//! The minimum this crate needs is a stack & registers capture.
//! But if any variable is found that points outside the stack, like a String, then you´ll need
//! to have captured the heap memory as well. Otherwise it will just show the pointer value.
//!
//! Right now, only the cortex m target is supported.
//! A lot of the code could be refactored to work cross-platform.
//!
//! If you want to add a target, then please discuss and create an issue or PR.
//!
//! ## Example
//!
//! In this case we have a cortex m target with FPU.
//! A dump has been made with the two register captures first and then the stack capture.
//!
//! ```ignore
//! let dump: Vec<u8> = // Get your dump from somewhere
//! let elf: Vec<u8> = // Read your elf file
//!
//! let mut dump_iter = dump.iter().copied();
//!
//! let mut device_memory = DeviceMemory::new();
//!
//! device_memory.add_register_data(VecRegisterData::from_iter(&mut dump_iter));
//! device_memory.add_register_data(VecRegisterData::from_iter(&mut dump_iter));
//! device_memory.add_memory_region(VecMemoryRegion::from_iter(&mut dump_iter));
//!
//! let frames = cortex_m::trace(device_memory, &elf).unwrap();
//! for (i, frame) in frames.iter().enumerate() {
//!     println!("{}: {}", i, frame);
//! }
//! ```
//!
//! ## Reading live from the device
//!
//! In principle, if you have a way of reading the memory of the device directly (e.g. via probe-rs),
//! then it is possible to create types that implement `RegisterData` and `MemoryRegion` so that you can
//! insert those into the `DeviceMemory` instance.
//!

#![warn(missing_docs)]

use std::fmt::Display;

pub use stackdump_core;

pub mod cortex_m;
pub mod error;
mod gimli_extensions;

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
pub struct Frame {
    /// The name of the function the frame is in
    pub function: String,
    /// The code location of the frame
    pub location: Location,
    /// The type of the frame
    pub frame_type: FrameType,
    /// The variables and their values that are present in the frame
    pub variables: Vec<Variable>,
}

impl Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} ({:?})", self.function, self.frame_type)?;

        let location_text = self.location.to_string();
        if !location_text.is_empty() {
            writeln!(f, "  at {}", location_text)?;
        }

        if !self.variables.is_empty() {
            writeln!(f, "  variables:")?;
            for variable in &self.variables {
                write!(f, "    {}", variable)?;
            }
        }
        Ok(())
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
}

/// A variable that was found in the tracing procedure
#[derive(Debug, Clone)]
pub struct Variable {
    /// The name of the variable
    pub name: String,
    /// The kind of variable (normal, parameter, etc)
    pub kind: VariableKind,
    /// The value of the variable.
    /// - Ok if the value could be found and read.
    /// - Err with an explanation if the value could not be found or read. This happens e.g. when it points to memory that was not captured.
    pub value: Result<String, String>, // TODO: Don't turn everything into a string
    /// The precise type of the variable
    pub variable_type: VariableType,
    /// The code location of where this variable is declared
    pub location: Location,
}

impl Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut kind_text = self.kind.to_string();
        if !kind_text.is_empty() {
            kind_text = format!("({}) ", kind_text);
        }

        let mut location_text = self.location.to_string();
        if !location_text.is_empty() {
            location_text = format!(" at {}", location_text);
        }

        writeln!(
            f,
            "{}{}: {} ({}){}",
            kind_text,
            self.name,
            self.value
                .clone()
                .unwrap_or_else(|e| format!("Error({})", &e)),
            self.variable_type.type_name(),
            location_text,
        )
    }
}

/// Type representing what kind of variable something is
#[derive(Debug, Clone, Copy)]
pub enum VariableKind {
    /// A normal variable
    Normal,
    /// A parameter of a function
    Parameter,
    /// A variable that is actually part of another function (either our caller or our callee), but is present in our function already
    Inlined,
    /// A variable that is actually a parameter of another function (either our caller or our callee), but is present in our function already
    InlinedParameter,
}

impl VariableKind {
    /// Returns the combination of two kinds.
    /// E.g. `Parameter` and `Inlined` = `InlinedParameter`
    #[must_use]
    pub fn and(self, modifier: Self) -> Self {
        match (self, modifier) {
            (VariableKind::Normal, other) => other,
            (VariableKind::Parameter, VariableKind::Inlined) => Self::InlinedParameter,
            (VariableKind::Inlined, VariableKind::Parameter) => Self::InlinedParameter,
            (s, _) => s,
        }
    }
}

impl Display for VariableKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VariableKind::Normal => Ok(()),
            VariableKind::Parameter => write!(f, "parameter"),
            VariableKind::Inlined => write!(f, "inlined"),
            VariableKind::InlinedParameter => write!(f, "inlined parameter"),
        }
    }
}

/// The precise type of a variable according to the debug info
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum VariableType {
    /// The variable is a struct type
    Structure {
        /// The name of the struct type
        name: String,
        /// A collection of possible type parameters
        type_params: Vec<TemplateTypeParam>,
        /// The members (fields) of the struct
        members: Vec<StructureMember>,
        /// The in-memory size of the struct in bytes
        byte_size: u64,
    },
    /// The variable is a union type
    Union {
        /// The name of the union type
        name: String,
        /// A collection of possible type parameters
        type_params: Vec<TemplateTypeParam>,
        /// The members (fields) of the union
        members: Vec<StructureMember>,
        /// The in-memory size of the union in bytes
        byte_size: u64,
    },
    /// The variable is a class type
    Class {
        /// The name of the union type
        name: String,
        /// A collection of possible type parameters
        type_params: Vec<TemplateTypeParam>,
        /// The members (fields) of the union
        members: Vec<StructureMember>,
        /// The in-memory size of the union in bytes
        byte_size: u64,
    },
    /// The variable is a primitive type (e.g. integer, float, etc)
    BaseType {
        /// The name of the base type
        name: String,
        /// The kind of base type this is, encoded as in the DWARF debug format
        encoding: gimli::DwAte,
        /// The in-memory size of the base type in bytes
        byte_size: u64,
    },
    /// The variable is a pointer
    PointerType {
        /// The type name of the pointer
        name: String,
        /// The type of the thing this pointer points at
        pointee_type: Box<VariableType>,
    },
    /// The variable is an array
    ArrayType {
        /// The type of the elements in the array
        array_type: Box<VariableType>,
        /// The lower bound index
        lower_bound: i64,
        /// The amount of elements in the array
        count: u64,
        /// The optionally given size in bytes
        byte_size: Option<u64>,
    },
    /// The variable is an enum (c-style)
    EnumerationType {
        /// The type name of the enum
        name: String,
        /// The type that is used to represent the enum in memory (typically an integer)
        underlying_type: Box<VariableType>,
        /// The variants of the enum
        enumerators: Vec<Enumerator>,
    },
    /// The variable is a subroutine (method)
    Subroutine, // TODO: Do more with this
}

impl VariableType {
    /// Get the name of the type
    pub fn type_name(&self) -> String {
        match self {
            VariableType::Structure {
                name: type_name, ..
            } => type_name.clone(),
            VariableType::Union {
                name: type_name, ..
            } => type_name.clone(),
            VariableType::Class {
                name: type_name, ..
            } => type_name.clone(),
            VariableType::BaseType { name, .. } => name.clone(),
            VariableType::PointerType { name, .. } => name.clone(),
            VariableType::ArrayType {
                array_type, count, ..
            } => format!("[{};{}]", array_type.type_name(), count),
            VariableType::EnumerationType { name, .. } => name.clone(),
            VariableType::Subroutine => "Unknown subroutine".into(),
        }
    }

    /// Get the size in bytes that this type takes up in memory
    pub fn byte_size(&self) -> u64 {
        match self {
            VariableType::Structure { byte_size, .. } => *byte_size,
            VariableType::Union { byte_size, .. } => *byte_size,
            VariableType::Class { byte_size, .. } => *byte_size,
            VariableType::BaseType { byte_size, .. } => *byte_size,
            VariableType::PointerType { .. } => 4, // Cortex-m specific
            VariableType::ArrayType {
                array_type,
                count,
                byte_size,
                ..
            } => byte_size.unwrap_or_else(|| array_type.byte_size() * count),
            VariableType::EnumerationType {
                underlying_type, ..
            } => underlying_type.byte_size(),
            VariableType::Subroutine => 0,
        }
    }
}

/// Description of a member (field) of a structure
#[derive(Debug, Clone)]
pub struct StructureMember {
    /// The name of the member
    pub name: String,
    /// The type of the variable of the member
    pub member_type: VariableType,
    /// The offset in bytes from the base address of the structure that this member starts
    pub member_location: u64, // TODO: Sometimes this is not a simple number, but a location expression
}

/// A type parameter that can be present on a structure
#[derive(Debug, Clone)]
pub struct TemplateTypeParam {
    /// The name of the type parameter
    pub name: String,
    /// The type of the type parameter after it has been monomorphised
    pub template_type: VariableType,
}

/// A variant of an enum
#[derive(Debug, Clone)]
pub struct Enumerator {
    /// The name of the variant
    pub name: String,
    /// The value of the variant
    pub const_value: i64,
}
