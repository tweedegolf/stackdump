use crate::{Enumerator, StructureMember, TemplateTypeParam, Variable, VariableType};
use addr2line::Context;
use gimli::{
    Abbreviations, AttributeValue, DebuggingInformationEntry, EndianReader, EntriesTree,
    EvaluationResult, Piece, Reader, RunTimeEndian, Unit,
};
use stackdump_capture::cortex_m::CortexMRegisters;
use stackdump_core::MemoryRegion;
use std::rc::Rc;

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
        gimli::constants::DW_TAG_subroutine_type => {
            Some(VariableType::Subroutine)
        }
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
) -> Vec<Piece<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>> {
    let maybe_location = entry.attr(gimli::constants::DW_AT_location).unwrap();

    let location = match maybe_location {
        Some(location) => location.value(),
        None => return Vec::new(),
    };

    let location_expression = match location {
        AttributeValue::Exprloc(_) | AttributeValue::Block(_) => location.exprloc_value().unwrap(),
        AttributeValue::LocationListsRef(l) => {
            let mut locations = context.dwarf().locations(unit, l).unwrap();
            let mut location = None;

            while let Ok(Some(maybe_location)) = locations.next() {
                if u64::from(*registers.base.pc()) >= maybe_location.range.begin
                    && u64::from(*registers.base.pc()) < maybe_location.range.end
                {
                    location = Some(maybe_location);
                }
            }

            if let Some(location) = location {
                location.data
            } else {
                return Vec::new();
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
                println!("Register {}: {:#08X}", register.0, value);
                result = location_evaluation
                    .resume_with_register(gimli::Value::U32(*value))
                    .unwrap();
            }
            r => unimplemented!("Evaluation step unimplemented: {:?}", r),
        }
    }

    location_evaluation.result()
}

fn get_variable_value<const STACK_SIZE: usize>(
    stack: &MemoryRegion<STACK_SIZE>,
    registers: &CortexMRegisters,
    variable_type: &VariableType,
    variable_location: &Vec<Piece<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>>,
) -> Option<String> {
    let variable_size = variable_type.get_variable_size();

    if variable_location.len() == 0 {
        return None;
    }

    let data = if variable_location.len() == 1 {
        let piece = variable_location.first().unwrap();

        match piece.location.clone() {
            gimli::Location::Empty => return Some("Optimized away".into()),
            gimli::Location::Register { register } => Some(
                registers
                    .base
                    .register(register.0.into())
                    .to_le_bytes()
                    .to_vec(),
            ),
            gimli::Location::Address { address } => stack
                .read_slice(address as usize..(address + variable_size) as usize)
                .map(|b| b.to_vec()),
            gimli::Location::Value { value: _ } => todo!(),
            gimli::Location::Bytes { value } => {
                value.get(0..variable_size as usize).map(|b| b.to_vec())
            }
            gimli::Location::ImplicitPointer {
                value: _,
                byte_offset: _,
            } => todo!(),
        }
    } else {
        return None;
    };

    let data = if let Some(data) = data {
        data
    } else {
        // Data is not on the stack
        return Some("Data is not available".into());
    };

    Some(read_variable(variable_type, &data))
}

fn read_variable(variable_type: &VariableType, data: &[u8]) -> String {
    match variable_type {
        VariableType::BaseType {
            encoding,
            byte_size,
            ..
        } => match encoding.clone() {
            gimli::constants::DW_ATE_unsigned => match byte_size {
                1 => format!("{}", u8::from_le_bytes(data.try_into().unwrap())),
                2 => format!("{}", u16::from_le_bytes(data.try_into().unwrap())),
                4 => format!("{}", u32::from_le_bytes(data.try_into().unwrap())),
                8 => format!("{}", u64::from_le_bytes(data.try_into().unwrap())),
                _ => unreachable!(),
            },
            t => format!(
                "Unimplemented encoding {} - data: {:X?}",
                t.static_string().unwrap(),
                data
            ),
        },
        VariableType::ArrayType {
            array_type, count, ..
        } => {
            let element_byte_size = array_type.get_variable_size() as usize;
            let element_data_chunks = data.chunks(element_byte_size);

            let mut values = Vec::new();

            for chunk in element_data_chunks.take(*count as usize) {
                values.push(read_variable(array_type, chunk));
            }

            format!("[{}]", values.join(", "))
        }
        t => format!(
            "Unimplemented variable type {} - data: {:X?}",
            t.get_first_level_name(),
            data
        ),
    }
}

pub fn find_variables<const STACK_SIZE: usize>(
    context: &Context<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    abbreviations: &Abbreviations,
    registers: &CortexMRegisters,
    stack: &MemoryRegion<STACK_SIZE>,
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
            (Some(variable_name), Some(variable_type)) if variable_type.get_variable_size() == 0 => {
                variables.push(Variable {
                    name: variable_name,
                    value: Some("{}".into()),
                    variable_type,
                })
            }
            (Some(variable_name), Some(variable_type)) => {
                // Get the value of the variable
                let variable_location = get_variable_location(context, unit, registers, entry);

                println!("{} is at {:X?}", variable_name, variable_location);
                let variable_value =
                    get_variable_value(stack, registers, &variable_type, &variable_location);

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
            stack,
            child,
            variables,
        );
    }
}
