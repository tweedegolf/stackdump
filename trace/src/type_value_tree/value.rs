use std::fmt::Display;

use super::AddressType;

#[derive(Debug, Clone)]
pub enum Value<ADDR: AddressType> {
    Unit,
    Object,
    Bool(bool),
    Char(char),
    Int(i128),
    Uint(u128),
    Float(f64),
    Address(ADDR),
    String(Vec<u8>, StringFormat),
    Array,
    Enumeration,
}

impl<ADDR: AddressType> Display for Value<ADDR> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Unit => write!(f, "()"),
            Value::Object | Value::Enumeration => write!(f, "{{}}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Char(v) => write!(f, "{v}"),
            Value::Int(v) => write!(f, "{v}"),
            Value::Uint(v) => write!(f, "{v}"),
            Value::Float(v) if *v > 1000000000.0 => write!(f, "{v:e}"),
            Value::Float(v) if *v <  1.0 / 1000000000.0 => write!(f, "{v:e}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Address(v) => write!(f, "{v:#X}"),
            Value::String(bytes, StringFormat::Ascii | StringFormat::Utf8) => {
                write!(
                    f,
                    "{}",
                    std::str::from_utf8(bytes)
                        .map(|str| format!("\"{str}\""))
                        .unwrap_or_else(|e| format!(
                            "\"{}\" (rest is corrupted: {:X?})",
                            std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap(),
                            &bytes[e.valid_up_to()..]
                        ))
                )
            }
            Value::Array => write!(f, "[]"),
        }
    }
}

impl<ADDR: AddressType> PartialEq for Value<ADDR> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(l0), Self::Bool(r0)) => l0 == r0,
            (Self::Char(l0), Self::Char(r0)) => l0 == r0,
            (Self::Int(l0), Self::Int(r0)) => l0 == r0,
            (Self::Int(l0), Self::Uint(r0)) if *l0 >= 0 => *l0 as u128 == *r0,
            (Self::Uint(l0), Self::Uint(r0)) => l0 == r0,
            (Self::Uint(l0), Self::Int(r0)) if *r0 >= 0 => *r0 as u128 == *l0,
            (Self::Float(l0), Self::Float(r0)) => l0 == r0,
            (Self::Address(l0), Self::Address(r0)) => l0 == r0,
            (Self::String(l0, l1), Self::String(r0, r1)) => l0 == r0 && l1 == r1,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringFormat {
    Ascii,
    Utf8,
}
