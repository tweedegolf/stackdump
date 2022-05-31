use gimli::DwAte;

#[derive(Debug, Clone, Default)]
pub struct VariableType {
    pub name: String,
    pub archetype: Archetype,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Archetype {
    Structure,
    Union,
    Class,
    /// An object (like a Structure) that is a pointer to another type's members.
    /// For example: the vtable of an object's Debug impl.
    ObjectMemberPointer,
    BaseType(DwAte),
    Pointer,
    Array,
    TaggedUnion,
    TaggedUnionVariant,
    Enumeration,
    Enumerator,
    Subroutine,
    Unknown,
}

impl Default for Archetype {
    fn default() -> Self {
        Self::Unknown
    }
}
