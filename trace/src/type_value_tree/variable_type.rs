use gimli::DwAte;

#[derive(Debug, Clone, Default)]
pub struct VariableType {
    pub name: String,
    pub archetype: Archetype,
}

#[derive(Debug, Clone)]
pub enum Archetype {
    Structure,
    Union,
    Class,
    BaseType(DwAte),
    Pointer,
    Array,
    Enumeration,
    Enumerator,
    TypeParameter,
    Unknown,
}

impl Default for Archetype {
    fn default() -> Self {
        Self::Unknown
    }
}