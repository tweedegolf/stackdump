use crate::{
    error::TraceError,
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{
        value::Value,
        variable_type::{Archetype, VariableType},
        TypeValue, TypeValueTree, VariableDataError,
    },
    variables::{build_type_value_tree, get_entry_type_reference_tree, read_data_member_location},
    DefaultReader,
};
use gimli::{Abbreviations, AttributeValue, DebugInfoOffset, Dwarf, Unit};
use std::collections::HashMap;

pub fn build_tagged_union<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    // We can't read the name and byte size, but we'll get that assigned when we return, so don't do that here
    // Read the DW_AT_discr. It will have a reference to the DIE that we can read to know which variant is active.
    // This will probably be an integer.
    type_value.data_mut().variable_type.archetype = Archetype::TaggedUnion;
    type_value.data_mut().variable_value = Ok(Value::Object);

    let discriminant_attr = entry.required_attr(unit, gimli::constants::DW_AT_discr)?;

    let discriminant_unit_offset =
        if let AttributeValue::UnitRef(offset) = discriminant_attr.value() {
            Ok(offset)
        } else {
            Err(TraceError::WrongAttributeValueType {
                attribute_name: discriminant_attr.name().to_string(),
                value_type_name: "UnitRef",
            })
        }?;

    let discriminant_entry = unit.entry(discriminant_unit_offset)?;

    // We've got some data about the discriminant, let's make it our first type value child

    let mut discriminant_tree =
        get_entry_type_reference_tree(unit, abbreviations, &discriminant_entry).map(
            |mut type_tree| {
                type_tree
                    .root()
                    .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root, type_cache))
            },
        )???;
    discriminant_tree.root_mut().data_mut().name = "discriminant".into();

    // The discriminant has its own member location, so we need to offset the bit range
    let discriminant_location_offset_bits = read_data_member_location(unit, &discriminant_entry)?;
    discriminant_tree.root_mut().data_mut().bit_range.start += discriminant_location_offset_bits;
    discriminant_tree.root_mut().data_mut().bit_range.end += discriminant_location_offset_bits;

    type_value_tree.push_back(discriminant_tree);

    // Now we need to read all of the variant parts which are the children of the entry.

    let mut children = node.children();
    while let Ok(Some(child)) = children.next() {
        let variant_entry = child.entry();

        // We'll find more nodes there as well like types and members. We can ignore those because if they are
        // relevant, we'll find them indirectly. For example, the member we'll likely find is the discriminant and
        // the types we'll find are the types that are defined for each variant.

        if variant_entry.tag() != gimli::constants::DW_TAG_variant {
            continue;
        }

        // We've found a variant part!
        // Three things can happen:
        // 1. It has a DW_AT_discr_value
        // 2. It has a DW_AT_discr_list
        // 3. It has nothing
        //
        // The first gives the value the discriminant has to have for this variant to be active.
        // The second one has a list of values, but I haven't seen that that being generated so far. We'll check
        // and give an error in that case.
        // A variant with nothing is the default case. If no other variant matches, then this one is selected.

        let discr_value = variant_entry.attr(gimli::constants::DW_AT_discr_value)?;
        let discr_list = variant_entry.attr(gimli::constants::DW_AT_discr_list)?;

        let discriminator_value = match (discr_value, discr_list) {
            (Some(discr_value), _) => Some(discr_value.required_sdata_value()?),
            (_, Some(_)) => {
                return Err(TraceError::OperationNotImplemented {
                    operation: "Reading the discr_list".into(),
                    file: file!(),
                    line: line!(),
                })
            }
            (None, None) => None,
        };

        // We know the value, so we can create a type value tree for the variant part

        let mut variant_tree = TypeValueTree::new(TypeValue {
            name: "variant".into(),
            variable_type: VariableType {
                name: "".into(),
                archetype: Archetype::TaggedUnionVariant,
            },
            bit_range: 0..0,
            variable_value: discriminator_value
                .map(|v| Value::Int(v as _))
                .ok_or(VariableDataError::NoDataAvailable),
        });

        // Variant parts have one child that is their actual value

        let mut variant_children = child.children();
        let variant_member =
            variant_children
                .next()?
                .ok_or(TraceError::ExpectedChildNotPresent {
                    entry_tag: "DW_TAG_variant_part".into(),
                })?;

        let variant_member_bit_offset = read_data_member_location(unit, variant_member.entry())?;

        let variant_member_tree =
            get_entry_type_reference_tree(unit, abbreviations, variant_member.entry()).map(
                |mut type_tree| {
                    type_tree.root().map(|root| {
                        build_type_value_tree(dwarf, unit, abbreviations, root, type_cache)
                    })
                },
            )???;

        variant_tree.root_mut().data_mut().bit_range = variant_member_bit_offset
            ..variant_member_bit_offset + variant_member_tree.root().data().bit_length();

        variant_tree.push_back(variant_member_tree);
        type_value_tree.push_back(variant_tree);
    }

    Ok(type_value_tree)
}
