#![doc = include_str!("../README.md")]
// #![warn(missing_docs)]

use render_colors::{ThemeColors, Theme};
pub use stackdump_core;

use crate::type_value_tree::variable_type::Archetype;
use gimli::{EndianReader, EvaluationResult, Piece, RunTimeEndian};
use std::{
    fmt::{Debug, Display},
    rc::Rc,
};
use type_value_tree::{rendering::render_type_value_tree, TypeValueTree};

pub mod error;
mod gimli_extensions;
pub mod platform;
pub mod render_colors;
pub mod type_value_tree;
mod variables;

type DefaultReader = EndianReader<RunTimeEndian, Rc<[u8]>>;

/// A source code location
#[derive(Debug, Clone, Default)]
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
pub struct Frame<ADDR: funty::Integral> {
    /// The name of the function the frame is in
    pub function: String,
    /// The code location of the frame
    pub location: Location,
    /// The type of the frame
    pub frame_type: FrameType,
    /// The variables and their values that are present in the frame
    pub variables: Vec<Variable<ADDR>>,
}

impl<ADDR: funty::Integral> Frame<ADDR> {
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
        theme: Theme,
    ) -> String {
        use std::fmt::Write;

        let mut display = String::new();

        writeln!(
            display,
            "{} ({})",
            theme.color_function(&self.function),
            theme.color_info(&self.frame_type)
        )
        .unwrap();

        let location_text = self.location.to_string();
        if !location_text.is_empty() {
            writeln!(display, "  at {}", theme.color_url(location_text)).unwrap();
        }

        let filtered_variables = self.variables.iter().filter(|v| {
            (show_inlined_vars || !v.kind.inlined)
                && (show_zero_sized_vars || !v.kind.zero_sized)
                && (show_parameters || !v.kind.parameter)
                // Hide the vtables
                && v.type_value.data().variable_type.archetype != Archetype::ObjectMemberPointer
        });
        if filtered_variables.clone().count() > 0 {
            writeln!(display, "  variables:").unwrap();
            for variable in filtered_variables {
                write!(display, "    {}", variable.display(theme)).unwrap();
            }
        }

        display
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

impl Display for FrameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameType::Function => write!(f, "Function"),
            FrameType::InlineFunction => write!(f, "Inline Function"),
            FrameType::Exception => write!(f, "Exception"),
            FrameType::Corrupted(reason) => write!(f, "Corrupted: \"{reason}\""),
            FrameType::Static => write!(f, "Static"),
        }
    }
}

/// A variable that was found in the tracing procedure
#[derive(Debug, Clone)]
pub struct Variable<ADDR: funty::Integral> {
    /// The name of the variable
    pub name: String,
    /// The kind of variable (normal, parameter, etc)
    pub kind: VariableKind,
    pub type_value: TypeValueTree<ADDR>,
    /// The code location of where this variable is declared
    pub location: Location,
}

impl<ADDR: funty::Integral> Variable<ADDR> {
    pub fn display(&self, theme: Theme) -> String {
        let mut kind_text = self.kind.to_string();
        if !kind_text.is_empty() {
            kind_text = theme.color_info(format!("({}) ", kind_text)).to_string();
        }

        let mut location_text = self.location.to_string();
        if !location_text.is_empty() {
            location_text = format!("at {}", theme.color_url(location_text));
        }

        format!(
            "{}{}: {} = {} ({})",
            kind_text,
            theme.color_variable_name(&self.name),
            theme.color_type_name(&self.type_value.root().data().variable_type.name),
            render_type_value_tree(&self.type_value, theme),
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
