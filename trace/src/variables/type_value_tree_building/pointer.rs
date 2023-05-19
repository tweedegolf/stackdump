use crate::{
    error::TraceError,
    get_entry_type_reference_tree_recursive,
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{variable_type::Archetype, TypeValue, TypeValueTree},
    variables::{build_type_value_tree, get_entry_name},
    DefaultReader,
};
use gimli::{Abbreviations, DebugInfoOffset, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_pointer<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    let entry_die_offset = entry.offset().to_debug_info_offset(&unit.header).unwrap();

    // A pointer in this context is just a number.
    // It has a name and a type that indicates the type of the object it points to.

    let (pointee_type_name, pointee_type_die_offset) = {
        get_entry_type_reference_tree_recursive!(
            pointee_type_tree = (dwarf, unit, abbreviations, entry)
        );

        pointee_type_tree.map(|mut type_tree| {
            type_tree.root().map(|root| {
                let die_offset = root
                    .entry()
                    .offset()
                    .to_debug_info_offset(&unit.header)
                    .unwrap();

                let pointee_type_name = get_entry_name(dwarf, unit, root.entry());

                pointee_type_name.map(|ptn| (ptn, die_offset))
            })
        })???
    };

    // Some pointers don't have names, but generally it is just `&<typename>`
    // So if only the name is missing, we can recover

    let name =
        get_entry_name(dwarf, unit, entry).unwrap_or_else(|_| format!("&{pointee_type_name}"));

    // The debug info also contains an address class that can describe what kind of pointer it is.
    // We only support `DW_ADDR_none` for now, which means that there's no special specification.
    // We do perform the check though to be sure.

    let address_class = entry
        .required_attr(&unit.header, gimli::constants::DW_AT_address_class)?
        .required_address_class()?;
    if address_class != gimli::constants::DW_ADDR_none {
        return Err(TraceError::UnexpectedPointerClass {
            pointer_name: name,
            class_value: address_class,
        });
    }

    type_value.data_mut().variable_type.name = name;
    type_value.data_mut().variable_type.archetype = Archetype::Pointer(pointee_type_die_offset);
    type_value.data_mut().bit_range = 0..W::BITS as u64;

    // Insert this pointer into the type cache
    type_cache.insert(entry_die_offset, Ok(type_value_tree.clone()));

    // Insert the pointee into the type cache
    #[allow(clippy::map_entry)] // Can't use the entry api because of the type_cache borrow later
    if !type_cache.contains_key(&pointee_type_die_offset) {
        get_entry_type_reference_tree_recursive!(
            pointee_type_tree = (dwarf, unit, abbreviations, entry)
        );

        let pointee_type_tree = pointee_type_tree.map(|mut type_tree| {
            type_tree
                .root()
                .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root, type_cache))
        })???;
        type_cache.insert(pointee_type_die_offset, Ok(pointee_type_tree));
    }

    Ok(type_value_tree)
}
