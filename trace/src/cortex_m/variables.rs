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
    Enumerator, Location, StructureMember, TemplateTypeParam, Variable, VariableKind, VariableType,
};
use bitvec::prelude::*;
use gimli::{
    Abbreviations, Attribute, AttributeValue, DebuggingInformationEntry, Dwarf, EndianReader,
    EntriesTree, EvaluationResult, Piece, Reader, RunTimeEndian, Unit,
};
use stackdump_core::device_memory::DeviceMemory;
use std::{ops::Deref, rc::Rc};

type DefaultReader = EndianReader<RunTimeEndian, Rc<[u8]>>;

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
        &VariableType::BaseType {
            name: String::from("frame_base"),
            encoding: gimli::constants::DW_ATE_unsigned,
            byte_size: 4, // Frame base is 4 bytes on cortex-m
        },
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

/// Decodes the type of an entry.
///
/// The given node should come from the [get_entry_type_reference_tree]
/// and [get_entry_abstract_origin_reference_tree] functions.
fn find_type(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
) -> Result<VariableType, TraceError> {
    // Get the root entry and its tag
    let entry = node.entry();
    let entry_tag = entry.tag().to_string();

    // The tag tells us what the base type it
    match entry.tag() {
        tag if tag == gimli::constants::DW_TAG_structure_type
            || tag == gimli::constants::DW_TAG_union_type
            || tag == gimli::constants::DW_TAG_class_type =>
        {
            // We have an object with members
            // The informations we can gather is:
            // - type name
            // - type parameters
            // - the members of the object
            // - the byte size of the object

            let type_name = get_entry_name(dwarf, unit, entry)?;
            let mut members = Vec::new();
            let mut type_params = Vec::new();
            let byte_size = entry
                .required_attr(unit, gimli::constants::DW_AT_byte_size)?
                .required_udata_value()?;

            // The members of the object can be found by looking at the children of the node
            let mut children = node.children();
            while let Ok(Some(child)) = children.next() {
                let member_entry = child.entry();

                // Object children can be a couple of things:
                // - Member fields
                // - Type parameters
                // - Sub programs (methods)
                // - Other objects (TODO what does this mean?)

                // Member fields have a name, a type and a location offset (relative to the base of the object).
                // Type parameters only have a name and a type.
                // The rest of the children are ignored.

                let member_name = match get_entry_name(dwarf, unit, member_entry) {
                    Ok(member_name) => member_name,
                    Err(_) => continue, // Only care about named members for now
                };

                let member_type = || {
                    get_entry_type_reference_tree(unit, abbreviations, member_entry).map(
                        |mut type_tree| {
                            type_tree
                                .root()
                                .map(|root| find_type(dwarf, unit, abbreviations, root))
                        },
                    )??
                };

                match member_entry.tag() {
                    gimli::constants::DW_TAG_member => {
                        // TODO: Sometimes this is not a simple number, but a location expression.
                        // As of writing this has not come up, but I can imagine this is the case for C bitfields.
                        let member_location = member_entry
                            .required_attr(unit, gimli::constants::DW_AT_data_member_location)?
                            .required_udata_value()?;

                        members.push(StructureMember {
                            name: member_name,
                            member_type: member_type()?,
                            member_location,
                        });
                    }
                    gimli::constants::DW_TAG_template_type_parameter => {
                        type_params.push(TemplateTypeParam {
                            name: member_name,
                            template_type: member_type()?,
                        })
                    }
                    gimli::constants::DW_TAG_subprogram => {} // Ignore
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

            match tag {
                gimli::constants::DW_TAG_structure_type => Ok(VariableType::Structure {
                    name: type_name,
                    type_params,
                    members,
                    byte_size,
                }),
                gimli::constants::DW_TAG_union_type => Ok(VariableType::Union {
                    name: type_name,
                    type_params,
                    members,
                    byte_size,
                }),
                gimli::constants::DW_TAG_class_type => Ok(VariableType::Class {
                    name: type_name,
                    type_params,
                    members,
                    byte_size,
                }),
                _ => unreachable!(),
            }
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

            Ok(VariableType::BaseType {
                name,
                encoding,
                byte_size,
            })
        }
        gimli::constants::DW_TAG_pointer_type => {
            // A pointer in this context is just a number.
            // It has a name and a type that indicates the type of the object it points to.

            let pointee_type = get_entry_type_reference_tree(unit, abbreviations, entry).map(
                |mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| find_type(dwarf, unit, abbreviations, root))
                },
            )???;

            // Some pointers don't have names, but generally it is just `&<typename>`
            // So if only the name is missing, we can recover

            let name = get_entry_name(dwarf, unit, entry)
                .unwrap_or_else(|_| format!("&{}", pointee_type.type_name()));

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

            Ok(VariableType::PointerType {
                name,
                pointee_type: Box::new(pointee_type),
            })
        }
        gimli::constants::DW_TAG_array_type => {
            // Arrays are their own thing in DWARF.
            // They have no name.
            // What can be found on the entry are the type of the elements of the array and the byte size.
            // Arrays have one child entry that contains information about the indexing of the array.

            let array_type = get_entry_type_reference_tree(unit, abbreviations, entry).map(
                |mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| find_type(dwarf, unit, abbreviations, root))
                },
            )???;

            let byte_size = entry
                .attr(gimli::constants::DW_AT_byte_size)?
                .and_then(|bsize| bsize.udata_value());

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

            Ok(VariableType::ArrayType {
                array_type: Box::new(array_type),
                lower_bound,
                count,
                byte_size,
            })
        }
        gimli::constants::DW_TAG_enumeration_type => {
            // This is an enum type (like a C-style enum).
            // Enums have a name and also an underlying_type (which is usually an integer).
            // The entry also has a child `DW_TAG_enumerator` for each variant.

            let name = get_entry_name(dwarf, unit, entry)?;
            let underlying_type = get_entry_type_reference_tree(unit, abbreviations, entry)
                .map(|mut type_tree| {
                type_tree
                    .root()
                    .map(|root| find_type(dwarf, unit, abbreviations, root))
            })???;

            let mut enumerators = Vec::new();

            let mut children = node.children();
            while let Ok(Some(child)) = children.next() {
                // Each child is a DW_TAG_enumerator or DW_TAG_subprogram
                let enumerator_entry = child.entry();

                if enumerator_entry.tag() == gimli::constants::DW_TAG_subprogram {
                    continue;
                }

                // Each variant has a name and an integer value.
                // If the enum has that value, then the enum is of that variant.
                // This does of course not work for flag enums.

                let enumerator_name = get_entry_name(dwarf, unit, enumerator_entry)?;
                let const_value = enumerator_entry
                    .required_attr(unit, gimli::constants::DW_AT_const_value)?
                    .required_sdata_value()?;

                enumerators.push(Enumerator {
                    name: enumerator_name,
                    const_value,
                });
            }

            Ok(VariableType::EnumerationType {
                name,
                underlying_type: Box::new(underlying_type),
                enumerators,
            })
        }
        gimli::constants::DW_TAG_subroutine_type => Ok(VariableType::Subroutine),
        tag => Err(TraceError::TypeNotImplemented {
            type_name: tag.to_string(),
        }),
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
            r => return Ok(VariableLocationResult::LocationEvaluationStepNotImplemented(r)),
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
) -> Result<Option<bitvec::vec::BitVec<u8, Lsb0>>, String> {
    let mut data = match piece.location.clone() {
        gimli::Location::Empty => return Err("Optimized away (Empty location)".into()),
        gimli::Location::Register { register } => Some(
            device_memory
                .register(register)
                .map(|r| r.to_ne_bytes().view_bits().to_bitvec())
                .map_err(|e| e.to_string())?,
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
fn get_variable_data(
    device_memory: &DeviceMemory<u32>,
    variable_type: &VariableType,
    variable_location: VariableLocationResult,
) -> Result<BitVec<u8, Lsb0>, String> {
    let variable_size = variable_type.byte_size();

    match variable_location {
        VariableLocationResult::NoLocationAttribute => Err("Optimized away (always)".into()),
        VariableLocationResult::LocationListNotFound => Err("Optimized away".into()),
        VariableLocationResult::NoLocationFound => Err("Optimized away".into()),
        VariableLocationResult::LocationsFound(pieces) => {
            let mut data = BitVec::new();

            // Get all the data of the pieces
            for piece in pieces {
                let piece_data = get_piece_data(device_memory, &piece, variable_size)?;

                if let Some(mut piece_data) = piece_data {
                    // TODO: Is this always in sequential order? We now assume that it is
                    data.append(&mut piece_data);
                } else {
                    // Data is not on the stack
                    return Err(format!(
                        "Data is not available in device memory: {:X?}",
                        piece.location
                    ));
                };
            }

            Ok(data)
        }
        VariableLocationResult::LocationEvaluationStepNotImplemented(step) => Err(format!(
            "A required step of the location evaluation logic has not been implemented yet: {:?}",
            step
        )),
    }
}

/// Gets a string representation of the variable
///
/// If it can be read, an Ok with the most literal value format is returned.
/// If it can not be read, an Err is returned with a user displayable error.
///
/// TODO: using strings is convenient, but not very nice. This should return proper types
fn render_variable(
    variable_type: &VariableType,
    data: &BitSlice<u8, Lsb0>,
    device_memory: &DeviceMemory<u32>,
) -> Result<String, String> {
    // We may not have enough data in some cases
    // I don't know why that is, so let's just print a warning
    if ((data.len() / 8) as u64) < variable_type.byte_size() {
        log::warn!(
            "Variable of type {} has size of {}, but only {} bytes are available",
            variable_type.type_name(),
            variable_type.byte_size(),
            data.len() / 8
        );
    }

    // Sometimes we have more data than we need. So we just need to trim that off
    let data = &data[..(variable_type.byte_size() as usize * 8).min(data.len())];

    fn read_base_type(
        encoding: &gimli::DwAte,
        byte_size: &u64,
        type_name: &str,
        data: &BitSlice<u8, Lsb0>,
    ) -> Result<String, String> {
        // It's possible to read unit types here
        if *byte_size == 0 && type_name == "()" {
            return Ok("()".into());
        }

        match *encoding {
            gimli::constants::DW_ATE_unsigned => match byte_size {
                1 => Ok(format!("{}", data.load_le::<u8>())),
                2 => Ok(format!("{}", data.load_le::<u16>())),
                4 => Ok(format!("{}", data.load_le::<u32>())),
                8 => Ok(format!("{}", data.load_le::<u64>())),
                16 => Ok(format!("{}", data.load_le::<u128>())),
                _ => unreachable!("A byte_size of {} is not possible", byte_size),
            },
            gimli::constants::DW_ATE_signed => match byte_size {
                1 => Ok(format!("{}", data.load_le::<u8>() as i8)),
                2 => Ok(format!("{}", data.load_le::<u16>() as i16)),
                4 => Ok(format!("{}", data.load_le::<u32>() as i32)),
                8 => Ok(format!("{}", data.load_le::<u64>() as i64)),
                16 => Ok(format!("{}", data.load_le::<u128>() as i128)),
                _ => unreachable!("A byte_size of {} is not possible", byte_size),
            },
            gimli::constants::DW_ATE_float => match byte_size {
                4 => {
                    let f = f32::from_bits(data.load_le::<u32>());
                    // If the float is really big or small, then we want to format it using an exponent
                    if f.abs() >= 1_000_000_000.0 || f.abs() < (1.0 / 1_000_000_000.0) {
                        Ok(format!("{:e}", f))
                    } else {
                        Ok(format!("{}", f))
                    }
                }
                8 => {
                    let f = f64::from_bits(data.load_le::<u64>());
                    // If the float is really big or small, then we want to format it using an exponent
                    if f.abs() >= 1_000_000_000.0 || f.abs() < (1.0 / 1_000_000_000.0) {
                        Ok(format!("{:e}", f))
                    } else {
                        Ok(format!("{}", f))
                    }
                }
                _ => unreachable!("A byte_size of {} is not possible", byte_size),
            },
            gimli::constants::DW_ATE_boolean => Ok(format!("{}", data.iter().any(|v| *v))),
            t => Err(format!(
                "Unimplemented BaseType encoding {} - data: {:X?}",
                t, data
            )),
        }
    }

    match variable_type {
        VariableType::BaseType {
            encoding,
            byte_size,
            name,
        } => read_base_type(encoding, byte_size, name, data),
        VariableType::ArrayType {
            array_type, count, ..
        } => {
            let element_byte_size = array_type.byte_size() as usize;
            let element_data_chunks = data.chunks(element_byte_size * 8);

            // We need to gather the value of all of the elements of the array
            let mut values = Vec::new();

            for chunk in element_data_chunks.take(*count as usize) {
                values
                    .push(render_variable(array_type, chunk, device_memory).unwrap_or_else(|e| e));
            }

            // Join all the values together in the array syntax
            Ok(format!("[{}]", values.join(", ")))
        }
        VariableType::PointerType { pointee_type, .. } => {
            // Cortex m, so pointer is little endian u32
            // TODO: This should be made cross platform
            let address = data.load_le::<u32>() as u64;
            let pointee_size = pointee_type.byte_size() as u64;

            // To render the pointer, we need to dereference it, so read the memory the pointer points to
            let pointee_memory = device_memory.read_slice(address..(address + pointee_size));

            // Now render the object that the pointer points at
            let pointee_value = match pointee_memory {
                Some(data) => render_variable(pointee_type, data.view_bits(), device_memory),
                None => Err(String::from("Not within available memory")),
            };

            // We want to show everything, so the address, the dereferenced value and the type of the value
            Ok(format!(
                "*{:#010X} (= {} ({}))",
                address,
                pointee_value.unwrap_or_else(|e| format!("Error({})", e)),
                pointee_type.type_name(),
            ))
        }
        VariableType::EnumerationType {
            name,
            underlying_type,
            enumerators,
        } => {
            // We need to know what the underlying value is of the enum.
            // With the current structure, the easiest thing to do is to render it and then parse it as an integer
            // TODO: This should use proper types instead of strings
            let underlying_value = match underlying_type.deref() {
                VariableType::BaseType {
                    encoding,
                    byte_size,
                    name,
                } => read_base_type(encoding, byte_size, name, data),
                t => Err(format!(
                    "Enumeration underlying type is not a BaseType: {}",
                    t.type_name()
                )),
            }?;

            let underlying_value: i64 = underlying_value.parse().map_err(|_| {
                format!(
                    "Could not parse the underlying type as an integer: {}",
                    underlying_value
                )
            })?;

            // Try to get the variant that is used
            // This does not work when the enum is used as flags
            let enumerator = enumerators
                .iter()
                .find(|e| e.const_value == underlying_value);

            match enumerator {
                Some(enumerator) => Ok(format!("{}::{}", name, enumerator.name)),
                None => Err(format!("{}", underlying_value)),
            }
        }
        VariableType::Structure {
            name: type_name,
            members,
            ..
        }
        | VariableType::Class {
            name: type_name,
            members,
            ..
        }
        | VariableType::Union {
            name: type_name,
            members,
            ..
        } => {
            // If the type is a string, then we want to render it as a string instead of as an array
            // TODO: This kind of rendering should be placed somewhere else instead of here inline
            let string_render = if type_name == "&str" {
                // Let's render the string nicely
                let pointer = members
                    .iter()
                    .find(|m| matches!(m.member_type, VariableType::PointerType { .. }));
                let length = members.iter().find(|m| {
                    matches!(
                        m.member_type,
                        VariableType::BaseType {
                            encoding: gimli::constants::DW_ATE_unsigned,
                            byte_size: 4, // The length is a usize, so 4 bytes on cortex-m
                            ..
                        }
                    )
                });

                match (pointer, length) {
                    (Some(pointer), Some(length)) => {
                        let pointer_address = data[pointer.member_location as usize * 8..][..32]
                            .load_le::<u32>() as u64;
                        let length_value = data[length.member_location as usize * 8..][..32]
                            .load_le::<u32>() as u64;
                        let string_contents = device_memory
                            .read_slice(pointer_address..(pointer_address + length_value));

                        Some(match string_contents {
                            Some(string_contents) => match std::str::from_utf8(string_contents) {
                                Ok(string) => Ok(format!(
                                    "*{:#010X}:{} (= \"{}\")",
                                    pointer_address, length_value, string
                                )),
                                Err(e) => Err(format!(
                                    "Error(string @ *{:#X}:{} contains invalid characters: {})",
                                    pointer_address, length_value, e
                                )),
                            },
                            None => Err(format!(
                                "Error(string @ *{:#X}:{} is not within available memory)",
                                pointer_address, length_value
                            )),
                        })
                    }
                    _ => None,
                }
            } else {
                None
            };

            match string_render {
                Some(string_render) => string_render,
                None => {
                    log::debug!(
                        "Creating struct render for {} with {} bytes of data",
                        type_name,
                        data.len() / 8
                    );
                    let members_string = members
                        .iter()
                        .map(|member| {
                            log::debug!("Creating struct render for structure member: {}", member.name);

                            let member_size = member.member_type.byte_size() as usize;
                            log::trace!("Getting the data for structure member from location {} bytes with a length of {} bytes", member.member_location, member_size);
                            let member_data =
                                data.get(member.member_location as usize * 8..).and_then(|data| data.get(..member_size * 8));

                            let member_value = match member_data {
                                None => Err("Data not available".into()),
                                Some(member_data) => render_variable(&member.member_type, member_data, device_memory),
                            };

                            format!(
                                "{}: {}",
                                member.name,
                                member_value.unwrap_or_else(|e| format!("Error({})", e))
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");

                    Ok(format!("{} {{ {} }}", type_name, members_string))
                }
            }
        }
        VariableType::Subroutine => Ok("_".into()),
    }
}

fn read_variable_entry(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    device_memory: &DeviceMemory<u32>,
    frame_base: Option<u32>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<Option<Variable>, TraceError> {
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
    let variable_type =
        get_entry_type_reference_tree(unit, abbreviations, entry).and_then(|mut type_tree| {
            let type_root = type_tree.root()?;
            find_type(dwarf, unit, abbreviations, type_root)
        });

    // Alternatively, get the type from the abstract origin
    let variable_type = match (variable_type, abstract_origin_entry) {
        (Err(_), Some(entry)) => get_entry_type_reference_tree(unit, abbreviations, entry)
            .and_then(|mut type_tree| {
                let type_root = type_tree.root()?;
                find_type(dwarf, unit, abbreviations, type_root)
            }),
        (variable_type, _) => variable_type,
    };

    let variable_kind = VariableKind {
        zero_sized: variable_type
            .as_ref()
            .map(|vt| vt.byte_size() == 0)
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

    match (variable_name, variable_type) {
        (Ok(variable_name), Ok(variable_type)) if variable_type.byte_size() == 0 => {
            Ok(Some(Variable {
                name: variable_name,
                kind: variable_kind,
                value: Ok("{ (ZST) }".into()),
                variable_type,
                location: variable_file_location,
            }))
        }
        (Ok(variable_name), Ok(variable_type)) => {
            let location_attr = entry.attr(gimli::constants::DW_AT_location)?;

            let location_attr = match (location_attr, abstract_origin_entry) {
                (None, Some(entry)) => entry.attr(gimli::constants::DW_AT_location)?,
                (location_attr, _) => location_attr,
            };

            // Get the location of the variable
            let variable_location =
                evaluate_location(dwarf, unit, device_memory, location_attr, frame_base)?;

            let variable_data = get_variable_data(device_memory, &variable_type, variable_location);
            let variable_value = match variable_data {
                Ok(variable_data) => render_variable(&variable_type, &variable_data, device_memory),
                Err(e) => Err(e),
            };

            Ok(Some(Variable {
                name: variable_name,
                kind: variable_kind,
                value: variable_value,
                variable_type,
                location: variable_file_location,
            }))
        }
        (Ok(variable_name), Err(type_error)) => {
            log::debug!(
                "Could not read the type of variable `{}`: {}",
                variable_name,
                type_error
            );
            return Ok(None);
        }
        (Err(name_error), _) => {
            log::debug!("Could not get the name of a variable: {}", name_error);
            return Ok(None);
        }
    }
}

pub fn find_variables_in_function(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    device_memory: &DeviceMemory<u32>,
    node: gimli::EntriesTreeNode<DefaultReader>,
) -> Result<Vec<Variable>, TraceError> {
    fn recursor(
        dwarf: &Dwarf<DefaultReader>,
        unit: &Unit<DefaultReader, usize>,
        abbreviations: &Abbreviations,
        device_memory: &DeviceMemory<u32>,
        node: gimli::EntriesTreeNode<DefaultReader>,
        variables: &mut Vec<Variable>,
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
) -> Result<Vec<Variable>, TraceError> {
    fn recursor(
        dwarf: &Dwarf<DefaultReader>,
        unit: &Unit<DefaultReader, usize>,
        abbreviations: &Abbreviations,
        device_memory: &DeviceMemory<u32>,
        node: gimli::EntriesTreeNode<DefaultReader>,
        variables: &mut Vec<Variable>,
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

#[derive(Debug)]
enum VariableLocationResult {
    /// The DW_AT_location attribute is missing
    NoLocationAttribute,
    /// The location list could not be found in the ELF
    LocationListNotFound,
    /// This variable is not present in memory at this point
    NoLocationFound,
    /// A required step of the location evaluation logic has not been implemented yet
    LocationEvaluationStepNotImplemented(EvaluationResult<DefaultReader>),
    /// The variable is split up into multiple pieces of memory
    LocationsFound(Vec<Piece<DefaultReader, usize>>),
}
