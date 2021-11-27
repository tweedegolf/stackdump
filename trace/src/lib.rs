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
    pub value: Option<String>,
    pub variable_type: VariableType,
}

impl Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{}: {} ({})",
            self.name,
            self.value.clone().unwrap_or("UNKNOWN".to_string()),
            self.variable_type.get_first_level_name(),
        )
    }
}

#[derive(Debug, Clone)]
pub enum VariableType {
    Structure {
        type_name: String,
        type_params: Vec<TemplateTypeParam>,
        members: Vec<StructureMember>,
    },
    Union {
        type_name: String,
        type_params: Vec<TemplateTypeParam>,
        members: Vec<StructureMember>,
    },
    Class {
        type_name: String,
        type_params: Vec<TemplateTypeParam>,
        members: Vec<StructureMember>,
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
        array_type: Box<VariableType>,
        member_type: Box<VariableType>,
        lower_bound: i64,
        count: u64,
    },
    EnumerationType {
        name: String,
        underlying_type: Box<VariableType>,
        enumerators: Vec<Enumerator>,
    },
}

impl VariableType {
    pub fn get_first_level_name(&self) -> String {
        match self {
            VariableType::Structure { type_name, .. } => type_name.clone(),
            VariableType::Union { type_name, .. } => type_name.clone(),
            VariableType::Class { type_name, .. } => type_name.clone(),
            VariableType::BaseType { name, .. } => name.clone(),
            VariableType::PointerType { name, .. } => name.clone(),
            VariableType::ArrayType { array_type, count, .. } => format!("[{};{}]", array_type.get_first_level_name(), count),
            VariableType::EnumerationType { name, .. } => name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StructureMember {
    pub name: String,
    pub member_type: VariableType,
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
