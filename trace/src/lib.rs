pub mod cortex_m;

use std::{error::Error, fmt::Display};

pub use stackdump_core;
pub use stackdump_capture;

#[derive(Debug, Clone)]
pub struct Frame {
    pub function: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub frame_type: FrameType,
}

impl Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} ({:?})", self.function.clone().unwrap_or_else(|| "UNKNOWN".into()), self.frame_type)?;
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

pub trait Trace {
    fn trace(&self, elf_data: &[u8]) -> Result<Vec<Frame>, Box<dyn Error>>;
}
