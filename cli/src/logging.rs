use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

use env_logger::fmt::style::Style;

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

            let level_style = f.default_level_style(record.level());

            let target_style = Style::new().bold();

            writeln!(
                f,
                " {level_style}{}{level_style:#} {target_style}{}{target_style:#} > {}",
                record.level().as_str(),
                Padded {
                    value: target,
                    width: max_width,
                },
                record.args()
            )
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
