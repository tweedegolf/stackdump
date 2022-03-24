use crate::VariableType;

use self::value::Value;

pub mod value;
pub mod variable_type;

pub struct TypeValue<ADDR> {
    pub variable_type: VariableType,
    pub variable_value: Option<Value<ADDR>>
}
