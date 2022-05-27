pub mod dark {
    use colored::{ColoredString, Colorize};
    use std::fmt::Display;

    pub fn color_numeric_value<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0xb5, 0xce, 0xa8)
    }
    pub fn color_invalid<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0xf4, 0x47, 0x47)
    }
    pub fn color_string_value<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0xce, 0x91, 0x78)
    }
    pub fn color_type_name<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0x4e, 0xc9, 0xb0)
    }
    pub fn color_variable_name<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0x9c, 0xdc, 0xfe)
    }
    pub fn color_enum_member<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0x9c, 0xdc, 0xfe)
    }
    pub fn color_url<S: Display>(text: S) -> ColoredString {
        text.to_string().bright_black().underline()
    }
    pub fn color_function<S: Display>(text: S) -> ColoredString {
        text.to_string().truecolor(0xdc, 0xdc, 0xaa)
    }
    pub fn color_info<S: Display>(text: S) -> ColoredString {
        text.to_string().bright_black()
    }
}
