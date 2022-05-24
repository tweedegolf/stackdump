use super::{variable_type::Archetype, AddressType, TypeValueNode, TypeValueTree, value::Value};

pub fn render_type_value_tree<ADDR: AddressType>(type_value_tree: &TypeValueTree<ADDR>) -> String {
    render_unknown(type_value_tree.root())
}

fn render_unknown<ADDR: AddressType>(type_value_node: &TypeValueNode<ADDR>) -> String {
    if let Err(e) = &type_value_node.data().variable_value {
        return format!("{{{e}}}");
    };

    match type_value_node.data().variable_type.archetype {
        Archetype::TaggedUnion => {
            todo!()
        }
        Archetype::Structure | Archetype::Union | Archetype::Class => {
            render_object(type_value_node)
        }
        Archetype::BaseType(_) => render_base_type(type_value_node),
        Archetype::Pointer => render_pointer(type_value_node),
        Archetype::Array => render_array(type_value_node),
        Archetype::Enumeration => render_enumeration(type_value_node),
        Archetype::Enumerator | Archetype::TaggedUnionVariant => unreachable!("Should never appear during rendering directly"),
        Archetype::Subroutine => "_".into(),
        Archetype::Unknown => "?".into(),
    }
}

fn render_object<ADDR: AddressType>(type_value_node: &TypeValueNode<ADDR>) -> String {
    if let Ok(s @ Value::String (_, _)) = type_value_node.data().variable_value.as_ref() {
        return s.to_string();
    }

    let mut output = type_value_node.data().variable_type.name.clone();

    output.push_str(" { ");

    // The fields of the object can be are the children in the tree
    output.push_str(
        &type_value_node
            .iter()
            .map(|field| format!("{}: {}", field.data().name, render_unknown(field)))
            .collect::<Vec<_>>()
            .join(", "),
    );

    output.push_str(" }");

    output
}

fn render_base_type<ADDR: AddressType>(type_value_node: &TypeValueNode<ADDR>) -> String {
    type_value_node
        .data()
        .variable_value
        .as_ref()
        .unwrap()
        .to_string()
}

fn render_pointer<ADDR: AddressType>(type_value_node: &TypeValueNode<ADDR>) -> String {
    let pointer_address = match type_value_node.data().variable_value.as_ref().unwrap() {
        super::value::Value::Address(addr) => addr,
        _ => unreachable!(),
    };

    let pointee = type_value_node.front().unwrap();
    format!("*{pointer_address:#X} = {}", render_unknown(pointee))
}

fn render_array<ADDR: AddressType>(type_value_node: &TypeValueNode<ADDR>) -> String {
    let mut output = String::new();

    output.push('[');

    // The values are the children of the tree
    output.push_str(
        &type_value_node
            .iter()
            .map(render_unknown)
            .collect::<Vec<_>>()
            .join(", "),
    );

    output.push(']');

    output
}

fn render_enumeration<ADDR: AddressType>(type_value_node: &TypeValueNode<ADDR>) -> String {
    let base_value = match &type_value_node.front().unwrap().data().variable_value {
        Ok(base_value) => base_value,
        Err(e) => {
            return format!("{{{e}}}");
        },
    };

    for enumerator in type_value_node.iter().skip(1) {
        if let Ok(enumerator_value) = enumerator.data().variable_value.as_ref() {
            if enumerator_value == base_value {
                return enumerator.data().name.clone();
            }
        }
    }

    base_value.to_string()
}
