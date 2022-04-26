use self::{value::Value, variable_type::VariableType};
use std::ops::Range;

pub mod value;
pub mod variable_type;

pub type TypeValueTree<ADDR> = trees::Tree<TypeValue<ADDR>>;

#[derive(Debug, Clone, Default)]
pub struct TypeValue<ADDR> {
    pub name: String,
    pub variable_type: VariableType,
    pub bit_range: Range<u64>,
    pub variable_value: Option<Value<ADDR>>,
}
