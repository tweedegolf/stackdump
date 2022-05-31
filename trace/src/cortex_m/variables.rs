//! A module containing functions for finding and reading the variables of frames.
//!
//! These are (almost) all pure functions that get all of their context through the parameters.
//! This was decided because the datastructures that are involved are pretty complex and I didn't
//! want to add complexity.
//! All functions can be reasoned with on the function level.
//!

use super::TraceError;
use crate::{
    gimli_extensions::{AttributeExt, DebuggingInformationEntryExt},
    type_value_tree::{
        value::{StringFormat, Value},
        variable_type::{Archetype, VariableType},
        TypeValue, TypeValueNode, TypeValueTree, VariableDataError,
    },
    DefaultReader, Location, Variable, VariableKind, VariableLocationResult,
};
use bitvec::prelude::*;
use gimli::{
    Abbreviations, Attribute, AttributeValue, DebuggingInformationEntry, Dwarf, EntriesTree,
    EvaluationResult, Piece, Reader, Unit,
};
use stackdump_core::device_memory::DeviceMemory;
use std::pin::Pin;

fn div_ceil(lhs: u64, rhs: u64) -> u64 {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}
/// Gets the string value from the `DW_AT_name` attribute of the given entry
fn get_entry_name(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<String, TraceError> {
    // Find the attribute
    let name_attr = entry.required_attr(unit, gimli::constants::DW_AT_name)?;
    // Read as a string type
    let attr_string = dwarf.attr_string(unit, name_attr.value())?;
    // Convert to String
    Ok(attr_string.to_string()?.into())
}

/// If available, get the EntriesTree of the `DW_AT_abstract_origin` attribute of the given entry
fn get_entry_abstract_origin_reference_tree<'abbrev, 'unit>(
    unit: &'unit Unit<DefaultReader, usize>,
    abbreviations: &'abbrev Abbreviations,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<Option<EntriesTree<'abbrev, 'unit, DefaultReader>>, TraceError> {
    // Find the attribute
    let abstract_origin_attr = entry.attr(gimli::constants::DW_AT_abstract_origin)?;

    let abstract_origin_attr = match abstract_origin_attr {
        Some(abstract_origin_attr) => abstract_origin_attr,
        None => return Ok(None),
    };

    // Check its offset
    let type_offset = if let AttributeValue::UnitRef(offset) = abstract_origin_attr.value() {
        Ok(offset)
    } else {
        Err(TraceError::WrongAttributeValueType {
            attribute_name: abstract_origin_attr.name().to_string(),
            value_type_name: "UnitRef",
        })
    }?;

    // Get the entries for the type
    Ok(Some(
        unit.header.entries_tree(abbreviations, Some(type_offset))?,
    ))
}

/// Get the EntriesTree of the `DW_AT_type` attribute of the given entry
fn get_entry_type_reference_tree<'abbrev, 'unit>(
    unit: &'unit Unit<DefaultReader, usize>,
    abbreviations: &'abbrev Abbreviations,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<EntriesTree<'abbrev, 'unit, DefaultReader>, TraceError> {
    // Find the attribute
    let type_attr = entry.required_attr(unit, gimli::constants::DW_AT_type)?;

    // Check its offset
    let type_offset = if let AttributeValue::UnitRef(offset) = type_attr.value() {
        Ok(offset)
    } else {
        Err(TraceError::WrongAttributeValueType {
            attribute_name: type_attr.name().to_string(),
            value_type_name: "UnitRef",
        })
    }?;

    // Get the entries for the type
    Ok(unit.header.entries_tree(abbreviations, Some(type_offset))?)
}

fn try_read_frame_base(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    device_memory: &DeviceMemory<u32>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<Option<u32>, TraceError> {
    let frame_base_location = evaluate_location(
        dwarf,
        unit,
        device_memory,
        entry.attr(gimli::constants::DW_AT_frame_base)?,
        None,
    )?;
    let frame_base_data = get_variable_data(
        device_memory,
        4 * 8, // Frame base is 4 bytes on cortex-m (TODO crossplatform)
        frame_base_location,
    );

    Ok(frame_base_data.ok().map(|data| data.load_le()))
}

/// Finds the [Location] of the given entry.
///
/// This is done based on the `DW_AT_decl_file`, `DW_AT_decl_line` and `DW_AT_decl_column` attributes.
/// These are normally present on variables and functions.
fn find_entry_location<'unit>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &'unit Unit<DefaultReader, usize>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<Location, TraceError> {
    // Get the attributes
    let variable_decl_file = entry
        .attr_value(gimli::constants::DW_AT_decl_file)?
        .and_then(|f| match f {
            AttributeValue::FileIndex(index) => Some(index),
            _ => None,
        });
    let variable_decl_line = entry
        .attr_value(gimli::constants::DW_AT_decl_line)?
        .and_then(|l| l.udata_value());
    let variable_decl_column = entry
        .attr_value(gimli::constants::DW_AT_decl_column)?
        .and_then(|c| c.udata_value());

    fn path_push(path: &mut String, p: &str) {
        /// Check if the path in the given string has a unix style root
        fn has_unix_root(p: &str) -> bool {
            p.starts_with('/')
        }

        /// Check if the path in the given string has a windows style root
        fn has_windows_root(p: &str) -> bool {
            p.starts_with('\\') || p.get(1..3) == Some(":\\")
        }

        if has_unix_root(p) || has_windows_root(p) {
            *path = p.to_string();
        } else {
            let dir_separator = if has_windows_root(path.as_str()) {
                '\\'
            } else {
                '/'
            };

            if !path.ends_with(dir_separator) {
                path.push(dir_separator);
            }
            *path += p;
        }
    }

    // The file is given as a number, so we need to search for the real file path
    let variable_file = if let (Some(variable_decl_file), Some(line_program)) =
        (variable_decl_file, unit.line_program.as_ref())
    {
        // The file paths are stored in the line_program
        if let Some(file_entry) = line_program.header().file(variable_decl_file) {
            let mut path = if let Some(comp_dir) = &unit.comp_dir {
                comp_dir.to_string_lossy()?.into_owned()
            } else {
                String::new()
            };

            // The directory index 0 is defined to correspond to the compilation unit directory
            if variable_decl_file != 0 {
                if let Some(directory) = file_entry.directory(line_program.header()) {
                    path_push(
                        &mut path,
                        &dwarf.attr_string(unit, directory)?.to_string_lossy()?,
                    )
                }
            }

            path_push(
                &mut path,
                &dwarf
                    .attr_string(unit, file_entry.path_name())?
                    .to_string()?,
            );

            Some(path)
        } else {
            None
        }
    } else {
        None
    };

    Ok(Location {
        file: variable_file,
        line: variable_decl_line,
        column: variable_decl_column,
    })
}

/// Reads the DW_AT_data_member_location and returns the entry's bit offset
fn read_data_member_location(
    unit: &Unit<DefaultReader, usize>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<u64, TraceError> {
    // TODO: Sometimes this is not a simple number, but a location expression.
    // As of writing this has not come up, but I can imagine this is the case for C bitfields.
    // It is the offset in bits from the base.
    Ok(entry
        .required_attr(unit, gimli::constants::DW_AT_data_member_location)?
        .required_udata_value()?
        * 8)
}

/// Decodes the type of an entry into a type value tree, however, the value is not yet filled in.
///
/// The given node should come from the [get_entry_type_reference_tree]
/// and [get_entry_abstract_origin_reference_tree] functions.
fn build_type_value_tree(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
) -> Result<TypeValueTree<u32>, TraceError> {
    // Get the root entry and its tag
    let entry = node.entry();
    let entry_tag = entry.tag().to_string();

    let mut type_value_tree = TypeValueTree::new(TypeValue::default());
    let mut type_value = type_value_tree.root_mut();

    // The tag tells us what the base type it
    match entry.tag() {
        gimli::constants::DW_TAG_variant_part => {
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
                            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root))
                    },
                )???;
            discriminant_tree.root_mut().data_mut().name = "discriminant".into();

            // The discriminant has its own member location, so we need to offset the bit range
            let discriminant_location_offset_bits =
                read_data_member_location(unit, &discriminant_entry)?;
            discriminant_tree.root_mut().data_mut().bit_range.start +=
                discriminant_location_offset_bits;
            discriminant_tree.root_mut().data_mut().bit_range.end +=
                discriminant_location_offset_bits;

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

                let variant_member_bit_offset =
                    read_data_member_location(unit, variant_member.entry())?;

                let variant_member_tree =
                    get_entry_type_reference_tree(unit, abbreviations, variant_member.entry())
                        .map(|mut type_tree| {
                        type_tree
                            .root()
                            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root))
                    })???;

                variant_tree.root_mut().data_mut().bit_range = variant_member_bit_offset
                    ..variant_member_bit_offset + variant_member_tree.root().data().bit_length();

                variant_tree.push_back(variant_member_tree);
                type_value_tree.push_back(variant_tree);
            }

            Ok(type_value_tree)
        }
        tag @ gimli::constants::DW_TAG_structure_type
        | tag @ gimli::constants::DW_TAG_union_type
        | tag @ gimli::constants::DW_TAG_class_type => {
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
                    let mut tagged_union = build_type_value_tree(dwarf, unit, abbreviations, child);

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
                        let member_location_offset_bits =
                            read_data_member_location(unit, member_entry)?;

                        let mut member_tree =
                            get_entry_type_reference_tree(unit, abbreviations, member_entry)
                                .map(|mut type_tree| {
                                type_tree.root().map(|root| {
                                    build_type_value_tree(dwarf, unit, abbreviations, root)
                                })
                            })???;

                        member_tree.root_mut().data_mut().name = member_name;
                        member_tree.root_mut().data_mut().bit_range.end +=
                            member_location_offset_bits;
                        member_tree.root_mut().data_mut().bit_range.start +=
                            member_location_offset_bits;

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
        gimli::constants::DW_TAG_base_type => {
            // A base type is a primitive and there are many of them.
            // Which base type this is, is recorded in the `DW_AT_encoding` attribute.
            // The value of that attribute is a `DwAte`.

            // We record the name, the encoding and the size of the primitive

            let name = get_entry_name(dwarf, unit, entry)?;
            let encoding = entry
                .required_attr(unit, gimli::constants::DW_AT_encoding)
                .map(|attr| {
                    if let AttributeValue::Encoding(encoding) = attr.value() {
                        Ok(encoding)
                    } else {
                        Err(TraceError::WrongAttributeValueType {
                            attribute_name: attr.name().to_string(),
                            value_type_name: "Encoding",
                        })
                    }
                })??;
            let byte_size = entry
                .required_attr(unit, gimli::constants::DW_AT_byte_size)?
                .required_udata_value()?;

            type_value.data_mut().variable_type.name = name;
            type_value.data_mut().variable_type.archetype = Archetype::BaseType(encoding);
            type_value.data_mut().bit_range = 0..byte_size * 8;

            Ok(type_value_tree)
        }
        gimli::constants::DW_TAG_pointer_type => {
            // A pointer in this context is just a number.
            // It has a name and a type that indicates the type of the object it points to.

            let mut pointee_type_tree = get_entry_type_reference_tree(unit, abbreviations, entry)
                .map(|mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root))
                })???;

            pointee_type_tree.root_mut().data_mut().name = "pointee".into();

            // Some pointers don't have names, but generally it is just `&<typename>`
            // So if only the name is missing, we can recover

            let name = get_entry_name(dwarf, unit, entry)
                .unwrap_or_else(|_| format!("&{}", pointee_type_tree.data().variable_type.name));

            // The debug info also contains an address class that can describe what kind of pointer it is.
            // We only support `DW_ADDR_none` for now, which means that there's no special specification.
            // We do perform the check though to be sure.

            let address_class = entry
                .required_attr(unit, gimli::constants::DW_AT_address_class)?
                .required_address_class()?;
            if address_class != gimli::constants::DW_ADDR_none {
                return Err(TraceError::UnexpectedPointerClass {
                    pointer_name: name,
                    class_value: address_class,
                });
            }

            type_value.data_mut().variable_type.name = name;
            type_value.data_mut().variable_type.archetype = Archetype::Pointer;
            type_value.data_mut().bit_range = 0..u32::BITS as u64; // TODO: Crossplatformness

            type_value.push_back(pointee_type_tree);

            Ok(type_value_tree)
        }
        gimli::constants::DW_TAG_array_type => {
            // Arrays are their own thing in DWARF.
            // They have no name.
            // What can be found on the entry are the type of the elements of the array and the byte size.
            // Arrays have one child entry that contains information about the indexing of the array.

            let mut base_element_type_tree =
                get_entry_type_reference_tree(unit, abbreviations, entry).map(
                    |mut type_tree| {
                        type_tree
                            .root()
                            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root))
                    },
                )???;

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
                .required_attr(unit, gimli::constants::DW_AT_lower_bound)?
                .sdata_value()
                .unwrap_or(0);

            // There's either a count or an upper bound
            let count = match (
                child_entry
                    .required_attr(unit, gimli::constants::DW_AT_count)
                    .and_then(|c| c.required_udata_value()),
                child_entry
                    .required_attr(unit, gimli::constants::DW_AT_upper_bound)
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
        gimli::constants::DW_TAG_enumeration_type => {
            // This is an enum type (like a C-style enum).
            // Enums have a name and also an underlying_type (which is usually an integer).
            // The entry also has a child `DW_TAG_enumerator` for each variant.

            let name = get_entry_name(dwarf, unit, entry)?;
            let mut underlying_type_tree =
                get_entry_type_reference_tree(unit, abbreviations, entry).map(
                    |mut type_tree| {
                        type_tree
                            .root()
                            .map(|root| build_type_value_tree(dwarf, unit, abbreviations, root))
                    },
                )???;
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
                    .required_attr(unit, gimli::constants::DW_AT_const_value)?
                    .required_sdata_value()?;

                type_value.push_back(TypeValueTree::new(TypeValue {
                    name: enumerator_name,
                    variable_type: VariableType {
                        name: "".into(),
                        archetype: Archetype::Enumerator,
                    },
                    bit_range: underlying_type_bitrange.clone(),
                    variable_value: Ok(Value::Int(const_value as i128)),
                }));
            }

            Ok(type_value_tree)
        }
        gimli::constants::DW_TAG_subroutine_type => {
            type_value.data_mut().variable_type.archetype = Archetype::Subroutine;
            Ok(type_value_tree)
        } // Ignore
        tag => Err(TraceError::TagNotImplemented {
            tag_name: tag.to_string(),
            entry_debug_info_offset: entry.offset().to_debug_info_offset(&unit.header).unwrap().0,
        })
        .unwrap(),
    }
}

/// Runs the location evaluation of gimli.
///
/// - `location`: The `DW_AT_location` attribute value of the entry of the variable we want to get the location of.
/// This may be a None if the variable has no location attribute.
fn evaluate_location(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    device_memory: &DeviceMemory<u32>,
    location: Option<Attribute<DefaultReader>>,
    frame_base: Option<u32>,
) -> Result<VariableLocationResult, TraceError> {
    // First, we need to have the actual value
    let location = match location {
        Some(location) => location.value(),
        None => return Ok(VariableLocationResult::NoLocationAttribute),
    };

    // Then we need to get the location expression. This expression can then later be evaluated by gimli.
    let location_expression = match location {
        AttributeValue::Block(ref data) => gimli::Expression(data.clone()),
        AttributeValue::Exprloc(ref data) => data.clone(),
        AttributeValue::LocationListsRef(l) => {
            let mut locations = dwarf.locations(unit, l)?;
            let mut location = None;

            while let Ok(Some(maybe_location)) = locations.next() {
                // The .debug_loc does not seem to count the thumb bit, so remove it
                let check_pc =
                    u64::from(device_memory.register(gimli::Arm::PC)? & !super::THUMB_BIT);

                if check_pc >= maybe_location.range.begin && check_pc < maybe_location.range.end {
                    location = Some(maybe_location);
                    break;
                }
            }

            if let Some(location) = location {
                location.data
            } else {
                return Ok(VariableLocationResult::LocationListNotFound);
            }
        }
        _ => unreachable!(),
    };

    // Turn the expression into an evaluation
    let mut location_evaluation = location_expression.evaluation(unit.encoding());

    // Now we need to evaluate everything.
    // DWARF has a stack based instruction set that needs to be executed.
    // Luckily, gimli already implements the bulk of it.
    // The evaluation stops when it requires some memory that we need to provide.
    let mut result = location_evaluation.evaluate()?;
    while result != EvaluationResult::Complete {
        log::trace!("Location evaluation result: {:?}", result);
        match result {
            EvaluationResult::RequiresRegister {
                register,
                base_type,
            } => {
                let value = device_memory.register(register)?;
                let value = match base_type.0 {
                    0 => gimli::Value::Generic(value.into()),
                    _ => todo!("Other types than generic haven't been implemented yet"),
                };
                result = location_evaluation.resume_with_register(value)?;
            }
            EvaluationResult::RequiresFrameBase if frame_base.is_some() => {
                result = location_evaluation.resume_with_frame_base(
                    frame_base.ok_or(TraceError::UnknownFrameBase)? as u64,
                )?;
            }
            EvaluationResult::RequiresRelocatedAddress(address) => {
                // We have no relocations of code
                result = location_evaluation.resume_with_relocated_address(address)?;
            }
            r => {
                return Ok(
                    VariableLocationResult::LocationEvaluationStepNotImplemented(std::rc::Rc::new(
                        r,
                    )),
                )
            }
        }
    }

    let result = location_evaluation.result();

    match result.len() {
        0 => Ok(VariableLocationResult::NoLocationFound),
        _ => Ok(VariableLocationResult::LocationsFound(result)),
    }
}

/// Reads the data of a piece of memory
///
/// The [Piece] is an indirect result of the [evaluate_location] function.
///
/// - `device_memory`: The captured memory of the device
/// - `piece`: The piece of memory location that tells us which data needs to be read
/// - `variable_size`: The size of the variable in bytes
fn get_piece_data(
    device_memory: &DeviceMemory<u32>,
    piece: &Piece<DefaultReader, usize>,
    variable_size: u64,
) -> Result<Option<bitvec::vec::BitVec<u8, Lsb0>>, VariableDataError> {
    let mut data = match piece.location.clone() {
        gimli::Location::Empty => return Err(VariableDataError::OptimizedAway),
        gimli::Location::Register { register } => Some(
            device_memory
                .register(register)
                .map(|r| r.to_ne_bytes().view_bits().to_bitvec())
                .map_err(|e| VariableDataError::NoDataAvailableAt(e.to_string()))?,
        ),
        gimli::Location::Address { address } => device_memory
            .read_slice(address..(address + variable_size))
            .map(|b| b.view_bits().to_bitvec()),
        gimli::Location::Value { value } => {
            let mut data = BitVec::new();

            match value {
                gimli::Value::Generic(v) => data.extend(v.view_bits::<Lsb0>()),
                gimli::Value::I8(v) => data.extend((v as u8).view_bits::<Lsb0>()),
                gimli::Value::U8(v) => data.extend(v.view_bits::<Lsb0>()),
                gimli::Value::I16(v) => data.extend((v as u16).view_bits::<Lsb0>()),
                gimli::Value::U16(v) => data.extend(v.view_bits::<Lsb0>()),
                gimli::Value::I32(v) => data.extend((v as u32).view_bits::<Lsb0>()),
                gimli::Value::U32(v) => data.extend(v.view_bits::<Lsb0>()),
                gimli::Value::I64(v) => data.extend((v as u64).view_bits::<Lsb0>()),
                gimli::Value::U64(v) => data.extend(v.view_bits::<Lsb0>()),
                gimli::Value::F32(v) => data.extend(v.to_bits().view_bits::<Lsb0>()),
                gimli::Value::F64(v) => data.extend(v.to_bits().view_bits::<Lsb0>()),
            }

            Some(data)
        }
        gimli::Location::Bytes { value } => value
            .get(0..variable_size as usize)
            .map(|b| b.view_bits().to_bitvec()),
        gimli::Location::ImplicitPointer {
            value: _,
            byte_offset: _,
        } => todo!("`ImplicitPointer` location not yet supported"),
    };

    // The piece can also specify offsets and a size, so adapt what we've just read to that
    if let Some(data) = data.as_mut() {
        if let Some(offset) = piece.bit_offset {
            data.drain(0..offset as usize);
        }
        if let Some(length) = piece.size_in_bits {
            data.truncate(length as usize);
        }
    }

    Ok(data)
}

/// Get all of the available variable data based on the [VariableLocationResult] of the [evaluate_location] function.
///
/// - `device_memory`: All the captured memory of the device
/// - `variable_size`: The size of the variable in bits
/// - `variable_location`: The location of the variable
fn get_variable_data(
    device_memory: &DeviceMemory<u32>,
    variable_size: u64,
    variable_location: VariableLocationResult,
) -> Result<BitVec<u8, Lsb0>, VariableDataError> {
    match variable_location {
        VariableLocationResult::NoLocationAttribute => Err(VariableDataError::OptimizedAway),
        VariableLocationResult::LocationListNotFound => Err(VariableDataError::OptimizedAway),
        VariableLocationResult::NoLocationFound => Err(VariableDataError::OptimizedAway),
        VariableLocationResult::LocationsFound(pieces) => {
            let mut data = BitVec::new();

            // Ceil-div with 8 to get the bytes we need to read
            let variable_size_bytes = div_ceil(variable_size, 8);

            // Get all the data of the pieces
            for piece in pieces {
                let piece_data = get_piece_data(device_memory, &piece, variable_size_bytes)?;

                if let Some(mut piece_data) = piece_data {
                    // TODO: Is this always in sequential order? We now assume that it is
                    data.append(&mut piece_data);
                } else {
                    // Data is not on the stack
                    return Err(VariableDataError::NoDataAvailableAt(format!(
                        "{:X?}",
                        piece.location
                    )));
                };
            }

            Ok(data)
        }
        VariableLocationResult::LocationEvaluationStepNotImplemented(step) => Err(
            VariableDataError::UnimplementedLocationEvaluationStep(format!("{:?}", step)),
        ),
    }
}

fn read_base_type(
    encoding: gimli::DwAte,
    data: &BitSlice<u8, Lsb0>,
) -> Result<Value<u32>, VariableDataError> {
    match encoding {
        gimli::constants::DW_ATE_unsigned => match data.len() {
            8 => Ok(Value::Uint(data.load_le::<u8>() as _)),
            16 => Ok(Value::Uint(data.load_le::<u16>() as _)),
            32 => Ok(Value::Uint(data.load_le::<u32>() as _)),
            64 => Ok(Value::Uint(data.load_le::<u64>() as _)),
            128 => Ok(Value::Uint(data.load_le::<u128>() as _)),
            _ => Err(VariableDataError::InvalidSize { bits: data.len() }),
        },
        gimli::constants::DW_ATE_signed => match data.len() {
            8 => Ok(Value::Int(data.load_le::<u8>() as _)),
            16 => Ok(Value::Int(data.load_le::<u16>() as _)),
            32 => Ok(Value::Int(data.load_le::<u32>() as _)),
            64 => Ok(Value::Int(data.load_le::<u64>() as _)),
            128 => Ok(Value::Int(data.load_le::<u128>() as _)),
            _ => Err(VariableDataError::InvalidSize { bits: data.len() }),
        },
        gimli::constants::DW_ATE_float => match data.len() {
            32 => Ok(Value::Float(f32::from_bits(data.load_le::<u32>()) as _)),
            64 => Ok(Value::Float(f64::from_bits(data.load_le::<u64>()) as _)),
            _ => Err(VariableDataError::InvalidSize { bits: data.len() }),
        },
        gimli::constants::DW_ATE_boolean => Ok(Value::Bool(data.iter().any(|v| *v))),
        gimli::constants::DW_ATE_address => match data.len() {
            32 => Ok(Value::Address(data.load_le::<u32>() as _)),
            _ => Err(VariableDataError::InvalidSize { bits: data.len() }),
        },
        t => Err(VariableDataError::UnsupportedBaseType {
            base_type: t,
            data: data.to_bitvec(),
        }),
    }
}

/// Gets a string representation of the variable
///
/// If it can be read, an Ok with the most literal value format is returned.
/// If it can not be read, an Err is returned with a user displayable error.
fn read_variable_data(
    mut variable: Pin<&mut TypeValueNode<u32>>,
    data: &BitSlice<u8, Lsb0>,
    device_memory: &DeviceMemory<u32>,
) {
    // We may not have enough data in some cases
    // I don't know why that is, so let's just print a warning
    if variable.data().bit_length() > data.len() as u64 {
        log::warn!(
            "Variable of type {} claims to take up {} bits, but only {} bits are available",
            variable.data().variable_type.name,
            variable.data().bit_range.end,
            data.len()
        );
    }

    match variable.data().variable_type.archetype {
        Archetype::TaggedUnion => {
            // The first child must be the descriminator and not one of the variants
            assert!(variable.front_mut().unwrap().data().name == "discriminant");

            // We have to read the discriminator, then select the active variant and then read that
            read_variable_data(variable.front_mut().unwrap(), data, device_memory);

            let discriminator_value = match &variable.front().unwrap().data().variable_value {
                Ok(value) => value.clone(),
                _ => {
                    return;
                }
            };

            // We know the discriminator value, so now we need to hunt for the active variant.
            // There may not be one though
            let active_variant = variable
                .iter_mut()
                .skip(1)
                .find(|variant| variant.data().variable_value.as_ref() == Ok(&discriminator_value));

            if let Some(active_variant) = active_variant {
                read_variable_data(active_variant, data, device_memory);
            } else if let Some(default_variant) = variable
                .iter_mut()
                .skip(1)
                .find(|variant| variant.data().variable_value.is_err())
            {
                // There is no active variant, so we need to go for the default
                read_variable_data(default_variant, data, device_memory);
            }
        }
        Archetype::TaggedUnionVariant => {
            read_variable_data(variable.front_mut().unwrap(), data, device_memory);
        }
        Archetype::Structure
        | Archetype::Union
        | Archetype::Class
        | Archetype::ObjectMemberPointer => {
            // Every member of this object is a child in the tree.
            // We simply need to read every child.

            for child in variable.iter_mut() {
                read_variable_data(child, data, device_memory);
            }

            if &variable.data().variable_type.name == "&str" {
                // This is a string
                let pointer = &variable
                    .iter()
                    .find(|field| field.data().name == "data_ptr")
                    .ok_or(())
                    .map(|node| &node.data().variable_value);
                let length = &variable
                    .iter()
                    .find(|field| field.data().name == "length")
                    .ok_or(())
                    .map(|node| &node.data().variable_value);

                match (pointer, length) {
                    (Ok(Ok(Value::Address(pointer))), Ok(Ok(Value::Uint(length)))) => {
                        // We can read the data. This works because the length field denotes the byte size, not the char size
                        let data = device_memory
                            .read_slice(*pointer as u64..*pointer as u64 + *length as u64);
                        if let Some(data) = data {
                            variable.data_mut().variable_value =
                                Ok(Value::String(data.to_vec(), StringFormat::Utf8));
                        } else {
                            // There's something wrong. Fall back to treating the string as an object
                            variable.data_mut().variable_value = Ok(Value::Object);
                        }
                    }
                    _ => {
                        log::error!("We started decoding a string, but found an error");
                        // There's something wrong. Fall back to treating the string as an object
                        variable.data_mut().variable_value = Ok(Value::Object);
                    }
                }
            } else {
                // This is a normal object
                variable.data_mut().variable_value = Ok(Value::Object);
            }
        }
        Archetype::BaseType(encoding) => {
            if variable.data().bit_length() == 0 && variable.data().variable_type.name == "()" {
                variable.data_mut().variable_value = Ok(Value::Unit);
            } else {
                variable.data_mut().variable_value =
                    match data.get(variable.data().bit_range_usize()) {
                        Some(data) => read_base_type(encoding, data),
                        None => Err(VariableDataError::NoDataAvailable),
                    };
            }
        }
        Archetype::Pointer => {
            // The variable is a number that is the address of the pointee.
            // The pointee is the single child of the tree.

            variable.data_mut().variable_value = match data.get(variable.data().bit_range_usize()) {
                Some(data) => read_base_type(gimli::constants::DW_ATE_address, data),
                None => Err(VariableDataError::NoDataAvailable),
            };

            let address = match variable.data().variable_value {
                Ok(Value::Address(addr)) => Ok(addr),
                _ => Err(VariableDataError::InvalidPointerData),
            };

            let mut pointee = variable
                .front_mut()
                .expect("Pointers must have a pointee child");

            match address {
                Ok(address) => {
                    let pointee_data = device_memory.read_slice(
                        address as u64..address as u64 + div_ceil(pointee.data().bit_range.end, 8),
                    );

                    match pointee_data {
                        Some(pointee_data) => {
                            read_variable_data(pointee, pointee_data.view_bits(), device_memory);
                        }
                        None => {
                            pointee.data_mut().variable_value =
                                Err(VariableDataError::NoDataAvailable);
                        }
                    }
                }
                Err(e) => pointee.data_mut().variable_value = Err(e),
            }
        }
        Archetype::Array => {
            variable.data_mut().variable_value = Ok(Value::Array);
            // The tree has all children that we have to read. These are the elements of the array
            for mut element in variable.iter_mut() {
                match data.get(element.data().bit_range_usize()) {
                    Some(_) => read_variable_data(element, data, device_memory),
                    None => {
                        element.data_mut().variable_value = Err(VariableDataError::NoDataAvailable)
                    }
                }
            }
        }
        Archetype::Enumeration => {
            variable.data_mut().variable_value = Ok(Value::Enumeration);

            // The first child of the enumeration is the base integer. We only have to read that one.
            read_variable_data(
                variable.front_mut().expect("Enumerations have a child"),
                data,
                device_memory,
            );
        }
        Archetype::Enumerator => {
            // Ignore, we don't have to do anything
        }
        Archetype::Subroutine => {
            // Ignore, there's nothing to do
        }
        Archetype::Unknown => {
            // Ignore, we don't know what to do
        }
    }
}

fn read_variable_entry(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    device_memory: &DeviceMemory<u32>,
    frame_base: Option<u32>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<Option<Variable<u32>>, TraceError> {
    let mut abstract_origin_tree =
        get_entry_abstract_origin_reference_tree(unit, abbreviations, entry)?;
    let abstract_origin_node = abstract_origin_tree
        .as_mut()
        .and_then(|tree| tree.root().ok());
    let abstract_origin_entry = abstract_origin_node.as_ref().map(|node| node.entry());

    // Get the name of the variable
    let variable_name = get_entry_name(dwarf, unit, entry);

    // Alternatively, get the name from the abstract origin
    let mut variable_name = match (variable_name, abstract_origin_entry) {
        (Err(_), Some(entry)) => get_entry_name(dwarf, unit, entry),
        (variable_name, _) => variable_name,
    };

    if entry.tag() == gimli::constants::DW_TAG_formal_parameter && variable_name.is_err() {
        log::trace!("Formal parameter does not have a name, renaming it to 'param'");
        variable_name = Ok("param".into());
    }

    // Get the type of the variable
    let variable_type_value_tree = get_entry_type_reference_tree(unit, abbreviations, entry)
        .and_then(|mut type_tree| {
            let type_root = type_tree.root()?;
            build_type_value_tree(dwarf, unit, abbreviations, type_root)
        });

    // Alternatively, get the type from the abstract origin
    let variable_type_value_tree = match (variable_type_value_tree, abstract_origin_entry) {
        (Err(_), Some(entry)) => get_entry_type_reference_tree(unit, abbreviations, entry)
            .and_then(|mut type_tree| {
                let type_root = type_tree.root()?;
                build_type_value_tree(dwarf, unit, abbreviations, type_root)
            }),
        (variable_type, _) => variable_type,
    };

    let variable_kind = VariableKind {
        zero_sized: variable_type_value_tree
            .as_ref()
            .map(|vt| vt.data().bit_length() == 0)
            .unwrap_or_default(),
        inlined: abstract_origin_entry.is_some(),
        parameter: entry.tag() == gimli::constants::DW_TAG_formal_parameter,
    };

    // Get the location of the variable
    let mut variable_file_location = find_entry_location(dwarf, unit, entry)?;
    if let (None, Some(abstract_origin_entry)) =
        (&variable_file_location.file, abstract_origin_entry)
    {
        variable_file_location = find_entry_location(dwarf, unit, abstract_origin_entry)?;
    }

    match (variable_name, variable_type_value_tree) {
        (Ok(variable_name), Ok(variable_type_value_tree)) if variable_kind.zero_sized => {
            Ok(Some(Variable {
                name: variable_name,
                kind: variable_kind,
                type_value: variable_type_value_tree,
                location: variable_file_location,
            }))
        }
        (Ok(variable_name), Ok(mut variable_type_value_tree)) => {
            let location_attr = entry.attr(gimli::constants::DW_AT_location)?;

            let location_attr = match (location_attr, abstract_origin_entry) {
                (None, Some(entry)) => entry.attr(gimli::constants::DW_AT_location)?,
                (location_attr, _) => location_attr,
            };

            // Get the location of the variable
            let variable_location =
                evaluate_location(dwarf, unit, device_memory, location_attr, frame_base)?;

            let variable_data = get_variable_data(
                device_memory,
                variable_type_value_tree.data().bit_length(),
                variable_location,
            );

            match variable_data {
                // We have the data so read the variable using it
                Ok(variable_data) => read_variable_data(
                    variable_type_value_tree.root_mut(),
                    &variable_data,
                    device_memory,
                ),
                // We couldn't get the data, so set the value to the error we got
                Err(e) => {
                    variable_type_value_tree
                        .root_mut()
                        .data_mut()
                        .variable_value = Err(e)
                }
            }

            Ok(Some(Variable {
                name: variable_name,
                kind: variable_kind,
                type_value: variable_type_value_tree,
                location: variable_file_location,
            }))
        }
        (Ok(variable_name), Err(type_error)) => {
            log::debug!(
                "Could not read the type of variable `{}`: {}",
                variable_name,
                type_error
            );
            Ok(None)
        }
        (Err(name_error), _) => {
            log::debug!("Could not get the name of a variable: {}", name_error);
            Ok(None)
        }
    }
}

pub fn find_variables_in_function(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    device_memory: &DeviceMemory<u32>,
    node: gimli::EntriesTreeNode<DefaultReader>,
) -> Result<Vec<Variable<u32>>, TraceError> {
    fn recursor(
        dwarf: &Dwarf<DefaultReader>,
        unit: &Unit<DefaultReader, usize>,
        abbreviations: &Abbreviations,
        device_memory: &DeviceMemory<u32>,
        node: gimli::EntriesTreeNode<DefaultReader>,
        variables: &mut Vec<Variable<u32>>,
        mut frame_base: Option<u32>,
    ) -> Result<(), TraceError> {
        let entry = node.entry();

        log::trace!(
            "Checking out the entry @ .debug_info: {:X}",
            unit.header.offset().as_debug_info_offset().unwrap().0 + entry.offset().0
        );

        if let Some(new_frame_base) = try_read_frame_base(dwarf, unit, device_memory, entry)? {
            frame_base = Some(new_frame_base);
        }

        if entry.tag() == gimli::constants::DW_TAG_variable
            || entry.tag() == gimli::constants::DW_TAG_formal_parameter
        {
            if let Some(variable) =
                read_variable_entry(dwarf, unit, abbreviations, device_memory, frame_base, entry)?
            {
                variables.push(variable);
            }
        }

        let mut children = node.children();
        while let Some(child) = children.next()? {
            recursor(
                dwarf,
                unit,
                abbreviations,
                device_memory,
                child,
                variables,
                frame_base,
            )?;
        }

        Ok(())
    }

    let mut variables = Vec::new();
    recursor(
        dwarf,
        unit,
        abbreviations,
        device_memory,
        node,
        &mut variables,
        None,
    )?;
    Ok(variables)
}

pub fn find_static_variables(
    dwarf: &Dwarf<DefaultReader>,
    device_memory: &DeviceMemory<u32>,
) -> Result<Vec<Variable<u32>>, TraceError> {
    fn recursor(
        dwarf: &Dwarf<DefaultReader>,
        unit: &Unit<DefaultReader, usize>,
        abbreviations: &Abbreviations,
        device_memory: &DeviceMemory<u32>,
        node: gimli::EntriesTreeNode<DefaultReader>,
        variables: &mut Vec<Variable<u32>>,
    ) -> Result<(), TraceError> {
        let entry = node.entry();

        match entry.tag() {
            gimli::constants::DW_TAG_compile_unit => {}
            gimli::constants::DW_TAG_namespace => {}
            gimli::constants::DW_TAG_structure_type
            | gimli::constants::DW_TAG_subprogram
            | gimli::constants::DW_TAG_enumeration_type
            | gimli::constants::DW_TAG_base_type
            | gimli::constants::DW_TAG_array_type
            | gimli::constants::DW_TAG_pointer_type
            | gimli::constants::DW_TAG_subroutine_type
            | gimli::constants::DW_TAG_typedef
            | gimli::constants::DW_TAG_restrict_type
            | gimli::constants::DW_TAG_const_type
            | gimli::constants::DW_TAG_union_type => return Ok(()),
            gimli::constants::DW_TAG_variable => {
                if let Some(variable) =
                    read_variable_entry(dwarf, unit, abbreviations, device_memory, None, entry)?
                {
                    variables.push(variable);
                }
            }
            tag => {
                log::error!("Unexpected tag in the search of static variables: {}", tag);
                return Ok(());
            }
        }

        let mut children = node.children();
        while let Some(child) = children.next()? {
            recursor(dwarf, unit, abbreviations, device_memory, child, variables)?;
        }

        Ok(())
    }

    let mut variables = Vec::new();
    let mut units = dwarf.units();
    while let Some(unit_header) = units.next()? {
        let abbreviations = dwarf.abbreviations(&unit_header)?;
        recursor(
            dwarf,
            &dwarf.unit(unit_header.clone())?,
            &abbreviations,
            device_memory,
            unit_header.entries_tree(&abbreviations, None)?.root()?,
            &mut variables,
        )?;
    }

    Ok(variables)
}
