pub mod cortex_m;

use std::{error::Error, fmt::Display};

pub use stackdump_capture;
pub use stackdump_core;

#[derive(Debug, Clone)]
pub struct Frame {
    pub function: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub frame_type: FrameType,
    pub variables: Vec<Variable>,
}

impl Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} ({:?})",
            self.function.clone().unwrap_or_else(|| "UNKNOWN".into()),
            self.frame_type
        )?;

        if let Some(file) = self.file.clone() {
            write!(f, "  at {}", file)?;
            if let Some(line) = self.line {
                write!(f, ":{}", line)?;
                if let Some(column) = self.column {
                    write!(f, ":{}", column)?;
                }
            }
            writeln!(f)?;
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

#[derive(Debug, Clone)]
pub enum FrameType {
    Function,
    InlineFunction,
    Exception,
    Corrupted(String),
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub value: Result<String, String>,
    pub variable_type: VariableType, // TODO: Make this platform independent. Right now this only works for cortex-m
}

impl Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}: {} ({})",
            self.name,
            self.value
                .clone()
                .unwrap_or_else(|e| format!("Error({})", &e)),
            self.variable_type.get_first_level_name(),
        )
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum VariableType {
    Structure {
        type_name: String,
        type_params: Vec<TemplateTypeParam>,
        members: Vec<StructureMember>,
        byte_size: u64,
    },
    Union {
        type_name: String,
        type_params: Vec<TemplateTypeParam>,
        members: Vec<StructureMember>,
        byte_size: u64,
    },
    Class {
        type_name: String,
        type_params: Vec<TemplateTypeParam>,
        members: Vec<StructureMember>,
        byte_size: u64,
    },
    BaseType {
        name: String,
        encoding: gimli::DwAte,
        byte_size: u64,
    },
    PointerType {
        name: String,
        pointee_type: Box<VariableType>,
    },
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
    EnumerationType {
        name: String,
        underlying_type: Box<VariableType>,
        enumerators: Vec<Enumerator>,
    },
    Subroutine,
}

impl VariableType {
    pub fn get_first_level_name(&self) -> String {
        match self {
            VariableType::Structure { type_name, .. } => type_name.clone(),
            VariableType::Union { type_name, .. } => type_name.clone(),
            VariableType::Class { type_name, .. } => type_name.clone(),
            VariableType::BaseType { name, .. } => name.clone(),
            VariableType::PointerType { name, .. } => name.clone(),
            VariableType::ArrayType {
                array_type, count, ..
            } => format!("[{};{}]", array_type.get_first_level_name(), count),
            VariableType::EnumerationType { name, .. } => name.clone(),
            VariableType::Subroutine => "Unknown subroutine".into(),
        }
    }

    pub fn get_raw_name(&self) -> &str {
        match self {
            VariableType::Structure { .. } => "Structure",
            VariableType::Union { .. } => "Union",
            VariableType::Class { .. } => "Class",
            VariableType::BaseType { .. } => "BaseType",
            VariableType::PointerType { .. } => "PointerType",
            VariableType::ArrayType { .. } => "ArrayType",
            VariableType::EnumerationType { .. } => "EnumerationType",
            VariableType::Subroutine => "Subroutine",
        }
    }

    pub fn get_variable_size(&self) -> u64 {
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
            } => byte_size.unwrap_or_else(|| array_type.get_variable_size() * count),
            VariableType::EnumerationType {
                underlying_type, ..
            } => underlying_type.get_variable_size(),
            VariableType::Subroutine => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StructureMember {
    pub name: String,
    pub member_type: VariableType,
    pub member_location: u64,
}

#[derive(Debug, Clone)]
pub struct TemplateTypeParam {
    pub name: String,
    pub template_type: VariableType,
}

#[derive(Debug, Clone)]
pub struct Enumerator {
    pub name: String,
    pub const_value: i64,
}

pub trait Trace {
    fn trace(&self, elf_data: &[u8]) -> Result<Vec<Frame>, Box<dyn Error>>;
}
