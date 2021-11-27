use crate::{Enumerator, StructureMember, TemplateTypeParam, Variable, VariableType};
use addr2line::Context;
use gimli::{
    Abbreviations, AttributeValue, DebuggingInformationEntry, EndianReader, EntriesTree,
    EvaluationResult, Piece, Reader, RunTimeEndian, Unit,
};
use stackdump_capture::cortex_m::CortexMRegisters;
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
            let type_name = get_entry_name(context, unit, entry)?;
            let mut members = Vec::new();
            let mut type_params = Vec::new();

            let mut children = node.children();
            while let Ok(Some(child)) = children.next() {
                let member_entry = child.entry();

                let member_name = get_entry_name(context, unit, member_entry)?;
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
                }),
                gimli::constants::DW_TAG_union_type => Some(VariableType::Union {
                    type_name,
                    type_params,
                    members,
                }),
                gimli::constants::DW_TAG_class_type => Some(VariableType::Class {
                    type_name,
                    type_params,
                    members,
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

            let mut children = node.children();
            let child = children.next().unwrap().unwrap();
            let child_entry = child.entry();

            let member_type = get_entry_type_reference_tree(unit, abbreviations, child_entry)
                .map(|mut type_tree| {
                    type_tree
                        .root()
                        .map(|root| find_type(context, unit, abbreviations, root))
                        .ok()
                })
                .flatten()
                .flatten()
                .unwrap();

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
                member_type: Box::new(member_type),
                lower_bound,
                count: count
                    .unwrap_or_else(|| (upper_bound.unwrap() - lower_bound).try_into().unwrap()),
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
                // Each child is a DW_TAG_enumerator
                let enumerator_entry = child.entry();

                let enumerator_name = get_entry_name(context, unit, enumerator_entry)?;
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
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    registers: &CortexMRegisters,
    entry: &DebuggingInformationEntry<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
) -> Vec<Piece<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>> {
    let mut location_evaluation = entry
        .attr(gimli::constants::DW_AT_location)
        .unwrap()
        .unwrap()
        .exprloc_value()
        .unwrap()
        .evaluation(unit.encoding());

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

    location_evaluation.result()
}

fn get_variable_value(
    variable_type: &VariableType,
    variable_location: &Vec<Piece<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>>,
) -> Option<String> {
    Some(format!("{:X?}", variable_location))
}

pub fn find_variables(
    context: &Context<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    unit: &Unit<EndianReader<RunTimeEndian, Rc<[u8]>>, usize>,
    abbreviations: &Abbreviations,
    registers: &CortexMRegisters,
    node: gimli::EntriesTreeNode<EndianReader<RunTimeEndian, Rc<[u8]>>>,
    variables: &mut Vec<Variable>,
) {
    let entry = node.entry();

    if entry.tag() == gimli::constants::DW_TAG_variable {
        // Get the name of the variable
        let variable_name = get_entry_name(context, unit, entry);

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

        // Get the value of the variable
        let variable_location = get_variable_location(unit, registers, entry);

        match (variable_name, variable_type) {
            (Some(variable_name), Some(variable_type)) => {
                let variable_value = get_variable_value(&variable_type, &variable_location);

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
        find_variables(context, unit, abbreviations, registers, child, variables);
    }
}
