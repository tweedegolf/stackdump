use crate::{
    error::TraceError,
    get_entry_type_reference_tree_recursive,
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{
        value::Value,
        variable_type::{Archetype, VariableType},
        TypeValue, TypeValueTree,
    },
    variables::{build_type_value_tree, get_entry_name},
    DefaultReader,
};
use gimli::{Abbreviations, DebugInfoOffset, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_enumeration<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    // This is an enum type (like a C-style enum).
    // Enums have a name and also an underlying_type (which is usually an integer).
    // The entry also has a child `DW_TAG_enumerator` for each variant.

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

    type_value.data_mut().variable_type.name = name;
    type_value.data_mut().variable_type.archetype = Archetype::Enumeration;
    type_value.data_mut().bit_range = underlying_type_bitrange.clone();

    type_value.push_back(underlying_type_tree);

    let mut children = node.children();
    while let Ok(Some(child)) = children.next() {
        let enumerator_entry = child.entry();

        // Each child is a DW_TAG_enumerator or DW_TAG_subprogram
        if enumerator_entry.tag() != gimli::constants::DW_TAG_enumerator {
            continue;
        }

        // Each variant has a name and an integer value.
        // If the enum has that value, then the enum is of that variant.
        // This does of course not work for flag enums.

        let enumerator_name = get_entry_name(dwarf, unit, enumerator_entry)?;
        let const_value = enumerator_entry
            .required_attr(&unit.header, gimli::constants::DW_AT_const_value)?
            .required_sdata_value()?;

        type_value.push_back(TypeValueTree::new(TypeValue {
            name: enumerator_name,
            variable_type: VariableType {
                archetype: Archetype::Enumerator,
                ..Default::default()
            },
            bit_range: underlying_type_bitrange.clone(),
            variable_value: Ok(Value::Int(const_value as i128)),
        }));
    }

    Ok(type_value_tree)
}
