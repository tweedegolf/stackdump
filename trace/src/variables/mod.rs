//! A module containing functions for finding and reading the variables of frames.
//!
//! These are (almost) all pure functions that get all of their context through the parameters.
//! This was decided because the datastructures that are involved are pretty complex and I didn't
//! want to add complexity.
//! All functions can be reasoned with on the function level.
//!

use crate::{
    error::TraceError,
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
    Abbreviations, Attribute, AttributeValue, DebugInfoOffset, DebuggingInformationEntry, Dwarf,
    EntriesTree, Evaluation, EvaluationResult, Piece, Reader, Unit, UnitOffset,
};
use stackdump_core::device_memory::DeviceMemory;
use std::{collections::HashMap, pin::Pin};

mod type_value_tree_building;

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

fn try_read_frame_base<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    device_memory: &DeviceMemory<W>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
) -> Result<Option<W>, TraceError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    let frame_base_location = evaluate_location(
        dwarf,
        unit,
        device_memory,
        entry.attr(gimli::constants::DW_AT_frame_base)?,
        None,
    )?;
    let frame_base_data = get_variable_data(
        device_memory,
        core::mem::size_of::<W>() as u64 * 8,
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
fn build_type_value_tree<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<TypeValueTree<W>, TraceError> {
    // Get the root entry and its tag
    let entry = node.entry();
    let entry_die_offset = entry.offset().to_debug_info_offset(&unit.header).unwrap();

    if let Some(existing_type) = type_cache.get(&entry_die_offset) {
        log::debug!(
            "Using cached type value tree for {:?} at {:X} (tag: {})",
            get_entry_name(dwarf, unit, entry),
            entry_die_offset.0,
            entry.tag()
        );

        return (*existing_type).clone();
    }

    log::debug!(
        "Building type value tree for {:?} at {:X} (tag: {})",
        get_entry_name(dwarf, unit, entry),
        entry_die_offset.0,
        entry.tag()
    );

    // The tag tells us what the base type it
    let result = match entry.tag() {
        gimli::constants::DW_TAG_variant_part => type_value_tree_building::build_tagged_union(
            dwarf,
            unit,
            abbreviations,
            node,
            type_cache,
        ),
        tag @ gimli::constants::DW_TAG_structure_type
        | tag @ gimli::constants::DW_TAG_union_type
        | tag @ gimli::constants::DW_TAG_class_type => type_value_tree_building::build_object(
            dwarf,
            unit,
            abbreviations,
            node,
            type_cache,
            tag,
        ),
        gimli::constants::DW_TAG_base_type => {
            type_value_tree_building::build_base_type(dwarf, unit, node)
        }
        gimli::constants::DW_TAG_pointer_type => {
            type_value_tree_building::build_pointer(dwarf, unit, abbreviations, node, type_cache)
        }
        gimli::constants::DW_TAG_array_type => {
            type_value_tree_building::build_array(dwarf, unit, abbreviations, node, type_cache)
        }
        gimli::constants::DW_TAG_typedef => {
            type_value_tree_building::build_typedef(dwarf, unit, abbreviations, node, type_cache)
        }
        gimli::constants::DW_TAG_enumeration_type => type_value_tree_building::build_enumeration(
            dwarf,
            unit,
            abbreviations,
            node,
            type_cache,
        ),
        gimli::constants::DW_TAG_subroutine_type => {
            let mut type_value_tree = TypeValueTree::new(TypeValue::default());
            let mut type_value = type_value_tree.root_mut();

            type_value.data_mut().variable_type.archetype = Archetype::Subroutine;
            Ok(type_value_tree)
        } // Ignore
        tag => Err(TraceError::TagNotImplemented {
            tag_name: tag.to_string(),
            entry_debug_info_offset: entry.offset().to_debug_info_offset(&unit.header).unwrap().0,
        }),
    };

    type_cache
        .entry(entry_die_offset)
        .or_insert_with(|| result.clone());

    result
}

/// Runs the location evaluation of gimli.
///
/// - `location`: The `DW_AT_location` attribute value of the entry of the variable we want to get the location of.
/// This may be a None if the variable has no location attribute.
fn evaluate_location<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    device_memory: &DeviceMemory<W>,
    location: Option<Attribute<DefaultReader>>,
    frame_base: Option<W>,
) -> Result<VariableLocationResult, TraceError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
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
                let check_pc = device_memory.register(gimli::Arm::PC)?;

                if check_pc.as_u64() >= maybe_location.range.begin
                    && check_pc.as_u64() < maybe_location.range.end
                {
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
    let result = evaluate_expression(
        dwarf,
        unit,
        device_memory,
        frame_base,
        location_expression.evaluation(unit.encoding()),
    );

    match result {
        Err(TraceError::LocationEvaluationStepNotImplemented(step)) => {
            Ok(VariableLocationResult::LocationEvaluationStepNotImplemented(step))
        }
        Err(e) => Err(e),
        Ok(pieces) if pieces.is_empty() => Ok(VariableLocationResult::NoLocationFound),
        Ok(pieces) => Ok(VariableLocationResult::LocationsFound(pieces)),
    }
}

fn evaluate_expression<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    device_memory: &DeviceMemory<W>,
    frame_base: Option<W>,
    mut evaluation: Evaluation<DefaultReader>,
) -> Result<Vec<Piece<DefaultReader, usize>>, TraceError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    // Now we need to evaluate everything.
    // DWARF has a stack based instruction set that needs to be executed.
    // Luckily, gimli already implements the bulk of it.
    // The evaluation stops when it requires some memory that we need to provide.
    let mut result = evaluation.evaluate()?;
    while result != EvaluationResult::Complete {
        log::trace!("Location evaluation result: {:?}", result);
        match result {
            EvaluationResult::RequiresRegister {
                register,
                base_type,
            } => {
                let value = device_memory.register(register)?;
                let value = match base_type.0 {
                    0 => gimli::Value::Generic(value.as_u64()),
                    val => return Err(TraceError::OperationNotImplemented { operation: format!("Other types than generic haven't been implemented yet. base_type value: {val}"), file: file!(), line: line!() } ),
                };
                result = evaluation.resume_with_register(value)?;
            }
            EvaluationResult::RequiresFrameBase if frame_base.is_some() => {
                result = evaluation.resume_with_frame_base(
                    frame_base.ok_or(TraceError::UnknownFrameBase)?.as_u64(),
                )?;
            }
            EvaluationResult::RequiresRelocatedAddress(address) => {
                // We have no relocations of code
                result = evaluation.resume_with_relocated_address(address)?;
            }
            EvaluationResult::RequiresEntryValue(ex) => {
                let entry_pieces = evaluate_expression(
                    dwarf,
                    unit,
                    device_memory,
                    frame_base,
                    ex.evaluation(unit.encoding()),
                )?;

                let entry_data = get_variable_data(
                    device_memory,
                    W::BITS as u64,
                    VariableLocationResult::LocationsFound(entry_pieces),
                )?;

                result = evaluation.resume_with_entry_value(gimli::Value::Generic(
                    entry_data.load_le::<W>().as_u64(), // TODO: What should be the endianness of this? Our device or the target device?
                ))?;
            }
            EvaluationResult::RequiresMemory {
                address,
                size,
                space: None,
                base_type: UnitOffset(0),
            } => {
                // This arm only accepts the generic base_type, so size should always be equal to the size of W
                assert_eq!(size as u32 * 8, W::BITS);

                let data = device_memory
                    .read_slice(address..address + size as u64)?
                    .ok_or(TraceError::MissingMemory(address))?;
                let value = gimli::Value::Generic(data.as_bits::<Lsb0>().load_le::<W>().as_u64());
                result = evaluation.resume_with_memory(value)?;
            }
            r => {
                return Err(TraceError::LocationEvaluationStepNotImplemented(
                    std::rc::Rc::new(r),
                ))
            }
        }
    }

    Ok(evaluation.result())
}

/// Reads the data of a piece of memory
///
/// The [Piece] is an indirect result of the [evaluate_location] function.
///
/// - `device_memory`: The captured memory of the device
/// - `piece`: The piece of memory location that tells us which data needs to be read
/// - `variable_size`: The size of the variable in bytes
fn get_piece_data<W: funty::Integral>(
    device_memory: &DeviceMemory<W>,
    piece: &Piece<DefaultReader, usize>,
    variable_size: u64,
) -> Result<Option<bitvec::vec::BitVec<u8, Lsb0>>, VariableDataError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    let mut data = match piece.location.clone() {
        gimli::Location::Empty => return Err(VariableDataError::OptimizedAway),
        gimli::Location::Register { register } => Some(
            device_memory
                .register(register)
                .map(|r| r.to_ne_bytes().view_bits().to_bitvec()) // TODO: Is this correct? Shouldn't this be the endianness of the target device?
                .map_err(|e| VariableDataError::NoDataAvailableAt(e.to_string()))?,
        ),
        gimli::Location::Address { address } => device_memory
            .read_slice(address..(address + variable_size))?
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
        } => {
            return Err(VariableDataError::OperationNotImplemented {
                operation: "`ImplicitPointer` location not yet supported".into(),
                file: file!(),
                line: line!(),
            })
        }
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
fn get_variable_data<W: funty::Integral>(
    device_memory: &DeviceMemory<W>,
    variable_size: u64,
    variable_location: VariableLocationResult,
) -> Result<BitVec<u8, Lsb0>, VariableDataError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
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

fn read_base_type<W: funty::Integral>(
    encoding: gimli::DwAte,
    data: &BitSlice<u8, Lsb0>,
) -> Result<Value<W>, VariableDataError> {
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
            8 => Ok(Value::Address(
                data.load_le::<u8>().try_into().ok().unwrap(),
            )),
            16 => Ok(Value::Address(
                data.load_le::<u16>().try_into().ok().unwrap(),
            )),
            32 => Ok(Value::Address(
                data.load_le::<u32>().try_into().ok().unwrap(),
            )),
            64 => Ok(Value::Address(
                data.load_le::<u64>().try_into().ok().unwrap(),
            )),
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
fn read_variable_data<W: funty::Integral>(
    mut variable: Pin<&mut TypeValueNode<W>>,
    data: &BitSlice<u8, Lsb0>,
    device_memory: &DeviceMemory<W>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
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
            read_variable_data(
                variable.front_mut().unwrap(),
                data,
                device_memory,
                type_cache,
            );

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
                read_variable_data(active_variant, data, device_memory, type_cache);
            } else if let Some(default_variant) = variable
                .iter_mut()
                .skip(1)
                .find(|variant| variant.data().variable_value.is_err())
            {
                // There is no active variant, so we need to go for the default
                read_variable_data(default_variant, data, device_memory, type_cache);
            }
        }
        Archetype::TaggedUnionVariant => {
            read_variable_data(
                variable.front_mut().unwrap(),
                data,
                device_memory,
                type_cache,
            );
        }
        Archetype::Structure
        | Archetype::Union
        | Archetype::Class
        | Archetype::ObjectMemberPointer => {
            // Every member of this object is a child in the tree.
            // We simply need to read every child.

            for child in variable.iter_mut() {
                read_variable_data(child, data, device_memory, type_cache);
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
                    (Ok(Ok(Value::Address(pointer))), Ok(Ok(Value::Uint(length))))
                        if *length < 64 * 1024 =>
                    {
                        // We can read the data. This works because the length field denotes the byte size, not the char size
                        let data = device_memory
                            .read_slice(pointer.as_u64()..pointer.as_u64() + *length as u64);
                        if let Ok(Some(data)) = data {
                            variable.data_mut().variable_value =
                                Ok(Value::String(data, StringFormat::Utf8));
                        } else {
                            // There's something wrong. Fall back to treating the string as an object
                            variable.data_mut().variable_value = Ok(Value::Object);
                        }
                    }
                    (Ok(Ok(Value::Address(_))), Ok(Ok(Value::Uint(length))))
                        if *length >= 64 * 1024 =>
                    {
                        log::warn!(
                            "We started decoding the string {}, but it is {length} bytes long",
                            variable.data().name
                        );
                        // There's something wrong. Fall back to treating the string as an object
                        variable.data_mut().variable_value = Ok(Value::Object);
                    }
                    _ => {
                        log::error!(
                            "We started decoding the string {}, but found an error",
                            variable.data().name
                        );
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
        Archetype::Pointer(die_offset) => {
            // The variable is a number that is the address of the pointee.
            // The pointee is not part of this tree yet and has to be looked up through the type_cache.
            // This is done so that we cannot get an infinite recursive type due to e.g. linked lists.

            variable.data_mut().variable_value = match data.get(variable.data().bit_range_usize()) {
                Some(data) => read_base_type(gimli::constants::DW_ATE_address, data),
                None => Err(VariableDataError::NoDataAvailable),
            };

            let address = match variable.data().variable_value {
                Ok(Value::Address(addr)) => Ok(addr),
                _ => Err(VariableDataError::InvalidPointerData),
            };

            let pointee_tree_clone = match type_cache
                .get(&die_offset)
                .expect("Pointers must have their pointee type cached")
                .clone()
            {
                Ok(pointee_tree_clone) => pointee_tree_clone,
                Err(_) => TypeValueTree::new(TypeValue {
                    name: "Pointee".into(),
                    variable_type: VariableType {
                        name: "".into(),
                        archetype: Archetype::Unknown,
                    },
                    bit_range: 0..0,
                    variable_value: Err(VariableDataError::Unknown),
                }),
            };
            variable.push_back(pointee_tree_clone);
            let mut pointee = variable.back_mut().unwrap();

            match address {
                Ok(address) if address == W::ZERO => {
                    pointee.data_mut().variable_value = Err(VariableDataError::NullPointer)
                }
                Ok(address) => {
                    let pointee_data = device_memory.read_slice(
                        address.as_u64()
                            ..address.as_u64() + div_ceil(pointee.data().bit_range.end, 8),
                    );

                    match pointee_data {
                        Ok(Some(pointee_data)) => {
                            read_variable_data(
                                pointee,
                                pointee_data.view_bits(),
                                device_memory,
                                type_cache,
                            );
                        }
                        Ok(None) => {
                            pointee.data_mut().variable_value =
                                Err(VariableDataError::NoDataAvailable);
                        }
                        Err(e) => {
                            pointee.data_mut().variable_value = Err(e.into());
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
                    Some(_) => read_variable_data(element, data, device_memory, type_cache),
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
                type_cache,
            );
        }
        Archetype::Typedef => {
            variable.data_mut().variable_value = Ok(Value::Typedef);

            // The first child of the enumeration is the base integer. We only have to read that one.
            read_variable_data(
                variable.front_mut().expect("Typedefs have a child"),
                data,
                device_memory,
                type_cache,
            );
        }
        Archetype::Enumerator => {
            // Ignore, we don't have to do anything
        }
        Archetype::Subroutine => {
            variable.data_mut().variable_value = Ok(Value::Object);
            // Ignore, there's nothing to do
        }
        Archetype::Unknown => {
            // Ignore, we don't know what to do
        }
    }
}

fn read_variable_entry<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    device_memory: &DeviceMemory<W>,
    frame_base: Option<W>,
    entry: &DebuggingInformationEntry<DefaultReader, usize>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<Option<Variable<W>>, TraceError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
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
            build_type_value_tree(dwarf, unit, abbreviations, type_root, type_cache)
        });

    // Alternatively, get the type from the abstract origin
    let variable_type_value_tree = match (variable_type_value_tree, abstract_origin_entry) {
        (Err(_), Some(entry)) => get_entry_type_reference_tree(unit, abbreviations, entry)
            .and_then(|mut type_tree| {
                let type_root = type_tree.root()?;
                build_type_value_tree(dwarf, unit, abbreviations, type_root, type_cache)
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

            log::debug!(
                "Reading variable data for `{variable_name}` at {variable_location:X?} of {} bits",
                variable_type_value_tree.data().bit_length()
            );
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
                    type_cache,
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

pub fn find_variables_in_function<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    unit: &Unit<DefaultReader, usize>,
    abbreviations: &Abbreviations,
    device_memory: &DeviceMemory<W>,
    node: gimli::EntriesTreeNode<DefaultReader>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<Vec<Variable<W>>, TraceError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    #[allow(clippy::too_many_arguments)]
    fn recursor<W: funty::Integral>(
        dwarf: &Dwarf<DefaultReader>,
        unit: &Unit<DefaultReader, usize>,
        abbreviations: &Abbreviations,
        device_memory: &DeviceMemory<W>,
        node: gimli::EntriesTreeNode<DefaultReader>,
        variables: &mut Vec<Variable<W>>,
        mut frame_base: Option<W>,
        type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
    ) -> Result<(), TraceError>
    where
        <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
    {
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
            if let Some(variable) = read_variable_entry(
                dwarf,
                unit,
                abbreviations,
                device_memory,
                frame_base,
                entry,
                type_cache,
            )? {
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
                type_cache,
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
        type_cache,
    )?;
    Ok(variables)
}

pub fn find_static_variables<W: funty::Integral>(
    dwarf: &Dwarf<DefaultReader>,
    device_memory: &DeviceMemory<W>,
    type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
) -> Result<Vec<Variable<W>>, TraceError>
where
    <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
{
    fn recursor<W: funty::Integral>(
        dwarf: &Dwarf<DefaultReader>,
        unit: &Unit<DefaultReader, usize>,
        abbreviations: &Abbreviations,
        device_memory: &DeviceMemory<W>,
        node: gimli::EntriesTreeNode<DefaultReader>,
        variables: &mut Vec<Variable<W>>,
        type_cache: &mut HashMap<DebugInfoOffset, Result<TypeValueTree<W>, TraceError>>,
    ) -> Result<(), TraceError>
    where
        <W as funty::Numeric>::Bytes: bitvec::view::BitView<Store = u8>,
    {
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
                if let Some(variable) = read_variable_entry(
                    dwarf,
                    unit,
                    abbreviations,
                    device_memory,
                    None,
                    entry,
                    type_cache,
                )? {
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
            recursor(
                dwarf,
                unit,
                abbreviations,
                device_memory,
                child,
                variables,
                type_cache,
            )?;
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
            type_cache,
        )?;
    }

    Ok(variables)
}
