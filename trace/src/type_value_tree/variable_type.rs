use gimli::{DebugInfoOffset, DwAte};

#[derive(Debug, Clone, Default)]
pub struct VariableType {
    pub name: String,
    pub archetype: Archetype,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Archetype {
    Structure,
    Union,
    Class,
    /// An object (like a Structure) that is a pointer to another type's members.
    /// For example: the vtable of an object's Debug impl.
    ObjectMemberPointer,
    BaseType(DwAte),
    Typedef,
    /// A pointer that points at an object.
    ///
    /// The type is not directly encoded in the tree because linked lists exists.
    /// We need to catch that to avoid recursions of linked lists.
    Pointer(DebugInfoOffset),
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
