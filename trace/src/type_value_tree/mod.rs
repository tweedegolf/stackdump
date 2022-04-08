use std::ops::Range;
use self::{value::Value, variable_type::VariableType};

pub mod value;
pub mod variable_type;

pub type TypeValueTree<ADDR> = trees::Tree<TypeValue<ADDR>>;

#[derive(Debug, Clone)]
pub struct TypeValue<ADDR> {
    pub name: String,
    pub variable_type: VariableType,
    pub bit_range: Range<u64>,
    pub variable_value: Option<Value<ADDR>>,
}
