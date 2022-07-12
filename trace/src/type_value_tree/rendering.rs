use super::{value::Value, variable_type::Archetype, TypeValueNode, TypeValueTree};
use crate::{
    render_colors::{Theme, ThemeColors},
    type_value_tree::VariableDataError,
};
use colored::ColoredString;
use phf::phf_map;

pub fn render_type_value_tree<ADDR: funty::Integral>(
    type_value_tree: &TypeValueTree<ADDR>,
    theme: Theme,
) -> ColoredString {
    render_unknown(type_value_tree.root(), theme)
}

fn render_unknown<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    if let Err(e) = &type_value_node.data().variable_value {
        return format!("{{{}}}", theme.color_invalid(e.to_string()))
            .as_str()
            .into();
    };

    match type_value_node.data().variable_type.archetype {
        Archetype::TaggedUnion => render_tagged_union(type_value_node, theme),
        Archetype::Structure
        | Archetype::Union
        | Archetype::Class
        | Archetype::ObjectMemberPointer => render_object(type_value_node, theme),
        Archetype::BaseType(_) => render_base_type(type_value_node, theme),
        Archetype::Pointer(_) => render_pointer(type_value_node, theme),
        Archetype::Array => render_array(type_value_node, theme),
        Archetype::Typedef => render_typedef(type_value_node, theme),
        Archetype::Enumeration => render_enumeration(type_value_node, theme),
        Archetype::Enumerator | Archetype::TaggedUnionVariant => {
            unreachable!("Should never appear during rendering directly")
        }
        Archetype::Subroutine => "_".into(),
        Archetype::Unknown => "?".into(),
    }
}

fn render_tagged_union<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    let discriminant = type_value_node.front().unwrap().data();
    assert_eq!(&discriminant.name, "discriminant");
    let discriminant_value = match &discriminant.variable_value {
        Ok(value) => value,
        Err(e) => return format!("{{{}}}", theme.color_invalid(e)).as_str().into(),
    };

    let active_variant = match type_value_node
        .iter()
        .skip(1)
        .find(|variant| variant.data().variable_value.as_ref() == Ok(discriminant_value))
    {
        Some(variant) => Some(variant),
        None => {
            // Let's look for the default variant
            type_value_node.iter().skip(1).find(|variant| {
                variant.data().variable_value == Err(VariableDataError::NoDataAvailable)
            })
        }
    };

    match active_variant {
        Some(active_variant) => render_unknown(active_variant.front().unwrap(), theme),
        None => format!(
            "{{{} {}}}",
            theme.color_invalid("invalid discriminant:"),
            theme.color_invalid(discriminant_value)
        )
        .as_str()
        .into(),
    }
}

fn render_object<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    // Check if the object is a string
    if let Ok(s @ Value::String(_, _)) = type_value_node.data().variable_value.as_ref() {
        return theme.color_string_value(s);
    }

    // Check if the object is transparent
    if let Some(field_name) = TRANSPARENT_TYPES.get(
        type_value_node
            .data()
            .variable_type
            .name
            .split('<')
            .next()
            .unwrap(),
    ) {
        // Now we need to find the field that the object is transparent to.
        // These types can be updated in the future without warning, so if the field cannot be found, then we're just gonna
        // render them normally

        for field in type_value_node.iter() {
            if &field.data().name == field_name {
                return render_unknown(field, theme);
            }
        }
    }

    let mut output = theme
        .color_type_name(&type_value_node.data().variable_type.name)
        .to_string();

    output.push_str(" { ");

    // The fields of the object can be are the children in the tree
    output.push_str(
        &type_value_node
            .iter()
            .map(|field| {
                format!(
                    "{}: {}",
                    theme.color_variable_name(&field.data().name),
                    render_unknown(field, theme)
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );

    output.push_str(" }");

    output.as_str().into()
}

fn render_base_type<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    theme.color_numeric_value(type_value_node.data().variable_value.as_ref().unwrap())
}

fn render_pointer<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    let pointer_address = match type_value_node.data().variable_value.as_ref().unwrap() {
        super::value::Value::Address(addr) => addr,
        _ => unreachable!(),
    };

    let pointee = type_value_node.front().unwrap();
    format!(
        "*{} = {}",
        theme.color_numeric_value(format!("{pointer_address:#X}")),
        render_unknown(pointee, theme)
    )
    .as_str()
    .into()
}

fn render_array<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    let mut output = String::new();

    output.push('[');

    // The values are the children of the tree
    output.push_str(
        &type_value_node
            .iter()
            .map(|element| render_unknown(element, theme).to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );

    output.push(']');

    output.as_str().into()
}

fn render_typedef<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    // When important, the typename has already been printed.
    // We just really only want to see the value, so we act like a transparent type.

    render_unknown(
        type_value_node.front().expect("Typedefs have a child"),
        theme,
    )
}

fn render_enumeration<ADDR: funty::Integral>(
    type_value_node: &TypeValueNode<ADDR>,
    theme: Theme,
) -> ColoredString {
    let base_value = match &type_value_node.front().unwrap().data().variable_value {
        Ok(base_value) => base_value,
        Err(e) => {
            return format!("{{{}}}", theme.color_invalid(e)).as_str().into();
        }
    };

    for enumerator in type_value_node.iter().skip(1) {
        if let Ok(enumerator_value) = enumerator.data().variable_value.as_ref() {
            if enumerator_value == base_value {
                return theme.color_enum_member(&enumerator.data().name);
            }
        }
    }

    theme.color_numeric_value(base_value)
}

/// List with the known transparent types (or types that are effectively transparent)
///
/// The key is the typename before any generics (so, before the '<' character) and the value is the fieldname
/// the type is transparent to.
static TRANSPARENT_TYPES: phf::Map<&'static str, &'static str> = phf_map! {
    "ManuallyDrop" => "value",
    "MaybeUninit" => "value",
    "UnsafeCell" => "value",
    "Cell" => "value",
    "AtomicBool" => "v",
    "AtomicI8" => "v",
    "AtomicI16" => "v",
    "AtomicI32" => "v",
    "AtomicI64" => "v",
    "AtomicIsize" => "v",
    "AtomicPtr" => "v",
    "AtomicU8" => "v",
    "AtomicU16" => "v",
    "AtomicU32" => "v",
    "AtomicU64" => "v",
    "AtomicUsize" => "v",
};
