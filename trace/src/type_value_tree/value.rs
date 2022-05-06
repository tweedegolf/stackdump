#[derive(Debug, Clone)]
pub enum Value<ADDR> {
    Unit,
    Bool(bool),
    Char(char),
    Int(i128),
    Uint(u128),
    Float(f64),
    Address(ADDR),
    Array
}
