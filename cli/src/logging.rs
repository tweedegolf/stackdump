use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

use env_logger::fmt::{Color, Style, StyledValue};
use log::Level;

pub fn init_logger() {
    env_logger::builder()
        .format(|f, record| {
            use std::io::Write;

            let target = match (record.file(), record.line()) {
                (Some(file), None) => file.into(),
                (Some(file), Some(line)) => format!("{file}:{line}"),
                _ => String::new(),
            };

            let max_width = max_target_width(&target);

            let mut style = f.style();
            let level = colored_level(&mut style, record.level());

            let mut style = f.style();
            let target = style.set_bold(true).value(Padded {
                value: target,
                width: max_width,
            });

            writeln!(f, " {} {} > {}", level, target, record.args())
        })
        .init();
}

struct Padded<T> {
    value: T,
    width: usize,
}

impl<T: fmt::Display> fmt::Display for Padded<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{: <width$}", self.value, width = self.width)
    }
}
fn colored_level<'a>(style: &'a mut Style, level: Level) -> StyledValue<'a, &'static str> {
    match level {
        Level::Trace => style.set_color(Color::Magenta).value("TRACE"),
        Level::Debug => style.set_color(Color::Blue).value("DEBUG"),
        Level::Info => style.set_color(Color::Green).value("INFO "),
        Level::Warn => style.set_color(Color::Yellow).value("WARN "),
        Level::Error => style.set_color(Color::Red).value("ERROR"),
    }
}

fn max_target_width(target: &str) -> usize {
    static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);

    let max_width = MAX_MODULE_WIDTH.load(Ordering::Relaxed);
    if max_width < target.len() {
        MAX_MODULE_WIDTH.store(target.len(), Ordering::Relaxed);
        target.len()
    } else {
        max_width
    }
}
