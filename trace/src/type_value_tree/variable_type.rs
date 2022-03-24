pub struct VariableType {
    pub name: String,
    pub byte_size: Option<u64>,
    pub archetype: Archetype,
}

pub enum Archetype {
    Structure,
    Union,
    Class,
    BaseType,
    Pointer,
    Array,
    Enumeration,
}