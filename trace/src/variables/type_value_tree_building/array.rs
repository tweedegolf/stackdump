use crate::{
    error::TraceError,
    get_entry_type_reference_tree_recursive,
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{variable_type::Archetype, TypeValue, TypeValueTree},
    variables::{build_type_value_tree, get_entry_type_reference_tree},
    DefaultReader,
};
use gimli::{Abbreviations, DebugInfoOffset, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_array<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    let entry_tag = entry.tag().to_string();

    // Arrays are their own thing in DWARF.
    // They have no name.
    // What can be found on the entry are the type of the elements of the array and the byte size.
    // Arrays have one child entry that contains information about the indexing of the array.

    get_entry_type_reference_tree_recursive!(
        base_element_type_tree = (dwarf, unit, abbreviations, entry)
    );

    let mut base_element_type_tree = base_element_type_tree.map(|mut type_tree| {
        type_tree
            .root()
            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root, type_cache))
    })???;

    base_element_type_tree.root_mut().data_mut().name = "base".into();

    let byte_size = entry
        .attr(gimli::constants::DW_AT_byte_size)?
        .and_then(|bsize| bsize.udata_value());
    let element_bitsize = base_element_type_tree.data().bit_length();

    let mut children = node.children();
    let child = children
        .next()?
        .ok_or(TraceError::ExpectedChildNotPresent { entry_tag })?;
    let child_entry = child.entry();

    let lower_bound = child_entry
        .required_attr(&unit.header, gimli::constants::DW_AT_lower_bound)?
        .sdata_value()
        .unwrap_or(0);

    // There's either a count or an upper bound
    let count = match (
        child_entry
            .required_attr(&unit.header, gimli::constants::DW_AT_count)
            .and_then(|c| c.required_udata_value()),
        child_entry
            .required_attr(&unit.header, gimli::constants::DW_AT_upper_bound)
            .and_then(|c| c.required_sdata_value()),
    ) {
        // We've got a count, so let's use that
        (Ok(count), _) => Ok(count),
        // We've got an upper bound, so let's calculate the count from that
        (_, Ok(upper_bound)) => Ok((upper_bound - lower_bound).try_into().unwrap()),
        // Both are not readable
        (Err(e), Err(_)) => Err(e),
    }?;

    type_value.data_mut().bit_range.end = type_value.data_mut().bit_range.start
        + byte_size
            .map(|byte_size| byte_size * 8)
            .unwrap_or_else(|| element_bitsize * count);
    type_value.data_mut().variable_type.name = format!(
        "[{};{}]",
        base_element_type_tree.data().variable_type.name,
        count
    );
    type_value.data_mut().variable_type.archetype = Archetype::Array;

    for data_index in lower_bound..(lower_bound + count as i64) {
        let mut element_type_tree = base_element_type_tree.clone();

        element_type_tree.root_mut().data_mut().name = data_index.to_string();
        element_type_tree.root_mut().data_mut().bit_range.start +=
            data_index as u64 * element_bitsize;
        element_type_tree.root_mut().data_mut().bit_range.end +=
            data_index as u64 * element_bitsize;

        type_value.push_back(element_type_tree);
    }

    Ok(type_value_tree)
}
