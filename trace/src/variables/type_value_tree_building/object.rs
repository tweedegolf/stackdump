use crate::{
    error::TraceError,
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{variable_type::Archetype, TypeValue, TypeValueTree},
    variables::{
        build_type_value_tree, get_entry_name, get_entry_type_reference_tree,
        read_data_member_location,
    },
    DefaultReader,
};
use gimli::{Abbreviations, DebugInfoOffset, DwTag, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_object<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
    tag: DwTag,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    // We have an object with members
    // The informations we can gather is:
    // - type name
    // - type parameters
    // - the members of the object
    // - the byte size of the object

    let type_name = get_entry_name(dwarf, unit, entry)?;
    let byte_size = entry
        .required_attr(unit, gimli::constants::DW_AT_byte_size)?
        .required_udata_value()?;

    // Check if this is a type that wraps another type
    let is_member_pointer = entry
        .attr(gimli::constants::DW_AT_containing_type)?
        .is_some();

    let archetype = match (tag, is_member_pointer) {
        (_, true) => Archetype::ObjectMemberPointer,
        (gimli::constants::DW_TAG_structure_type, _) => Archetype::Structure,
        (gimli::constants::DW_TAG_union_type, _) => Archetype::Union,
        (gimli::constants::DW_TAG_class_type, _) => Archetype::Class,
        _ => unreachable!(),
    };

    type_value.data_mut().variable_type.name = type_name.clone();
    type_value.data_mut().variable_type.archetype = archetype;
    type_value.data_mut().bit_range = 0..byte_size * 8;

    // The members of the object can be found by looking at the children of the node
    let mut children = node.children();
    while let Ok(Some(child)) = children.next() {
        let member_entry = child.entry();

        // We can be a normal object, but we can also still be a tagged union.
        // We know that we're a tagged union if one of the members has the `DW_TAG_variant_part` tag, so we'll check for that.
        // If this object is a tagged union, then we will assume it isn't also a a normal object even though
        // that could be the case with how the DWARF spec states things. This isn't something Rust and I think even C++ can do.

        if member_entry.tag() == gimli::constants::DW_TAG_variant_part {
            // This is a tagged union, so ignore everything and build the type value tree from this child
            let mut tagged_union =
                build_type_value_tree(dwarf, unit, abbreviations, child, type_cache);

            if let Ok(tagged_union) = tagged_union.as_mut() {
                // The tagged union child doesn't have a name or byte size, so we need to give it the name of the object we
                // we thought we would get
                tagged_union.root_mut().data_mut().variable_type.name = type_name;
                tagged_union.root_mut().data_mut().bit_range = 0..byte_size * 8;
            }

            return tagged_union;
        }

        // This is an object and not a tagged union

        // Object children can be a couple of things:
        // - Member fields
        // - Sub programs (methods)
        // - Other objects (TODO what does this mean?)

        // Member fields have a name, a type and a location offset (relative to the base of the object).
        // Type parameters only have a name and a type.
        // The rest of the children are ignored.

        let member_name = match get_entry_name(dwarf, unit, member_entry) {
            Ok(member_name) => member_name,
            Err(_) => continue, // Only care about named members for now
        };

        match member_entry.tag() {
            gimli::constants::DW_TAG_member => {
                let member_location_offset_bits = read_data_member_location(unit, member_entry)?;

                let mut member_tree =
                    get_entry_type_reference_tree(unit, abbreviations, member_entry).map(
                        |mut type_tree| {
                            type_tree.root().map(|root| {
                                build_type_value_tree(dwarf, unit, abbreviations, root, type_cache)
                            })
                        },
                    )???;

                member_tree.root_mut().data_mut().name = member_name;
                member_tree.root_mut().data_mut().bit_range.end += member_location_offset_bits;
                member_tree.root_mut().data_mut().bit_range.start += member_location_offset_bits;

                type_value.push_back(member_tree);
            }
            gimli::constants::DW_TAG_template_type_parameter => {} // Ignore
            gimli::constants::DW_TAG_subprogram => {}              // Ignore
            gimli::constants::DW_TAG_structure_type
            | gimli::constants::DW_TAG_union_type
            | gimli::constants::DW_TAG_class_type => {} // Ignore
            member_tag => {
                return Err(TraceError::UnexpectedMemberTag {
                    object_name: type_name,
                    member_name,
                    member_tag,
                })
            }
        }
    }

    Ok(type_value_tree)
}
