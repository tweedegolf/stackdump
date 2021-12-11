use crate::{Enumerator, StructureMember, TemplateTypeParam, Variable, VariableType};
use addr2line::Context;
use gimli::{
    Abbreviations, AttributeValue, DebuggingInformationEntry, EndianReader, EntriesTree,
    EvaluationResult, Piece, Reader, RunTimeEndian, Unit,
};
use stackdump_capture::cortex_m::CortexMRegisters;
use stackdump_core::device_memory::DeviceMemory;
use std::{rc::Rc, ops::Deref};

fn get_entry_name(
    context: &Context<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    entry: &DebuggingInformationEntry<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
) -> Option<String> {
    entry
        // Find the attribute
        .attr(gimli::constants::DW_AT_name)
        .ok()
        .flatten()
        // Read as a string type
        .map(|name| context.dwarf().attr_string(unit, name.value()).ok())
        .flatten()
        // Convert to String
        .map(|name| name.to_string().map(|name| name.to_string()).ok())
        .flatten()
}

fn get_entry_type_reference_tree<'abbrev, 'unit>(
    unit: &'unit Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    abbreviations: &'abbrev Abbreviations,
    entry: &DebuggingInformationEntry<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
) -> Option<EntriesTree<'abbrev, 'unit, EndianReader<RunTimeEndian, Rc<[u8]>>>> {
    entry
        // Find the attribute
        .attr(gimli::constants::DW_AT_type)
        .ok()
        .flatten()
        // Check its offset
        .map(|v_type| {
            if let AttributeValue::UnitRef(offset) = v_type.value() {
                Some(offset)
            } else {
                None
            }
        })
        .flatten()
        // Get the entries for the type
        .map(|type_offset| {
            unit.header
                .entries_tree(abbreviations, Some(type_offset))
                .ok()
        })
        .flatten()
}

fn find_type(
    context: &Context<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    abbreviations: &Abbreviations,
    node: gimli::EntriesTreeNode<EndianReader<RunTimeEndian, Rc<[u8]>>>,
) -> Option<VariableType> {
    let entry = node.entry();

    match entry.tag() {
        tag if tag == gimli::constants::DW_TAG_structure_type
            || tag == gimli::constants::DW_TAG_union_type
            || tag == gimli::constants::DW_TAG_class_type =>
        {
            let type_name = get_entry_name(context, unit, entry).unwrap();
            let mut members = Vec::new();
            let mut type_params = Vec::new();
            let byte_size = entry
                .attr(gimli::constants::DW_AT_byte_size)
                .unwrap()
                .unwrap()
                .value()
                .udata_value()
                .unwrap();

            let mut children = node.children();
            while let Ok(Some(child)) = children.next() {
                let member_entry = child.entry();

                let member_name = match get_entry_name(context, unit, member_entry) {
                    Some(member_name) => member_name,
                    None => continue, // Only care about named members for now
                };

                let member_type = get_entry_type_reference_tree(unit, abbreviations, member_entry)
                    .map(|mut type_tree| {
                        type_tree
                            .root()
                            .map(|root| find_type(context, unit, abbreviations, root))
                            .ok()
                    })
                    .flatten()
                    .flatten();

                match member_entry.tag() {
                    gimli::constants::DW_TAG_member => members.push(StructureMember {
                        name: member_name,
                        member_type: member_type.unwrap(),
                    }),
                    gimli::constants::DW_TAG_template_type_parameter => {
                        type_params.push(TemplateTypeParam {
                            name: member_name,
                            template_type: member_type.unwrap(),
                        })
                    }
                    gimli::constants::DW_TAG_subprogram => {} // Ignore
                    gimli::constants::DW_TAG_structure_type => {} // Ignore
                    _ => unimplemented!(
                        "Unexpected tag {:?} for {}",
                        member_entry.tag().static_string(),
                        type_name,
                    ),
                }
            }

            match tag {
                gimli::constants::DW_TAG_structure_type => Some(VariableType::Structure {
                    type_name,
                    type_params,
                    members,
                    byte_size,
                }),
                gimli::constants::DW_TAG_union_type => Some(VariableType::Union {
                    type_name,
                    type_params,
                    members,
                    byte_size,
                }),
                gimli::constants::DW_TAG_class_type => Some(VariableType::Class {
                    type_name,
                    type_params,
                    members,
                    byte_size,
                }),
                _ => unreachable!(),
            }
        }
        gimli::constants::DW_TAG_base_type => {
            let name = get_entry_name(context, unit, entry).unwrap();
            let encoding = entry
                .attr(gimli::constants::DW_AT_encoding)
                .unwrap()
                .map(|attr| {
                    if let AttributeValue::Encoding(encoding) = attr.value() {
                        Some(encoding)
                    } else {
                        None
                    }
                })
                .flatten()
                .unwrap();
            let byte_size = entry
                .attr(gimli::constants::DW_AT_byte_size)
                .unwrap()
                .unwrap()
                .value()
                .udata_value()
                .unwrap();

            Some(VariableType::BaseType {
                name,
                encoding,
                byte_size,
            })
        }
        gimli::constants::DW_TAG_pointer_type => {
            let name = get_entry_name(context, unit, entry).unwrap();
            let pointee_type = get_entry_type_reference_tree(unit, abbreviations, entry)
                .map(|mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| find_type(context, unit, abbreviations, root))
                        .ok()
                })
                .flatten()
                .flatten()
                .unwrap();

            Some(VariableType::PointerType {
                name,
                pointee_type: Box::new(pointee_type),
            })
        }
        gimli::constants::DW_TAG_array_type => {
            let array_type = get_entry_type_reference_tree(unit, abbreviations, entry)
                .map(|mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| find_type(context, unit, abbreviations, root))
                        .ok()
                })
                .flatten()
                .flatten()
                .unwrap();

            let byte_size = entry
                .attr(gimli::constants::DW_AT_byte_size)
                .unwrap()
                .map(|byte_size| byte_size.value().udata_value())
                .flatten();

            let mut children = node.children();
            let child = children.next().unwrap().unwrap();
            let child_entry = child.entry();

            let lower_bound = child_entry
                .attr(gimli::constants::DW_AT_lower_bound)
                .unwrap()
                .unwrap()
                .sdata_value()
                .unwrap_or(0);
            let count = child_entry
                .attr(gimli::constants::DW_AT_count)
                .ok()
                .flatten()
                .map(|value| value.udata_value())
                .flatten();
            let upper_bound = child_entry
                .attr(gimli::constants::DW_AT_upper_bound)
                .ok()
                .flatten()
                .map(|value| value.sdata_value())
                .flatten();

            Some(VariableType::ArrayType {
                array_type: Box::new(array_type),
                lower_bound,
                count: count
                    .unwrap_or_else(|| (upper_bound.unwrap() - lower_bound).try_into().unwrap()),
                byte_size,
            })
        }
        gimli::constants::DW_TAG_enumeration_type => {
            let name = get_entry_name(context, unit, entry).unwrap();
            let underlying_type = get_entry_type_reference_tree(unit, abbreviations, entry)
                .map(|mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| find_type(context, unit, abbreviations, root))
                        .ok()
                })
                .flatten()
                .flatten()
                .unwrap();

            let mut enumerators = Vec::new();

            let mut children = node.children();
            while let Ok(Some(child)) = children.next() {
                // Each child is a DW_TAG_enumerator or DW_TAG_subprogram
                let enumerator_entry = child.entry();

                if enumerator_entry.tag() == gimli::constants::DW_TAG_subprogram {
                    continue;
                }

                let enumerator_name = get_entry_name(context, unit, enumerator_entry).unwrap();
                let const_value = enumerator_entry
                    .attr(gimli::constants::DW_AT_const_value)
                    .unwrap()
                    .unwrap()
                    .value()
                    .sdata_value()
                    .unwrap();

                enumerators.push(Enumerator {
                    name: enumerator_name,
                    const_value,
                });
            }

            Some(VariableType::EnumerationType {
                name,
                underlying_type: Box::new(underlying_type),
                enumerators,
            })
        }
        gimli::constants::DW_TAG_subroutine_type => Some(VariableType::Subroutine),
        tag => {
            eprintln!(
                "Variable type not implement yet: {}",
                tag.static_string().unwrap()
            );
            None
        }
    }
}

fn get_variable_location(
    context: &Context<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    registers: &CortexMRegisters,
    entry: &DebuggingInformationEntry<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
) -> VariableLocationResult {
    let maybe_location = entry.attr(gimli::constants::DW_AT_location).unwrap();

    let location = match maybe_location {
        Some(location) => location.value(),
        None => return VariableLocationResult::NoLocationAttribute,
    };

    let location_expression = match location {
        AttributeValue::Exprloc(_) | AttributeValue::Block(_) => location.exprloc_value().unwrap(),
        AttributeValue::LocationListsRef(l) => {
            let mut locations = context.dwarf().locations(unit, l).unwrap();
            let mut location = None;

            while let Ok(Some(maybe_location)) = locations.next() {
                // The .debug_loc does not seem to count the thumb bit, so remove it
                let check_pc = u64::from(*registers.base.pc() & !super::THUMB_BIT);

                if check_pc >= maybe_location.range.begin && check_pc < maybe_location.range.end {
                    location = Some(maybe_location);
                    break;
                }
            }

            if let Some(location) = location {
                location.data
            } else {
                return VariableLocationResult::LocationListNotFound;
            }
        }
        _ => unreachable!(),
    };

    let mut location_evaluation = location_expression.evaluation(unit.encoding());

    let mut result = location_evaluation.evaluate().unwrap();
    while result != EvaluationResult::Complete {
        match result {
            EvaluationResult::RequiresRegister {
                register,
                base_type: _,
            } => {
                let value = registers.base.register(register.0 as usize);
                result = location_evaluation
                    .resume_with_register(gimli::Value::U32(*value))
                    .unwrap();
            }
            r => unimplemented!("Evaluation step unimplemented: {:?}", r),
        }
    }

    let mut result = location_evaluation.result();

    match result.len() {
        0 => VariableLocationResult::NoLocationFound,
        1 => VariableLocationResult::LocationFound(result.remove(0)),
        _ => VariableLocationResult::LocationsFound(result),
    }
}

fn get_variable_value(
    device_memory: &DeviceMemory,
    registers: &CortexMRegisters,
    variable_type: &VariableType,
    variable_location: VariableLocationResult,
) -> Result<String, String> {
    let variable_size = variable_type.get_variable_size();

    match variable_location {
        VariableLocationResult::NoLocationAttribute => {
            Err("Optimized away (No location attribute)".into())
        }
        VariableLocationResult::LocationListNotFound => {
            Err("Location list not found for the current PC value (A variable lower on the stack may contain the value)".into())
        }
        VariableLocationResult::NoLocationFound => {
            Err("Optimized away (No location at this point)".into())
        }
        VariableLocationResult::LocationFound(piece) => {
            let data = match piece.location.clone() {
                gimli::Location::Empty => return Err("Optimized away (Empty location)".into()),
                gimli::Location::Register { register } => match register.0 {
                    // Check R0..=R15
                    r @ 0..=15 => Some(registers.base.register(r.into()).to_le_bytes().to_vec()),
                    // Check S0..=S31
                    r @ 256..=271 => {
                        let s_l = registers.fpu.fpu_register(r as usize - 256).to_le_bytes();
                        let s_h = registers
                            .fpu
                            .fpu_register(r as usize - 256 + 1)
                            .to_le_bytes();
                        let mut data = vec![];
                        data.extend_from_slice(&s_l);
                        data.extend_from_slice(&s_h);
                        Some(data)
                    }
                    _ => unreachable!(
                        "Register {} is not available",
                        gimli::Arm::register_name(register).unwrap()
                    ),
                },
                gimli::Location::Address { address } => device_memory
                    .read_slice(address as usize..(address + variable_size) as usize)
                    .map(|b| b.to_vec()),
                gimli::Location::Value { value: _ } => todo!("`Value` location not yet supported"),
                gimli::Location::Bytes { value } => {
                    value.get(0..variable_size as usize).map(|b| b.to_vec())
                }
                gimli::Location::ImplicitPointer {
                    value: _,
                    byte_offset: _,
                } => todo!("`ImplicitPointer` location not yet supported"),
            };

            let data = if let Some(data) = data {
                data
            } else {
                // Data is not on the stack
                return Err("Data is not available in registers or stack".into());
            };

            read_variable(variable_type, &data, device_memory)
        }
        VariableLocationResult::LocationsFound(pieces) => {
            Err(format!("Multi piece variables not yet supported: {:?}", pieces))
        }
    }
}

fn read_variable(
    variable_type: &VariableType,
    data: &[u8],
    device_memory: &DeviceMemory,
) -> Result<String, String> {
    fn read_base_type(
        encoding: &gimli::DwAte,
        byte_size: &u64,
        data: &[u8],
    ) -> Result<String, String> {
        match *encoding {
            gimli::constants::DW_ATE_unsigned => match byte_size {
                1 => Ok(format!("{}", u8::from_le_bytes(data.try_into().unwrap()))),
                2 => Ok(format!("{}", u16::from_le_bytes(data.try_into().unwrap()))),
                4 => Ok(format!("{}", u32::from_le_bytes(data.try_into().unwrap()))),
                8 => Ok(format!("{}", u64::from_le_bytes(data.try_into().unwrap()))),
                16 => Ok(format!("{}", u128::from_le_bytes(data.try_into().unwrap()))),
                _ => unreachable!(),
            },
            gimli::constants::DW_ATE_signed => match byte_size {
                1 => Ok(format!("{}", i8::from_le_bytes(data.try_into().unwrap()))),
                2 => Ok(format!("{}", i16::from_le_bytes(data.try_into().unwrap()))),
                4 => Ok(format!("{}", i32::from_le_bytes(data.try_into().unwrap()))),
                8 => Ok(format!("{}", i64::from_le_bytes(data.try_into().unwrap()))),
                16 => Ok(format!("{}", i128::from_le_bytes(data.try_into().unwrap()))),
                _ => unreachable!(),
            },
            gimli::constants::DW_ATE_float => match byte_size {
                4 => Ok(format!("{}", f32::from_le_bytes(data.try_into().unwrap()))),
                8 => Ok(format!("{}", f64::from_le_bytes(data.try_into().unwrap()))),
                _ => unreachable!(),
            },
            t => Err(format!(
                "Unimplemented BaseType encoding {} - data: {:X?}",
                t.static_string().unwrap(),
                data
            )),
        }
    }

    match variable_type {
        VariableType::BaseType {
            encoding,
            byte_size,
            ..
        } => read_base_type(encoding, byte_size, data),
        VariableType::ArrayType {
            array_type, count, ..
        } => {
            let element_byte_size = array_type.get_variable_size() as usize;
            let element_data_chunks = data.chunks(element_byte_size);

            let mut values = Vec::new();

            for chunk in element_data_chunks.take(*count as usize) {
                values.push(read_variable(array_type, chunk, device_memory).unwrap_or_else(|e| e));
            }

            Ok(format!("[{}]", values.join(", ")))
        }
        VariableType::PointerType { pointee_type, .. } => {
            // Cortex m, so pointer is little endian u32
            let address = u32::from_le_bytes(data.try_into().unwrap()) as usize;
            let pointee_size = pointee_type.get_variable_size() as usize;
            let pointee_memory = device_memory.read_slice(address..(address + pointee_size));

            let pointee_value = match pointee_memory {
                Some(data) => read_variable(pointee_type, data, device_memory),
                None => Err(String::from("(Not within available memory)")),
            };

            Ok(format!(
                "*{:#010X} = {}",
                address,
                pointee_value.unwrap_or_else(|e| format!("Error: {}", e))
            ))
        }
        VariableType::EnumerationType {
            name,
            underlying_type,
            enumerators,
        } => {
            let underlying_value = match underlying_type.deref() {
                VariableType::BaseType {
                    encoding,
                    byte_size,
                    ..
                } => read_base_type(encoding, byte_size, data),
                t => Err(format!(
                    "Enumeration underlying type is not a BaseType: {}",
                    t.get_first_level_name()
                )),
            }?;

            let underlying_value: i64 = underlying_value.parse().map_err(|_| {
                format!(
                    "Could not parse the underlying type as an integer: {}",
                    underlying_value
                )
            })?;

            let enumerator = enumerators
                .iter()
                .find(|e| e.const_value == underlying_value);

            match enumerator {
                Some(enumerator) => Ok(format!("{}::{}", name, enumerator.name)),
                None => Err(format!("{}", underlying_value)),
            }
        }
        t => Err(format!(
            "(Unimplemented variable type {} ({}) - data: {:X?})",
            t.get_first_level_name(),
            t.get_raw_name(),
            data
        )),
    }
}

pub fn find_variables(
    context: &Context<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    abbreviations: &Abbreviations,
    registers: &CortexMRegisters,
    device_memory: &DeviceMemory,
    node: gimli::EntriesTreeNode<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    variables: &mut Vec<Variable>,
) {
    let entry = node.entry();

    if entry.tag() == gimli::constants::DW_TAG_variable
        || entry.tag() == gimli::constants::DW_TAG_formal_parameter
    {
        // Get the name of the variable
        let mut variable_name = get_entry_name(context, unit, entry);

        if entry.tag() == gimli::constants::DW_TAG_formal_parameter && variable_name.is_none() {
            variable_name = Some("param".into());
        }

        // Get the type of the variable
        let variable_type = get_entry_type_reference_tree(unit, abbreviations, entry)
            .map(|mut type_tree| {
                type_tree
                    .root()
                    .map(|root| find_type(context, unit, abbreviations, root))
                    .ok()
            })
            .flatten()
            .flatten();

        match (variable_name, variable_type) {
            (Some(variable_name), Some(variable_type))
                if variable_type.get_variable_size() == 0 =>
            {
                variables.push(Variable {
                    name: variable_name,
                    value: Ok("{ (ZST) }".into()),
                    variable_type,
                })
            }
            (Some(variable_name), Some(variable_type)) => {
                // Get the value of the variable
                let variable_location = get_variable_location(context, unit, registers, entry);

                let variable_value =
                    get_variable_value(device_memory, registers, &variable_type, variable_location);

                variables.push(Variable {
                    name: variable_name,
                    value: variable_value,
                    variable_type,
                })
            }
            _ => {}
        }
    }

    let mut children = node.children();
    while let Ok(Some(child)) = children.next() {
        find_variables(
            context,
            unit,
            abbreviations,
            registers,
            device_memory,
            child,
            variables,
        );
    }
}

#[derive(Debug)]
enum VariableLocationResult {
    /// The DW_AT_location attribute is missing
    NoLocationAttribute,
    /// The location list could not be found in the ELF
    LocationListNotFound,
    /// This variable is not present in memory at this point
    NoLocationFound,
    /// The variable is present in a single piece of memory
    LocationFound(Piece<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>),
    /// The variable is split up into multiple pieces of memory
    LocationsFound(Vec<Piece<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>>),
}
