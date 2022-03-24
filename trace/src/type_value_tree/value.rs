pub enum Value<ADDR> {
    Bool(bool),
    Char(char),
    Int(i128),
    Uint(u128),
    Float(f64),
    Address(ADDR),
    /// An array of values. All values in the array must have the same type.
    Array(Vec<Value<ADDR>>),
}