use crate::{
    error::TraceError,
    get_entry_type_reference_tree_recursive,
    type_value_tree::{variable_type::Archetype, TypeValue, TypeValueTree},
    variables::{build_type_value_tree, get_entry_name, get_entry_type_reference_tree},
    DefaultReader,
};
use gimli::{Abbreviations, DebugInfoOffset, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_typedef<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    // A typedef is basically a named type alias.
    // We record the name and have the real value as the child of this one

    let name = get_entry_name(dwarf, unit, entry)?;

    get_entry_type_reference_tree_recursive!(
        underlying_type_tree = (dwarf, unit, abbreviations, entry)
    );

    let mut underlying_type_tree = underlying_type_tree.map(|mut type_tree| {
        type_tree
            .root()
            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root, type_cache))
    })???;
    underlying_type_tree.root_mut().data_mut().name = "base".into();
    let underlying_type_bitrange = underlying_type_tree.root().data().bit_range.clone();

    type_value.push_back(underlying_type_tree);

    type_value.data_mut().variable_type.name = name;
    type_value.data_mut().variable_type.archetype = Archetype::Typedef;
    type_value.data_mut().bit_range = underlying_type_bitrange;

    Ok(type_value_tree)
}
