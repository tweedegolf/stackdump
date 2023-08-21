use crate::{
    error::TraceError, get_entry_type_reference_tree_recursive, type_value_tree::TypeValueTree,
    variables::build_type_value_tree, DefaultReader,
};
use gimli::{Abbreviations, DebugInfoOffset, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_const_type<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    // Const is expressed as a type of its own, but that's BS.
    // So we're just gonna take the underlying type tree and use that as the real type which we then mark as const.

    // Get the entry
    let entry = node.entry();

    // Get the underlying type tree
    get_entry_type_reference_tree_recursive!(
        underlying_type_tree = (dwarf, unit, abbreviations, entry)
    );

    // Build a normal type value tree from the underlying tree
    let mut type_value_tree = underlying_type_tree.map(|mut type_tree| {
        type_tree
            .root()
            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root, type_cache))
    })???;

    // Mark it as const
    type_value_tree
        .root_mut()
        .data_mut()
        .variable_type
        .const_type = true;

    // Return the type value tree we built from the underlying tree
    Ok(type_value_tree)
}
