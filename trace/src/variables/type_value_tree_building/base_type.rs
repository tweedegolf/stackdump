use crate::{
    error::TraceError,
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{variable_type::Archetype, TypeValue, TypeValueTree},
    variables::get_entry_name,
    DefaultReader,
};
use gimli::{AttributeValue, Dwarf, Unit};

pub fn build_base_type<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    node: gimli::EntriesTreeNode<DefaultReader>,
) -> Result<TypeValueTree<W>, TraceError> {
    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();
    let entry = node.entry();

    // A base type is a primitive and there are many of them.
    // Which base type this is, is recorded in the `DW_AT_encoding` attribute.
    // The value of that attribute is a `DwAte`.

    // We record the name, the encoding and the size of the primitive

    let name = get_entry_name(dwarf, unit, entry)?;
    let encoding = entry
        .required_attr(&unit.header, gimli::constants::DW_AT_encoding)
        .map(|attr| {
            if let AttributeValue::Encoding(encoding) = attr.value() {
                Ok(encoding)
            } else {
                Err(TraceError::WrongAttributeValueType {
                    attribute_name: attr.name().to_string(),
                    expected_type_name: "Encoding",
                    gotten_value: format!("{:X?}", attr.value()),
                })
            }
        })??;
    let byte_size = entry
        .required_attr(&unit.header, gimli::constants::DW_AT_byte_size)?
        .required_udata_value()?;

    type_value.data_mut().variable_type.name = name;
    type_value.data_mut().variable_type.archetype = Archetype::BaseType(encoding);
    type_value.data_mut().bit_range = 0..byte_size * 8;

    Ok(type_value_tree)
}
