use std::fmt::Display;
use colored::{ColoredString, Colorize};

#[derive(Debug, Clone, Copy, strum_macros::Display, strum_macros::EnumString)]
pub enum Theme {
    None,
    Dark,
}

impl ThemeColors for Theme {
    fn color_numeric_value<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_numeric_value(text),
            Theme::Dark => DarkTheme.color_numeric_value(text),
        }
    }

    fn color_invalid<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_invalid(text),
            Theme::Dark => DarkTheme.color_invalid(text),
        }
    }

    fn color_string_value<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_string_value(text),
            Theme::Dark => DarkTheme.color_string_value(text),
        }
    }

    fn color_type_name<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_type_name(text),
            Theme::Dark => DarkTheme.color_type_name(text),
        }
    }

    fn color_variable_name<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_variable_name(text),
            Theme::Dark => DarkTheme.color_variable_name(text),
        }
    }

    fn color_enum_member<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_enum_member(text),
            Theme::Dark => DarkTheme.color_enum_member(text),
        }
    }

    fn color_url<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_url(text),
            Theme::Dark => DarkTheme.color_url(text),
        }
    }

    fn color_function<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_function(text),
            Theme::Dark => DarkTheme.color_function(text),
        }
    }

    fn color_info<S: Display>(&self, text: S) -> ColoredString {
        match self {
            Theme::None => NoTheme.color_info(text),
            Theme::Dark => DarkTheme.color_info(text),
        }
    }
}

pub struct DarkTheme;

impl ThemeColors for DarkTheme {
    fn color_numeric_value<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0xb5, 0xce, 0xa8)
    }
    fn color_invalid<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0xf4, 0x47, 0x47)
    }
    fn color_string_value<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0xce, 0x91, 0x78)
    }
    fn color_type_name<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0x4e, 0xc9, 0xb0)
    }
    fn color_variable_name<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0x9c, 0xdc, 0xfe)
    }
    fn color_enum_member<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0x9c, 0xdc, 0xfe)
    }
    fn color_url<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().bright_black().underline()
    }
    fn color_function<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().truecolor(0xdc, 0xdc, 0xaa)
    }
    fn color_info<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().bright_black()
    }
}

pub struct NoTheme;

impl ThemeColors for NoTheme {
    fn color_numeric_value<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_invalid<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_string_value<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_type_name<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_variable_name<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_enum_member<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_url<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_function<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }

    fn color_info<S: Display>(&self, text: S) -> ColoredString {
        text.to_string().as_str().into()
    }
}

pub trait ThemeColors {
    fn color_numeric_value<S: Display>(&self, text: S) -> ColoredString;
    fn color_invalid<S: Display>(&self, text: S) -> ColoredString;
    fn color_string_value<S: Display>(&self, text: S) -> ColoredString;
    fn color_type_name<S: Display>(&self, text: S) -> ColoredString;
    fn color_variable_name<S: Display>(&self, text: S) -> ColoredString;
    fn color_enum_member<S: Display>(&self, text: S) -> ColoredString;
    fn color_url<S: Display>(&self, text: S) -> ColoredString;
    fn color_function<S: Display>(&self, text: S) -> ColoredString;
    fn color_info<S: Display>(&self, text: S) -> ColoredString;
}
